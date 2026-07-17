//! Developer CLI for The Last Aeons.
//!
//! Hosts the workflows CI and developers run against the headless
//! simulation: content validation, seeded campaign runs, and replay
//! verification. Grows a subcommand per milestone as those systems land.

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "aeon", version, about = "The Last Aeons developer tools")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the engine and game version.
    Version,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Version => {
            println!("{} {}", aeon_core::GAME_NAME, env!("CARGO_PKG_VERSION"));
        }
    }
}
