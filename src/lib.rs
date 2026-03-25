#![forbid(unsafe_code)]

//! Core library facade for ScreenHistory.
//!
//! This crate exposes a small, async-first API for:
//! - Initializing/migrating the local database
//! - Incremental sync from Apple's Screen Time (Knowledge) database
//! - Exporting usage data (CSV/JSON)
//!
//! Internals are split into focused modules (paths, db, ingest, export, models).
//! This lib.rs re-exports the primary domain types and provides thin async
//! functions that delegate to those modules.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Result;
use time::OffsetDateTime;

mod db;
mod export;
mod ingest;
pub mod models;
pub mod paths;

pub use models::rows::{KnowledgeUsageRow, LocalUsageRow};
pub use models::{ExportFilters, SyncSummary, UsageRecord};

/// Default local DB path: ~/.screenhistory.sqlite
pub fn default_local_db_path() -> Result<PathBuf> {
    paths::default_local_db_path()
}

/// Default Apple Screen Time DB path:
/// ~/Library/Application Support/Knowledge/knowledgeC.db
pub fn default_knowledge_db_path() -> Result<PathBuf> {
    paths::default_knowledge_db_path()
}

/// Create (if needed) and migrate the local DB. Returns the resolved path.
///
/// - When `path` is None, uses [`default_local_db_path`].
pub async fn init_db(path: Option<&Path>) -> Result<PathBuf> {
    let local = match path {
        Some(p) => p.to_path_buf(),
        None => paths::default_local_db_path()?,
    };
    let mut conn = db::open_local_rw(&local).await?;
    db::migrate(&mut conn).await?;
    Ok(local)
}

/// Incremental sync from Apple's Screen Time DB into the local DB.
///
/// Watermarking is based on the maximum known `event_id`. Inserts are
/// performed with `INSERT OR IGNORE` for idempotency.
///
/// - `knowledge_db_path`: optional override for the Apple DB path
/// - `local_db_path`: optional override for the local DB path
pub async fn sync(
    knowledge_db_path: Option<&Path>,
    local_db_path: Option<&Path>,
) -> Result<SyncSummary> {
    let default_knowledge = paths::default_knowledge_db_path()?;
    let default_local = paths::default_local_db_path()?;
    ingest::sync_impl(
        knowledge_db_path,
        local_db_path,
        &default_knowledge,
        &default_local,
    )
    .await
}

/// Latest end time (as OffsetDateTime) present in the local DB, if any.
pub async fn last_sync(local_db_path: Option<&Path>) -> Result<Option<OffsetDateTime>> {
    let local = match local_db_path {
        Some(p) => p.to_path_buf(),
        None => paths::default_local_db_path()?,
    };
    let mut conn = db::open_local_ro(&local).await?;
    let max = db::max_local_end_time(&mut conn).await?;
    Ok(max.and_then(|secs| OffsetDateTime::from_unix_timestamp(secs).ok()))
}

/// Export matching rows as CSV to the provided writer.
///
/// - `local_db_path`: optional override for the local DB path
/// - `filters`: optional time/app filters
/// - `out`: writer to stream CSV content into
pub async fn export_csv(
    local_db_path: Option<&Path>,
    filters: &ExportFilters,
    out: &mut dyn Write,
) -> Result<()> {
    let default_local = paths::default_local_db_path()?;
    export::export_csv_impl(local_db_path, filters, &default_local, out).await
}

/// Export matching rows as a JSON array to the provided writer.
///
/// - `local_db_path`: optional override for the local DB path
/// - `filters`: optional time/app filters
/// - `out`: writer to stream JSON content into
pub async fn export_json(
    local_db_path: Option<&Path>,
    filters: &ExportFilters,
    out: &mut dyn Write,
) -> Result<()> {
    let default_local = paths::default_local_db_path()?;
    export::export_json_impl(local_db_path, filters, &default_local, out).await
}
