use std::path::PathBuf;

use clap::{Parser, Subcommand};
use kidex_common::{util::{get_index, query_index, regenerate_index, reload_config, shutdown_server}, IndexEntry, query::*};

#[derive(Parser)]
struct Opts {
    #[command(subcommand)]
    subcommand: Command,
}

#[derive(Subcommand)]
enum Command {
    Shutdown,
    ReloadConfig,
    RegenerateIndex,
    GetIndex { path: Option<PathBuf> },
    Query { args: Vec<String> },
    Find {
        // TODO: Add some CLI arguments:
        // --limit <amount>
        // --root <path>
        // --filetype (dir|file)
        // --mode <mode> | --regex | --literal | --smart (default)
        args: Vec<String>
    },
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
    filtered.sort_by_key(|(s, _)| *s);
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
        Command::Find { args } => {
            let query = Query::from_query_elements(args);
            let opts = QueryOptions { query, ..Default::default()};

            let index = get_index(None).exit_on_err("Failed to get index");
            let filtered = filter(index, &opts);
            println!(
                "{}",
                serde_json::to_string_pretty(&filtered).exit_on_err("Failed to serialize data")
            );
        }
    }
}
