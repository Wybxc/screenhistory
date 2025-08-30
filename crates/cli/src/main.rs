use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use time::format_description::well_known::Rfc3339;
use time::{format_description, Date, OffsetDateTime, Time};

use screenhistory_core as core;

const DEFAULT_LABEL: &str = "com.mikkelam.screenhistory";

#[derive(Parser, Debug)]
#[command(name = "screenhistory")]
#[command(about = "Headless CLI to sync/export Screen Time history", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Sync from macOS Screen Time DB into the local history DB
    #[command(alias = "s")]
    Sync(SyncArgs),

    /// Export local history to CSV or JSON
    #[command(alias = "e")]
    Export(ExportArgs),

    /// Manage launchd scheduling for periodic sync
    #[command(alias = "sch")]
    Schedule(ScheduleArgs),
}

#[derive(Args, Debug)]
struct SyncArgs {
    /// Path to the macOS Screen Time knowledge DB (defaults to system path)
    #[arg(long)]
    knowledge_db: Option<PathBuf>,

    /// Path to the local history DB (defaults to ~/.screenhistory.sqlite)
    #[arg(long)]
    local_db: Option<PathBuf>,

    /// Verbose output (show source/target paths)
    #[arg(short, long)]
    verbose: bool,
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

#[derive(Args, Debug)]
struct ScheduleArgs {
    #[command(subcommand)]
    action: ScheduleAction,
}

#[derive(Subcommand, Debug)]
enum ScheduleAction {
    Install(InstallArgs),
    Uninstall(LabelOnly),
    Status(LabelOnly),
    RunNow(LabelOnly),
}

#[derive(Args, Debug)]
struct LabelOnly {
    /// Label of the LaunchAgent
    #[arg(long)]
    label: Option<String>,
}

#[derive(Args, Debug)]
struct InstallArgs {
    /// LaunchAgent label
    #[arg(long)]
    label: Option<String>,

    /// Interval like "15m", "1h", or seconds (e.g., "900"). Conflicts with --at.
    #[arg(long, conflicts_with = "at")]
    every: Option<String>,

    /// Daily time in 24h "HH:MM". Conflicts with --every.
    #[arg(long, conflicts_with = "every")]
    at: Option<String>,

    /// Also run immediately after load
    #[arg(long)]
    run_at_load: bool,

    /// Forwarded to `sync`
    #[arg(long)]
    local_db: Option<PathBuf>,

    /// Forwarded to `sync`
    #[arg(long)]
    knowledge_db: Option<PathBuf>,

    /// Log file path (defaults to ~/Library/Logs/screenhistory/sync.log)
    #[arg(long)]
    log_file: Option<PathBuf>,
}

async fn cmd_schedule(args: ScheduleArgs) -> Result<()> {
    match args.action {
        ScheduleAction::Install(a) => {
            let label = a.label.unwrap_or_else(|| DEFAULT_LABEL.to_string());

            let (start_interval, start_calendar) = if let Some(every) = a.every.as_deref() {
                (Some(parse_duration_secs(every)?), None)
            } else if let Some(at) = a.at.as_deref() {
                (None, Some(parse_hhmm(at)?))
            } else {
                // default
                (Some(3600_u64), None)
            };

            let exe = std::env::current_exe().context("Locating current executable")?;
            let mut program_args: Vec<String> = Vec::new();
            program_args.push(exe.to_string_lossy().to_string());
            program_args.push("sync".to_string());
            if let Some(p) = a.local_db.as_deref() {
                program_args.push("--local_db".to_string()); // clap converts '--local-db' to 'local_db' arg; ProgramArguments get passed to our CLI unchanged, so use long form with dash
                                                             // Use the exact flag expected by our CLI:
                program_args.pop();
                program_args.push("--local-db".to_string());
                program_args.push(p.to_string_lossy().to_string());
            }
            if let Some(p) = a.knowledge_db.as_deref() {
                program_args.push("--knowledge_db".to_string());
                // Use the exact flag expected by our CLI:
                program_args.pop();
                program_args.push("--knowledge-db".to_string());
                program_args.push(p.to_string_lossy().to_string());
            }

            let log_path = match a.log_file {
                Some(p) => p,
                None => default_log_path().context("Preparing default log directory")?,
            };

            let plist = render_plist(
                &label,
                &program_args,
                a.run_at_load,
                start_interval,
                start_calendar,
                &log_path,
            );

            let plist_path = plist_path_for_label(&label)?;
            if let Some(parent) = plist_path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("Creating directory {}", parent.display()))?;
            }
            fs::write(&plist_path, plist.as_bytes())
                .with_context(|| format!("Writing {}", plist_path.display()))?;

            let uid = get_uid()?;
            run_launchctl(&[
                "bootstrap",
                &format!("gui/{uid}"),
                &plist_path.to_string_lossy(),
            ])?;
            run_launchctl(&["enable", &format!("gui/{uid}/{}", label)])?;

            eprintln!("Installed LaunchAgent: {}", plist_path.display());
            eprintln!("Label: {}", label);
            eprintln!(
                "Note: If Full Disk Access is required, grant it to: {}",
                exe.display()
            );

            Ok(())
        }
        ScheduleAction::Uninstall(o) => {
            let label = o.label.unwrap_or_else(|| DEFAULT_LABEL.to_string());
            let uid = get_uid()?;
            // Best-effort bootout
            let _ = run_launchctl(&["bootout", &format!("gui/{uid}/{}", label)]);
            let plist_path = plist_path_for_label(&label)?;
            if plist_path.exists() {
                fs::remove_file(&plist_path)
                    .with_context(|| format!("Removing {}", plist_path.display()))?;
            }
            eprintln!("Uninstalled LaunchAgent: {}", label);
            Ok(())
        }
        ScheduleAction::Status(o) => {
            let label = o.label.unwrap_or_else(|| DEFAULT_LABEL.to_string());
            let uid = get_uid()?;
            run_launchctl(&["print", &format!("gui/{uid}/{}", label)])
        }
        ScheduleAction::RunNow(o) => {
            let label = o.label.unwrap_or_else(|| DEFAULT_LABEL.to_string());
            let uid = get_uid()?;
            run_launchctl(&["kickstart", "-k", &format!("gui/{uid}/{}", label)])
        }
    }
}

fn parse_duration_secs(s: &str) -> Result<u64> {
    let t = s.trim();
    if let Some(rest) = t.strip_suffix('m') {
        let v: u64 = rest.trim().parse().context("Parsing minutes")?;
        return Ok(v * 60);
    }
    if let Some(rest) = t.strip_suffix('h') {
        let v: u64 = rest.trim().parse().context("Parsing hours")?;
        return Ok(v * 3600);
    }
    let v: u64 = t.parse().context("Parsing seconds")?;
    Ok(v)
}

fn parse_hhmm(s: &str) -> Result<(u32, u32)> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        bail!("Time must be HH:MM (24h)");
    }
    let hour: u32 = parts[0].parse().context("Parsing hour")?;
    let minute: u32 = parts[1].parse().context("Parsing minute")?;
    if hour > 23 || minute > 59 {
        bail!("Time out of range (HH: 0-23, MM: 0-59)");
    }
    Ok((hour, minute))
}

fn get_uid() -> Result<String> {
    let out = Command::new("id")
        .arg("-u")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .context("Running id -u")?;
    if !out.status.success() {
        bail!("Failed to obtain UID via `id -u`");
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn launch_agents_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("Resolving HOME")?;
    Ok(PathBuf::from(home).join("Library").join("LaunchAgents"))
}

fn default_log_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("Resolving HOME")?;
    let dir = PathBuf::from(home)
        .join("Library")
        .join("Logs")
        .join("screenhistory");
    fs::create_dir_all(&dir).with_context(|| format!("Creating {}", dir.display()))?;
    Ok(dir.join("sync.log"))
}

fn plist_path_for_label(label: &str) -> Result<PathBuf> {
    Ok(launch_agents_dir()?.join(format!("{label}.plist")))
}

fn render_plist(
    label: &str,
    program_args: &[String],
    run_at_load: bool,
    start_interval: Option<u64>,
    start_calendar: Option<(u32, u32)>,
    log_path: &Path,
) -> String {
    let mut args_xml = String::new();
    for a in program_args {
        args_xml.push_str(&format!("        <string>{}</string>\n", a));
    }

    let schedule_xml = if let Some(secs) = start_interval {
        format!(
            "    <key>StartInterval</key>\n    <integer>{}</integer>\n",
            secs
        )
    } else if let Some((hour, minute)) = start_calendar {
        format!(
            "    <key>StartCalendarInterval</key>\n    <dict>\n      <key>Hour</key>\n      <integer>{}</integer>\n      <key>Minute</key>\n      <integer>{}</integer>\n    </dict>\n",
            hour, minute
        )
    } else {
        String::new()
    };

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
  <dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
{args_xml}    </array>
    <key>RunAtLoad</key>
    <{run_at_load}/>
    <key>KeepAlive</key>
    <false/>
    <key>StandardOutPath</key>
    <string>{log}</string>
    <key>StandardErrorPath</key>
    <string>{log}</string>
{schedule}  </dict>
</plist>
"#,
        label = label,
        args_xml = args_xml,
        run_at_load = if run_at_load { "true" } else { "false" },
        log = log_path.display(),
        schedule = schedule_xml
    )
}

fn run_launchctl(args: &[&str]) -> Result<()> {
    let status = Command::new("launchctl")
        .args(args)
        .status()
        .with_context(|| format!("Running: launchctl {}", args.join(" ")))?;
    if !status.success() {
        bail!("launchctl failed: {}", args.join(" "));
    }
    Ok(())
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
        Commands::Schedule(args) => cmd_schedule(args).await,
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

    if args.verbose {
        eprintln!(
            "🔄 Syncing from {} -> {}",
            knowledge_path.display(),
            db_path.display()
        );
    } else {
        eprintln!("🔄 Syncing…");
    }

    match core::sync(Some(&knowledge_path), Some(&db_path)).await {
        Ok(summary) => {
            println!(
                "✅ Sync complete: scanned={}, inserted={}, skipped={}",
                summary.scanned, summary.inserted, summary.skipped
            );
            Ok(())
        }
        Err(e) => {
            // Provide a helpful hint if we failed to open Apple's DB (common FDA issue).
            if default_knowledge_path_matches(&knowledge_path) && likely_permission_error(&e) {
                eprintln!();
                eprintln!("💡 Full Disk Access may be required to read:");
                eprintln!("   {}", knowledge_path.display());
                eprintln!("   Grant Full Disk Access to this binary in System Settings › Privacy & Security › Full Disk Access, then retry.");
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
