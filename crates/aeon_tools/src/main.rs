//! Developer CLI for The Last Aeons.
//!
//! Hosts the workflows CI and developers run against the headless
//! simulation: content validation, seeded campaign runs (content-free or
//! on the authored scenario), replay verification, and an end-to-end
//! acceptance round-trip.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use aeon_core::calendar::GameDate;
use aeon_core::hash::StateHash;
use aeon_data::ContentSet;
use aeon_sim::persistence::files;
use aeon_sim::{CampaignConfig, CampaignOver, PlayerCommand, PoliticsIndex, SimHost};
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
    /// Run the authored scenario, snapshot it mid-run, replay from the
    /// snapshot, and verify the replayed hash matches the original.
    Accept(AcceptArgs),
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
    /// Run the authored scenario from this content directory instead of a
    /// content-free campaign.
    #[arg(long)]
    content: Option<PathBuf>,
    /// Campaign name (ignored when a scenario supplies one).
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
    /// Content directory the snapshot was taken against, if any.
    #[arg(long)]
    content: Option<PathBuf>,
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
    /// Content directory the snapshot was taken against, if any.
    #[arg(long)]
    content: Option<PathBuf>,
}

#[derive(Args)]
struct AcceptArgs {
    /// Content root directory.
    #[arg(long, default_value = "assets/content")]
    content: PathBuf,
    /// Campaign seed.
    #[arg(long, default_value_t = 0xA301)]
    seed: u64,
    /// Total days to simulate.
    #[arg(long, default_value_t = 3600)]
    days: u32,
    /// Day offset at which to snapshot before replaying the remainder.
    #[arg(long, default_value_t = 1200)]
    snapshot_at: u32,
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
        Command::Accept(args) => accept(args),
    }
}

/// Loads and validates an authored content set, failing on any error.
fn load_content_set(dir: &Path) -> Result<Arc<ContentSet>, String> {
    let sources = aeon_data::fs::read_content_dir(dir)
        .map_err(|e| format!("reading {}: {e}", dir.display()))?;
    if sources.is_empty() {
        return Err(format!("no .rhai files under {}", dir.display()));
    }
    let (set, report) = aeon_data::load_content(&sources);
    set.ok_or_else(|| {
        let errors: Vec<String> = report.findings.iter().map(|f| f.to_string()).collect();
        format!("content failed validation:\n{}", errors.join("\n"))
    })
    .map(Arc::new)
}

/// Config from a content set's authored scenario (or a default).
fn scenario_config(content: &ContentSet, seed: u64, fallback_name: &str) -> CampaignConfig {
    match &content.scenario {
        Some(scenario) => CampaignConfig {
            name: scenario.name.clone(),
            seed,
            start_date: aeon_core::calendar::CalendarDate {
                year: scenario.start_year,
                month: scenario.start_month,
                day: scenario.start_day,
            }
            .to_date()
            .expect("validated scenario start date"),
        },
        None => CampaignConfig {
            name: fallback_name.to_owned(),
            seed,
            start_date: GameDate::EPOCH,
        },
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

    let mut host = match &args.content {
        Some(dir) => {
            let content = load_content_set(dir)?;
            let config = scenario_config(&content, args.seed, &args.name);
            SimHost::new_with_content(config, content)
        }
        None => SimHost::new(CampaignConfig {
            name: args.name.clone(),
            seed: args.seed,
            start_date: GameDate::EPOCH,
        }),
    };

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

fn restore(snapshot_path: &Path, content: &Option<PathBuf>) -> Result<SimHost, String> {
    let snapshot = files::load_snapshot(snapshot_path).map_err(|e| e.to_string())?;
    match content {
        Some(dir) => {
            let content = load_content_set(dir)?;
            SimHost::restore_with_content(snapshot, content).map_err(|e| e.to_string())
        }
        None => SimHost::restore(snapshot).map_err(|e| e.to_string()),
    }
}

fn replay(args: ReplayArgs) -> Result<ExitCode, String> {
    let mut host = restore(&args.snapshot, &args.content)?;
    let snapshot_date = host.date();
    let start_date = host
        .world_mut()
        .resource::<aeon_sim::CampaignClock>()
        .start_date;

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
    let host = restore(&args.snapshot, &args.content)?;
    println!("date: {}", host.date());
    println!("state-hash: {}", host.state_hash());
    Ok(ExitCode::SUCCESS)
}

/// The end-to-end acceptance round-trip: run the authored scenario to the
/// end, snapshot at a mid-point, restore that snapshot, replay the
/// remaining days, and confirm the replayed final hash matches the
/// original — proving deterministic seed-plus-command replay on the real
/// scenario.
fn accept(args: AcceptArgs) -> Result<ExitCode, String> {
    if args.snapshot_at >= args.days {
        return Err("snapshot_at must be before the total days".to_owned());
    }
    let content = load_content_set(&args.content)?;
    let config = scenario_config(&content, args.seed, "Acceptance Run");

    println!("scenario: {}", config.name);
    println!("seed: {:#x}  days: {}", args.seed, args.days);

    // Full run, capturing the mid-point snapshot.
    let mut original = SimHost::new_with_content(config.clone(), content.clone());
    original.advance_days(args.snapshot_at);
    let mid_snapshot = original.snapshot();
    let mid_hash = original.state_hash();
    original.advance_days(args.days - args.snapshot_at);
    let final_hash = original.state_hash();
    let final_date = original.date();

    // A completed player campaign is a legitimate outcome; note it.
    let over = original
        .world_mut()
        .get_resource::<CampaignOver>()
        .map(|o| o.reason.clone());

    // Replay from the mid-point snapshot to the end.
    let mut replayed = SimHost::restore_with_content(mid_snapshot, content)
        .map_err(|e| format!("restoring snapshot: {e}"))?;
    if replayed.state_hash() != mid_hash {
        eprintln!("snapshot restore hash mismatch");
        return Ok(ExitCode::from(2));
    }
    replayed.advance_days(args.days - args.snapshot_at);

    let survivors = replayed
        .world_mut()
        .resource::<PoliticsIndex>()
        .characters
        .len();

    println!("mid-hash:   {mid_hash}");
    println!("final-date: {final_date}");
    println!("final-hash: {final_hash}");
    println!("replay-hash:{}", replayed.state_hash());
    println!("characters: {survivors}");
    if let Some(reason) = over {
        println!("campaign-over: {reason}");
    }

    if replayed.state_hash() == final_hash {
        println!("ACCEPTANCE OK — replay reproduced the final state exactly");
        Ok(ExitCode::SUCCESS)
    } else {
        eprintln!("ACCEPTANCE FAILED — replay diverged from the original run");
        Ok(ExitCode::from(2))
    }
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
