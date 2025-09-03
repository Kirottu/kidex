use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use kidex_common::{util::{get_index, regenerate_index, reload_config, shutdown_server}, IndexEntry, query::*};

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
    /// Get the index and filters the results
    Find {
        // TODO: Add some CLI arguments:
        // --root <path>
        // --mode <mode> | --regex | --literal | --smart (default)

        /// Case-insensitive search (default: smart-case)
        #[arg(short, long, group = "case")]
        ignore_case: bool,
        /// Case-sensitive search (default: smart-case)
        #[arg(short = 's', long, group = "case")]
        case_sensitive: bool,

        #[arg(short, long, group = "filetype")]
        r#type: Option<ClapFileType>,
        #[arg(short, long, group = "filetype")]
        dirs_only: bool,
        #[arg(short, long, group = "filetype")]
        files_only: bool,

        /// How data should be printed
        #[arg(short, long, value_enum, default_value_t = OutputFormat::Json)]
        output_format: OutputFormat,

        /// Return only the <N> best matches
        #[arg(short, long, value_name = "N")]
        limit: Option<usize>,
        ///
        args: Vec<String>
    },
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum OutputFormat {
    Json,
    List,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum ClapFileType {
    All,
    Files,
    Dirs,
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
        filtered = pick_top_entries(filtered, limit);
        filtered.reverse();
    } else {
        filtered.sort_by_key(|(s, _)| *s);
    }
    filtered.into_iter().map(|p| p.1).collect()
}


fn main() {
    env_logger::init();
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
        Command::Find {
            args, limit, output_format,
            r#type, dirs_only, files_only,
            ignore_case, case_sensitive,
        } => {
            let mut query = Query::from_query_elements(args);

            // Override query settings
            if let Some(t) = r#type {
                query.file_type = match t {
                    ClapFileType::All => FileType::All,
                    ClapFileType::Files => FileType::FilesOnly,
                    ClapFileType::Dirs => FileType::DirOnly,
                }
            }
            if dirs_only {
                query.file_type = FileType::DirOnly;
            }
            if files_only {
                query.file_type = FileType::FilesOnly;
            }

            if ignore_case {
                query.case_option = CaseOption::Ignore;
            }
            if case_sensitive {
                query.case_option = CaseOption::Match;
            }

            let opts = QueryOptions { query, limit, ..Default::default()};
            log::info!("{:?}", opts);

            let index = get_index(None).exit_on_err("Failed to get index");
            let filtered = filter(index, &opts);

            // Print results
            match output_format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&filtered).exit_on_err("Failed to serialize data")
                    );
                },
                OutputFormat::List => {
                    for f in filtered {
                        println!("{}{}",
                            f.path.to_string_lossy(),
                            if f.directory {"/"} else {""}
                        )
                    }
                },
            }
        }
    }
}
