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
    Query { str: String },
    Find { args: Vec<String> },
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

trait ToSaneString {
    fn to_string_safe(&self) -> &str;
}

impl ToSaneString for std::ffi::OsStr {
    fn to_string_safe(&self) -> &str {
        &self.to_str().expect("Path with invalid unicode found")
    }
}
impl ToSaneString for std::path::Path {
    fn to_string_safe(&self) -> &str {
        &self.to_str().expect("Path with invalid unicode found")
    }
}

// Frontend searching. Searches the received index
pub fn filter(index: Vec<IndexEntry>, query_opts: &QueryOptions) -> Vec<IndexEntry> {
    let mut filtered: Vec<(i64,IndexEntry)> = index
        .into_iter()
        .filter_map(|entry| {
            let score = calc_score(&query_opts.query, &entry);
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
        Command::Query { str } => {
            let index = query_index(QueryOptions::default()).exit_on_err("Failed to get index");
            println!(
                "{}",
                serde_json::to_string_pretty(&index).exit_on_err("Failed to serialize data")
            );
        }
        Command::Find { args } => {
            let index = get_index(None).exit_on_err("Failed to get index");
            let opts = QueryOptions { query: Query::from_query_elements(args), ..Default::default()};
            let filtered = filter(index, &opts);
            println!(
                "{}",
                serde_json::to_string_pretty(&filtered).exit_on_err("Failed to serialize data")
            );
        }
    }
}
