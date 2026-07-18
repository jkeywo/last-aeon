//! Developer CLI for The Last Aeons.
//!
//! Hosts the workflows CI and developers run against the headless
//! simulation: seeded campaign runs, snapshot inspection, and replay
//! verification. Grows a subcommand per milestone as those systems land.

use std::path::PathBuf;
use std::process::ExitCode;

use aeon_core::calendar::GameDate;
use aeon_core::hash::StateHash;
use aeon_sim::persistence::files;
use aeon_sim::{CampaignConfig, PlayerCommand, SimHost};
use clap::{Args, Parser, Subcommand};

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
    /// Run a seeded headless campaign and print its final state hash.
    Run(RunArgs),
    /// Replay a snapshot plus command log and verify the resulting hash.
    Replay(ReplayArgs),
    /// Verify a snapshot's integrity and print its state hash.
    Hash(HashArgs),
    /// Load and validate the authored content, reporting all findings.
    ValidateContent(ValidateContentArgs),
}

#[derive(Args)]
struct ValidateContentArgs {
    /// Content root directory.
    #[arg(long, default_value = "assets/content")]
    root: PathBuf,
}

#[derive(Args)]
struct RunArgs {
    /// Campaign seed.
    #[arg(long)]
    seed: u64,
    /// Number of days to simulate.
    #[arg(long)]
    days: u32,
    /// Campaign name.
    #[arg(long, default_value = "Headless Campaign")]
    name: String,
    /// Rename the campaign on a given day offset, as OFFSET:NAME.
    /// May repeat. Offsets are days after campaign start, starting at 1.
    #[arg(long = "rename", value_parser = parse_rename)]
    renames: Vec<(u32, String)>,
    /// Write a snapshot to this path.
    #[arg(long)]
    snapshot_out: Option<PathBuf>,
    /// Take the snapshot at this day offset instead of at the end.
    #[arg(long, requires = "snapshot_out")]
    snapshot_at: Option<u32>,
    /// Write the applied-command log (JSONL) to this path.
    #[arg(long)]
    log_out: Option<PathBuf>,
}

#[derive(Args)]
struct ReplayArgs {
    /// Snapshot to restore.
    #[arg(long)]
    snapshot: PathBuf,
    /// Command log to replay (entries after the snapshot date).
    #[arg(long)]
    log: Option<PathBuf>,
    /// Advance to this day offset from the campaign start.
    #[arg(long)]
    to_offset: Option<u32>,
    /// Fail unless the final state hash equals this hex digest.
    #[arg(long)]
    expect_hash: Option<StateHash>,
}

#[derive(Args)]
struct HashArgs {
    /// Snapshot to verify.
    #[arg(long)]
    snapshot: PathBuf,
}

fn parse_rename(value: &str) -> Result<(u32, String), String> {
    let (offset, name) = value
        .split_once(':')
        .ok_or_else(|| "expected OFFSET:NAME".to_owned())?;
    let offset: u32 = offset
        .parse()
        .map_err(|_| format!("invalid day offset '{offset}'"))?;
    if offset == 0 {
        return Err("day offsets start at 1".to_owned());
    }
    if name.is_empty() {
        return Err("rename requires a non-empty name".to_owned());
    }
    Ok((offset, name.to_owned()))
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match dispatch(cli.command) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::from(1)
        }
    }
}

fn dispatch(command: Command) -> Result<ExitCode, String> {
    match command {
        Command::Version => {
            println!("{} {}", aeon_core::GAME_NAME, env!("CARGO_PKG_VERSION"));
            Ok(ExitCode::SUCCESS)
        }
        Command::Run(args) => run(args),
        Command::Replay(args) => replay(args),
        Command::Hash(args) => hash(args),
        Command::ValidateContent(args) => validate_content(args),
    }
}

fn validate_content(args: ValidateContentArgs) -> Result<ExitCode, String> {
    let sources = aeon_data::fs::read_content_dir(&args.root)
        .map_err(|e| format!("reading {}: {e}", args.root.display()))?;
    if sources.is_empty() {
        return Err(format!("no .rhai files under {}", args.root.display()));
    }

    let (set, report) = aeon_data::load_content(&sources);
    for finding in &report.findings {
        println!("{finding}");
    }

    // Loading must be deterministic: a second pass must produce identical
    // data and hash. Divergence means authored content used forbidden
    // stateful behaviour that slipped past the sandbox.
    if let Some(set) = &set {
        let (second, _) = aeon_data::load_content(&sources);
        match second {
            Some(second) if set.data_eq(&second) => {}
            _ => {
                eprintln!("error: content loading is not deterministic across passes");
                return Ok(ExitCode::from(1));
            }
        }
    }

    println!(
        "files: {}  jobs: {}  bodies: {}  provinces: {}  scenario: {}",
        sources.len(),
        set.as_ref().map_or(0, |s| s.jobs.len()),
        set.as_ref().map_or(0, |s| s.bodies.len()),
        set.as_ref().map_or(0, |s| s.provinces.len()),
        set.as_ref()
            .and_then(|s| s.scenario.as_ref())
            .map_or("none".to_owned(), |s| s.key.to_string()),
    );
    match set {
        Some(set) => {
            println!("content-hash: {}", set.content_hash);
            println!("content OK");
            Ok(ExitCode::SUCCESS)
        }
        None => {
            eprintln!("content validation failed");
            Ok(ExitCode::from(1))
        }
    }
}

fn run(args: RunArgs) -> Result<ExitCode, String> {
    let mut renames = args.renames.clone();
    renames.sort_by_key(|(offset, _)| *offset);
    if let Some((offset, _)) = renames.iter().find(|(offset, _)| *offset > args.days) {
        return Err(format!(
            "rename at offset {offset} is beyond the run length {}",
            args.days
        ));
    }

    let mut host = SimHost::new(CampaignConfig {
        name: args.name,
        seed: args.seed,
        start_date: GameDate::EPOCH,
    });

    for offset in 1..=args.days {
        // A command submitted now executes on the next day, i.e. `offset`.
        for (_, name) in renames.iter().filter(|(at, _)| *at == offset) {
            host.submit(PlayerCommand::RenameCampaign { name: name.clone() })
                .map_err(|e| e.to_string())?;
        }
        host.advance_days(1);
        if args.snapshot_at == Some(offset) {
            let path = args.snapshot_out.as_ref().expect("clap enforces");
            files::save_snapshot(path, &host.snapshot()).map_err(|e| e.to_string())?;
        }
    }

    if args.snapshot_at.is_none()
        && let Some(path) = &args.snapshot_out
    {
        files::save_snapshot(path, &host.snapshot()).map_err(|e| e.to_string())?;
    }
    if let Some(path) = &args.log_out {
        files::save_command_log(path, &host.applied_commands()).map_err(|e| e.to_string())?;
    }

    println!("date: {}", host.date());
    println!("state-hash: {}", host.state_hash());
    Ok(ExitCode::SUCCESS)
}

fn replay(args: ReplayArgs) -> Result<ExitCode, String> {
    let snapshot = files::load_snapshot(&args.snapshot).map_err(|e| e.to_string())?;
    let mut host = SimHost::restore(snapshot).map_err(|e| e.to_string())?;
    let snapshot_date = host.date();
    let start_date = {
        // Snapshot state carries the campaign start; recover it for offsets.
        let world = host.world_mut();
        world.resource::<aeon_sim::CampaignClock>().start_date
    };

    let mut target = snapshot_date;
    if let Some(log_path) = &args.log {
        for envelope in files::load_command_log(log_path).map_err(|e| e.to_string())? {
            if envelope.day > snapshot_date {
                if envelope.day > target {
                    target = envelope.day;
                }
                host.submit_recorded(envelope).map_err(|e| e.to_string())?;
            }
        }
    }
    if let Some(offset) = args.to_offset {
        let requested = start_date.add_days(i64::from(offset));
        if requested > target {
            target = requested;
        }
    }

    let days = snapshot_date.days_until(target);
    host.advance_days(days as u32);

    println!("date: {}", host.date());
    println!("state-hash: {}", host.state_hash());

    if let Some(expected) = args.expect_hash {
        if host.state_hash() != expected {
            eprintln!("hash mismatch: expected {expected}");
            return Ok(ExitCode::from(2));
        }
        println!("hash verified");
    }
    Ok(ExitCode::SUCCESS)
}

fn hash(args: HashArgs) -> Result<ExitCode, String> {
    let snapshot = files::load_snapshot(&args.snapshot).map_err(|e| e.to_string())?;
    let host = SimHost::restore(snapshot).map_err(|e| e.to_string())?;
    println!("date: {}", host.date());
    println!("state-hash: {}", host.state_hash());
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_specs_parse() {
        assert_eq!(
            parse_rename("15:New Name").unwrap(),
            (15, "New Name".to_owned())
        );
        assert_eq!(
            parse_rename("3:with:colons").unwrap(),
            (3, "with:colons".to_owned())
        );
        assert!(parse_rename("nope").is_err());
        assert!(parse_rename("0:name").is_err());
        assert!(parse_rename("5:").is_err());
    }
}
