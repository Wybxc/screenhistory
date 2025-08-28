use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use time::format_description::well_known::Rfc3339;
use time::{format_description, Date, OffsetDateTime, Time};

use screenhistory_core as core;

#[derive(Parser, Debug)]
#[command(name = "screenhistory-agent")]
#[command(about = "Headless agent to sync/export Screen Time history", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Sync from macOS Screen Time DB into the local history DB
    Sync(SyncArgs),

    /// Export local history to CSV or JSON
    Export(ExportArgs),
}

#[derive(Args, Debug)]
struct SyncArgs {
    /// Path to the macOS Screen Time knowledge DB (defaults to system path)
    #[arg(long)]
    knowledge_db: Option<PathBuf>,

    /// Path to the local history DB (defaults to ~/.screenhistory.sqlite)
    #[arg(long)]
    local_db: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ExportFormat {
    Csv,
    Json,
}

#[derive(Args, Debug)]
struct ExportArgs {
    /// Path to the local history DB (defaults to ~/.screenhistory.sqlite)
    #[arg(long)]
    local_db: Option<PathBuf>,

    /// Filter: start of time range (RFC3339 like 2025-01-02T03:04:05Z, or date-only 2025-01-02)
    #[arg(long)]
    from: Option<String>,

    /// Filter: end of time range (RFC3339 like 2025-01-02T03:04:05Z, or date-only 2025-01-02)
    #[arg(long)]
    to: Option<String>,

    /// Filter: only export rows for this app name
    #[arg(long)]
    app: Option<String>,

    /// Output format
    #[arg(long, value_enum, default_value_t = ExportFormat::Csv)]
    format: ExportFormat,

    /// Output file path (defaults to stdout if omitted)
    #[arg(long)]
    out: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> ExitCode {
    // Initialize logging if RUST_LOG is set; otherwise keep quiet by default.
    let _ =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("")).try_init();

    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Sync(args) => cmd_sync(args).await,
        Commands::Export(args) => cmd_export(args).await,
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("Error: {err:?}");
            ExitCode::from(1)
        }
    }
}

async fn cmd_sync(args: SyncArgs) -> Result<()> {
    let local_db_path = match args.local_db.as_deref() {
        Some(p) => p.to_path_buf(),
        None => core::default_local_db_path().context("Obtaining default local DB path")?,
    };

    // Ensure local DB exists and is migrated
    let db_path = core::init_db(Some(&local_db_path)).await?;
    let knowledge_path = match args.knowledge_db.as_deref() {
        Some(p) => p.to_path_buf(),
        None => core::default_knowledge_db_path().context("Obtaining default knowledge DB path")?,
    };

    eprintln!("Syncing from {:?} -> {:?}", knowledge_path, db_path);

    match core::sync(Some(&knowledge_path), Some(&db_path)).await {
        Ok(summary) => {
            println!(
                "Sync complete: scanned={}, inserted={}, skipped={}",
                summary.scanned, summary.inserted, summary.skipped
            );
            Ok(())
        }
        Err(e) => {
            // Provide a helpful hint if we failed to open Apple's DB (common FDA issue).
            if default_knowledge_path_matches(&knowledge_path) && likely_permission_error(&e) {
                eprintln!();
                eprintln!("Hint: Full Disk Access may be required to read:");
                eprintln!("      {:?}", knowledge_path);
                eprintln!("Grant Full Disk Access to this binary in System Settings > Privacy & Security > Full Disk Access, then retry.");
            }
            Err(e)
        }
    }
}

async fn cmd_export(args: ExportArgs) -> Result<()> {
    let local_db_path = match args.local_db.as_deref() {
        Some(p) => p.to_path_buf(),
        None => core::default_local_db_path().context("Obtaining default local DB path")?,
    };

    // Build filters
    let filters = core::ExportFilters {
        from: match args.from.as_deref() {
            Some(s) => {
                Some(parse_datetime_or_date(s).with_context(|| format!("Parsing --from '{s}'"))?)
            }
            None => None,
        },
        to: match args.to.as_deref() {
            Some(s) => {
                Some(parse_datetime_or_date(s).with_context(|| format!("Parsing --to '{s}'"))?)
            }
            None => None,
        },
        app: args.app.clone(),
    };

    match args.out.as_deref() {
        Some(path) => {
            let mut file =
                File::create(path).with_context(|| format!("Creating output at {:?}", path))?;
            match args.format {
                ExportFormat::Csv => {
                    core::export_csv(Some(&local_db_path), &filters, &mut file).await?
                }
                ExportFormat::Json => {
                    core::export_json(Some(&local_db_path), &filters, &mut file).await?
                }
            }
            eprintln!("Wrote {:?}", path);
        }
        None => {
            // stdout
            let stdout = io::stdout();
            let mut handle = stdout.lock();
            match args.format {
                ExportFormat::Csv => {
                    core::export_csv(Some(&local_db_path), &filters, &mut handle).await?
                }
                ExportFormat::Json => {
                    core::export_json(Some(&local_db_path), &filters, &mut handle).await?
                }
            }
        }
    }

    Ok(())
}

/* ----------------------------- helpers ----------------------------- */

fn parse_datetime_or_date(s: &str) -> Result<OffsetDateTime> {
    // Try RFC3339 first (e.g., 2025-01-02T03:04:05Z or with offsets)
    if let Ok(dt) = OffsetDateTime::parse(s, &Rfc3339) {
        return Ok(dt);
    }

    // Try "YYYY-MM-DD HH:MM[:SS]" (assume UTC if no offset provided)
    if let Ok(fmt) =
        format_description::parse("[year]-[month]-[day] [hour]:[minute][optional::[::[second]]]")
    {
        if let Ok(date_time) = time::PrimitiveDateTime::parse(s, &fmt) {
            return Ok(date_time.assume_utc());
        }
    }

    // Try date-only "YYYY-MM-DD" (assume midnight UTC)
    if let Ok(fmt) = format_description::parse("[year]-[month]-[day]") {
        if let Ok(date) = Date::parse(s, &fmt) {
            return Ok(date.with_time(Time::MIDNIGHT).assume_utc());
        }
    }

    bail!("Unrecognized datetime format: {s}")
}

fn default_knowledge_path_matches(p: &Path) -> bool {
    core::default_knowledge_db_path()
        .ok()
        .as_deref()
        .map(|d| d == p)
        .unwrap_or(false)
}

fn likely_permission_error(e: &anyhow::Error) -> bool {
    let s = format!("{e:#}");
    // Heuristic checks for common permission/open errors.
    s.contains("permission")
        || s.contains("denied")
        || s.contains("open")
        || s.contains("Operation not permitted")
}
