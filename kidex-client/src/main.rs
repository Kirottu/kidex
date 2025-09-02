use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use kidex_common::{util::{get_index, query_index, regenerate_index, reload_config, shutdown_server}, IndexEntry, query::*};

#[derive(Parser)]
#[command(version, about)]
struct Opts {
    #[command(subcommand)]
    subcommand: Command,
}

#[derive(Subcommand)]
enum Command {
    Shutdown,
    ReloadConfig,
    RegenerateIndex,
    /// Return the entire index
    GetIndex { path: Option<PathBuf> },
    /// Queries the kidex daemon to return filtered results
    Query { args: Vec<String> },
    /// Get the index and filters the results
    Find {
        // TODO: Add some CLI arguments:
        // --root <path>
        // --mode <mode> | --regex | --literal | --smart (default)

        #[arg(long, group = "filetype")]
        r#type: Option<ClapFileType>,
        #[arg(short, long, group = "filetype")]
        dirs_only: bool,
        #[arg(short, long, group = "filetype")]
        files_only: bool,

        /// Return only the <N> best matches
        #[arg(short, long, value_name = "N")]
        limit: Option<usize>,
        ///
        args: Vec<String>
    },
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum ClapFileType {
    All,
    FilesOnly,
    DirOnly,
}

trait ExitWithError<T> {
    fn exit_on_err(self, msg: &str) -> T;
}
impl<T, E> ExitWithError<T> for Result<T, E>
where E: std::error::Error
{
    #[allow(unreachable_code)]
    fn exit_on_err(self, msg: &str) -> T {
        match self {
            Err(e)=> {
                println!("[Error] {}: {}", msg, e);
                std::process::exit(-1);
                self.unwrap()
            },
            a => a.unwrap()
        }
    }
}

// Frontend searching. Searches the received index
pub fn filter(index: Vec<IndexEntry>, query_opts: &QueryOptions) -> Vec<IndexEntry> {
    let mut filtered: Vec<(i64,IndexEntry)> = index
        .into_iter()
        .filter_map(|entry| {
            let score = calc_score(&query_opts.query, &entry.path, entry.directory);
            if score > 0 { Some((score, entry)) } else { None }
        })
        .collect();

    if let Some(limit) = query_opts.limit {
        if filtered.len() <= limit {
            // Result amount is below the limit, just sort it then
            filtered.sort_by_key(|(s, _)| *s);
        } else {
            // Pick the top n most highest ranked entries
            let mut top: Vec<(i64, IndexEntry)> = Vec::new();
            for (scr, entry) in filtered {
                // Ignore score if it's worse than the worst one so far
                let worst = top.get(limit).map_or(0i64, |f| f.0);
                if top.len() >= limit && scr <= worst {
                    continue;
                }
                let index = top.partition_point(|&(i, _)| i >= scr);
                top.insert(index, (scr, entry.to_owned()));
            }
            // Cut 
            if top.len() > limit {
                top = top[..limit].to_vec()
            }
            top.reverse();
            filtered = top;
        }
    } else {
        // Biggest score last!
        filtered.sort_by_key(|(s, _)| *s);
    }
    filtered.into_iter().map(|p| p.1).collect()
}


fn main() {
    let opts = Opts::parse();

    match opts.subcommand {
        Command::Shutdown => {
            shutdown_server().exit_on_err("Failed to shut down server");
            println!("Success!");
        }
        Command::ReloadConfig => {
            reload_config().exit_on_err("Failed to reload config");
            println!("Success!");
        }
        Command::RegenerateIndex => {
            regenerate_index().exit_on_err("Failed to regenerate index");
            println!("Success!");
        }
        Command::GetIndex { path } => {
            let index = get_index(path).exit_on_err("Failed to get index");
            println!(
                "{}",
                serde_json::to_string_pretty(&index).exit_on_err("Failed to serialize data")
            );
        }
        Command::Query { args } => {
            let query = Query::from_query_elements(args);
            let opts = QueryOptions { query, ..Default::default()};

            let index = query_index(opts).exit_on_err("Failed to query index");
            println!(
                "{}",
                serde_json::to_string_pretty(&index).exit_on_err("Failed to serialize data")
            );
        }
        Command::Find { args, limit, r#type, dirs_only, files_only } => {
            let file_type = if let Some(t) = r#type {
                match t {
                    ClapFileType::All => FileType::All,
                    ClapFileType::FilesOnly => FileType::FilesOnly,
                    ClapFileType::DirOnly => FileType::DirOnly,
                }
            } else { FileType::All };

            let mut query = Query::from_query_elements(args);
            query.file_type = file_type;
            let opts = QueryOptions { query, limit, ..Default::default()};

            let index = get_index(None).exit_on_err("Failed to get index");
            let filtered = filter(index, &opts);
            println!(
                "{}",
                serde_json::to_string_pretty(&filtered).exit_on_err("Failed to serialize data")
            );
        }
    }
}
