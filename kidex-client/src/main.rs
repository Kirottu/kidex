use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use kidex_common::{util::{get_index, query_index, regenerate_index, reload_config, shutdown_server}, IndexEntry, QueryOptions};

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
    Find { str: String },
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

fn calc_score(query: &str, entry: &IndexEntry) -> i64 {
    let path = entry.path.parent().unwrap_or(Path::new("/")).to_string_lossy();
    let basename  = entry.path.file_name().unwrap_or_default().to_string_lossy();
    let mut score: i64 = -1;
    if basename.contains(query) { 
        score += 100 * query.len() as i64;
    }
    // Check if it's in the path
    let mut backdepth = 21;
    for p in entry.path.components().rev() {
        if p.as_os_str()
            .to_string_safe()
            .contains(query)
        {
            score+=backdepth;
        }
        backdepth -= 3;
    }
    return score;
}

// Frontend searching. Searches the received index
pub fn filter(index: Vec<IndexEntry>, query_string: &str) -> Vec<IndexEntry> {
    let mut filtered: Vec<(i64,IndexEntry)> = index
        .into_iter()
        .filter_map(|entry| {
            let score = calc_score(query_string, &entry);
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
            let index = query_index(QueryOptions::from_str(&str)).exit_on_err("Failed to get index");
            println!(
                "{}",
                serde_json::to_string_pretty(&index).exit_on_err("Failed to serialize data")
            );
        }
        Command::Find { str } => {
            let index = get_index(None).exit_on_err("Failed to get index");
            let filtered = filter(index, &str);
            println!(
                "{}",
                serde_json::to_string_pretty(&filtered).exit_on_err("Failed to serialize data")
            );
        }
    }
}
