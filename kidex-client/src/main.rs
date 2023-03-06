use std::path::PathBuf;

use clap::{Parser, Subcommand};
use kidex_common::util::{get_index, regenerate_index, reload_config, shutdown_server};

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
}

fn main() {
    let opts = Opts::parse();

    match opts.subcommand {
        Command::Shutdown => {
            shutdown_server().expect("Failed to shut down server");
            println!("Success!");
        }
        Command::ReloadConfig => {
            reload_config().expect("Failed to reload config");
            println!("Success!");
        }
        Command::RegenerateIndex => {
            regenerate_index().expect("Failed to regenerate index");
            println!("Success!");
        }
        Command::GetIndex { path } => {
            let index = get_index(path).expect("Failed to get index");
            println!(
                "{}",
                serde_json::to_string_pretty(&index).expect("Failed to serialize data")
            );
        }
    }
}
