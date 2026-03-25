#![forbid(unsafe_code)]

//! Shared model types used across the core crate.
//!
//! This module:
//! - Hosts small, shared domain types (filters, summaries, export record).
//! - Re-exports strongly-typed row structs for SQL mappings from `rows`.

pub mod rows;

pub use rows::{KnowledgeUsageRow, LocalUsageRow};

use serde::Serialize;
use time::OffsetDateTime;

/// Filters applied when exporting usage records.
#[derive(Debug, Default, Clone)]
pub struct ExportFilters {
    /// Inclusive lower bound on end time (UTC).
    pub from: Option<OffsetDateTime>,
    /// Inclusive upper bound on end time (UTC).
    pub to: Option<OffsetDateTime>,
    /// Exact application identifier/name to match.
    pub app: Option<String>,
}

/// Summary information from a sync run.
#[derive(Debug, Default, Clone, Serialize)]
pub struct SyncSummary {
    /// Total rows scanned from the Knowledge DB.
    pub scanned: u64,
    /// Number of new rows inserted into the local DB.
    pub inserted: u64,
    /// Number of rows skipped (duplicates/conflicts).
    pub skipped: u64,
}

/// A normalized usage record shape used by exports (CSV/JSON).
///
/// All timestamps are Unix epoch seconds. Column names mirror the export headers.
#[derive(Debug, Clone, Serialize)]
pub struct UsageRecord {
    /// Stable unique identifier (ZOBJECT.Z_PK).
    pub event_id: i64,
    /// Application identifier or name (from ZOBJECT.ZVALUESTRING).
    pub app_name: String,
    /// Duration in seconds for the usage interval.
    pub amount: f64,
    /// Start time (epoch seconds).
    pub start_time: i64,
    /// End time (epoch seconds).
    pub end_time: i64,
    /// Creation time (epoch seconds).
    pub created_at: i64,
    /// Seconds offset from GMT (from ZOBJECT.ZSECONDSFROMGMT, defaulted if NULL).
    pub tz_offset: i32,
    /// Optional originating device identifier (from ZSOURCE.ZDEVICEID).
    pub device_id: Option<String>,
    /// Model string of the originating device (from ZSYNCPEER.ZMODEL).
    pub device_model: String,
}
