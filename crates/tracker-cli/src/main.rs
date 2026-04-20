use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::Serialize;
use tracker_core::{discovery, hooks, ingest, sync};

#[derive(Parser)]
#[command(name = "tracker-cli", version, about = "Claude Code project tracker CLI")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Called by Claude Code hooks. Reads event JSON from stdin.
    Ingest {
        /// Event name, e.g. SessionStart, UserPromptSubmit.
        event: String,
    },
    /// Scan ~/.claude/projects/ and insert any projects not yet in the DB.
    Discover,
    /// Re-enrich every known project (git, launch, deploy detection).
    Sync {
        /// Ignore the 1-hour per-project cache.
        #[arg(long)]
        force: bool,
        /// Enable opt-in live deploy URL lookup for projects that have it on.
        #[arg(long)]
        live_lookup: bool,
    },
    /// Print every known project as a simple TSV.
    List,
    /// Print the most-recently-active projects as JSON. Used by the
    /// `/claude-tracker:recent` slash command.
    Recent {
        /// Maximum number of projects to return.
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Print the current hook-install status and DB path.
    Doctor,
    /// Install global Claude Code hooks that call this binary.
    InstallHooks {
        /// Override the binary path embedded in the hook command.
        /// Defaults to the absolute path of the currently-running binary.
        #[arg(long)]
        cli: Option<PathBuf>,
    },
    /// Remove tracker hooks from ~/.claude/settings.json.
    UninstallHooks,
}

fn main() {
    // tracker-cli ingest must never crash loudly — we silently exit 0 on any
    // error inside the ingest path and log to file.
    let cli = Cli::parse();
    match &cli.cmd {
        Cmd::Ingest { event } => {
            if let Err(e) = ingest::ingest_from_stdin(event) {
                log_err("ingest", &e);
            }
            // Always succeed from Claude Code's perspective.
            std::process::exit(0);
        }
        other => {
            if let Err(e) = run(other) {
                eprintln!("error: {e:#}");
                std::process::exit(1);
            }
        }
    }
}

fn run(cmd: &Cmd) -> Result<()> {
    init_tracing();
    let db = tracker_core::open_db()?;
    match cmd {
        Cmd::Ingest { .. } => unreachable!("handled earlier"),
        Cmd::Discover => {
            let (total, added) = discovery::discover_all(&db)?;
            println!("discovered {total} project(s); added {added} new");
        }
        Cmd::Sync { force, live_lookup } => {
            let n = sync::sync_all(
                &db,
                &sync::SyncOpts {
                    force: *force,
                    allow_live_lookup: *live_lookup,
                },
            )?;
            println!("synced {n} project(s)");
        }
        Cmd::List => {
            let projects = db.list_projects(true)?;
            let mut out = std::io::stdout().lock();
            writeln!(
                out,
                "NAME\tSTATUS\tLAST_ACTIVE\tSESSIONS\tPROMPTS\tGITHUB\tPATH"
            )?;
            for p in projects {
                let status = p.status.clone().unwrap_or_else(|| "-".to_string());
                let last = p
                    .last_active_at
                    .map(|t| t.to_rfc3339())
                    .unwrap_or_else(|| "-".into());
                let gh = p.github_url.clone().unwrap_or_default();
                writeln!(
                    out,
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    p.name, status, last, p.sessions_started, p.prompts_count, gh, p.path
                )?;
            }
        }
        Cmd::Recent { limit } => {
            let projects = db.recent_active(*limit)?;
            let out: Vec<RecentEntry> = projects.into_iter().map(RecentEntry::from).collect();
            println!("{}", serde_json::to_string(&out)?);
        }
        Cmd::Doctor => {
            let status = hooks::status()?;
            println!("settings.json: {}", status.settings_path.display());
            println!("hooks installed: {:?}", status.installed_events);
            if let Some(p) = &status.cli_path {
                println!("cli path registered in hooks: {}", p.display());
            }
            println!(
                "db: {} (projects: {})",
                tracker_core::paths::tracker_db()?.display(),
                db.list_projects(true)?.len()
            );
        }
        Cmd::InstallHooks { cli } => {
            let cli_path = match cli {
                Some(p) => p.clone(),
                None => std::env::current_exe().context("reading current binary path")?,
            };
            let status = hooks::install(&cli_path)?;
            println!(
                "installed hooks for events: {:?}",
                status.installed_events
            );
            println!("settings.json: {}", status.settings_path.display());
        }
        Cmd::UninstallHooks => {
            let status = hooks::uninstall()?;
            println!(
                "remaining tracker-installed events: {:?} (should be empty)",
                status.installed_events
            );
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct RecentEntry {
    name: String,
    path: String,
    last_active_at: Option<String>,
    sessions_started: i64,
    prompts_count: i64,
    effective_status: String,
}

impl From<tracker_core::db::Project> for RecentEntry {
    fn from(p: tracker_core::db::Project) -> Self {
        let effective = if p.status_manual {
            p.status
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        } else {
            None
        }
        .unwrap_or_else(|| {
            tracker_core::status::infer(&tracker_core::status::StatusInputs {
                last_active_at: p.last_active_at,
                deploy_url: p.deploy_url.as_deref(),
                archived_at: p.archived_at,
                now: chrono::Utc::now(),
            })
            .as_str()
            .to_string()
        });
        RecentEntry {
            name: p.name,
            path: p.path,
            last_active_at: p.last_active_at.map(|t| t.to_rfc3339()),
            sessions_started: p.sessions_started,
            prompts_count: p.prompts_count,
            effective_status: effective,
        }
    }
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();
}

fn log_err(ctx: &str, e: &anyhow::Error) {
    tracker_core::paths::append_log("cli.log", &format!("{ctx}: {e:#}"));
}
