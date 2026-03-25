use sqlx::FromRow;
use time::OffsetDateTime;

/// Rows mapped from Apple's Screen Time (Knowledge) database queries.
///
/// The ingestion query selects and aliases columns to human‑friendly names,
/// converting Cocoa epoch timestamps (2001-01-01) to Unix epoch seconds by adding
/// 978_307_200 and casting to REAL to avoid INTEGER/REAL mismatches.
///
/// Aliases used in the ingestion SELECT:
/// - event_id            (ZOBJECT.Z_PK)
/// - app_name            (ZOBJECT.ZVALUESTRING)
/// - amount_seconds      (CAST(ZOBJECT.ZENDDATE - ZOBJECT.ZSTARTDATE AS REAL))
/// - start_time          (CAST(ZOBJECT.ZSTARTDATE + 978307200 AS INTEGER))
/// - end_time            (CAST(ZOBJECT.ZENDDATE + 978307200 AS INTEGER))
/// - created_at          (CAST(ZOBJECT.ZCREATIONDATE + 978307200 AS INTEGER))
/// - tz_offset           (ZOBJECT.ZSECONDSFROMGMT)           -- nullable
/// - device_id           (ZSOURCE.ZDEVICEID)                 -- nullable
/// - device_model        (ZSYNCPEER.ZMODEL)                  -- nullable
#[derive(Debug, Clone, FromRow)]
pub struct KnowledgeUsageRow {
    /// Unique event identifier (ZOBJECT.Z_PK). Stable across re-reads and used for deduplication.
    pub event_id: i64,

    /// Application identifier or name associated with the usage event (ZOBJECT.ZVALUESTRING).
    pub app_name: String,

    /// Duration of the usage interval in seconds.
    /// Computed as (ZENDDATE - ZSTARTDATE) and explicitly CAST to REAL in the query.
    #[sqlx(rename = "amount_seconds")]
    pub duration_secs: f64,

    /// Start time as Unix epoch seconds. Converted from Cocoa epoch in the query.
    #[sqlx(rename = "start_time")]
    pub start_unix: i64,

    /// End time as Unix epoch seconds. Converted from Cocoa epoch in the query.
    #[sqlx(rename = "end_time")]
    pub end_unix: i64,

    /// Creation time as Unix epoch seconds. Converted from Cocoa epoch in the query.
    #[sqlx(rename = "created_at")]
    pub created_unix: i64,

    /// Seconds offset from GMT for the event, if present (ZOBJECT.ZSECONDSFROMGMT).
    #[sqlx(rename = "tz_offset")]
    pub tz_offset_seconds: Option<i32>,

    /// Device identifier the record originated from (nullable) (ZSOURCE.ZDEVICEID).
    pub device_id: Option<String>,

    /// Device model string (nullable) (ZSYNCPEER.ZMODEL).
    pub device_model: Option<String>,
}

impl KnowledgeUsageRow {
    /// Start time as whole seconds since Unix epoch.
    pub fn start_ts(&self) -> i64 {
        self.start_unix
    }

    /// End time as whole seconds since Unix epoch.
    pub fn end_ts(&self) -> i64 {
        self.end_unix
    }

    /// Creation time as whole seconds since Unix epoch.
    pub fn created_ts(&self) -> i64 {
        self.created_unix
    }

    /// Start time as OffsetDateTime (None if out of range).
    pub fn start_datetime(&self) -> Option<OffsetDateTime> {
        OffsetDateTime::from_unix_timestamp(self.start_ts()).ok()
    }

    /// End time as OffsetDateTime (None if out of range).
    pub fn end_datetime(&self) -> Option<OffsetDateTime> {
        OffsetDateTime::from_unix_timestamp(self.end_ts()).ok()
    }

    /// Creation time as OffsetDateTime (None if out of range).
    pub fn created_datetime(&self) -> Option<OffsetDateTime> {
        OffsetDateTime::from_unix_timestamp(self.created_ts()).ok()
    }

    /// Convenience to get normalized values for insertion into the local DB schema.
    ///
    /// Returns:
    /// - event_id (i64)
    /// - app_name (&str)
    /// - duration_secs (f64)
    /// - start_ts (i64)
    /// - end_ts (i64)
    /// - created_ts (i64)
    /// - tz_offset_seconds (i32, default 0)
    /// - device_id (Option<&str>)
    /// - device_model (&str, default empty when NULL)
    pub fn as_local_insert_args(&self) -> (i64, &str, f64, i64, i64, i64, i32, Option<&str>, &str) {
        (
            self.event_id,
            self.app_name.as_str(),
            self.duration_secs,
            self.start_ts(),
            self.end_ts(),
            self.created_ts(),
            self.tz_offset_seconds.unwrap_or(0),
            self.device_id.as_deref(),
            self.device_model.as_deref().unwrap_or_default(),
        )
    }
}

/// Rows mapped from the local persistent database (`usage` table).
///
/// Schema (created via migration):
///   CREATE TABLE usage (
///       event_id     INTEGER PRIMARY KEY,
///       app_id       INTEGER NOT NULL,
///       amount       INTEGER NOT NULL,     -- duration in seconds
///       start_time   INTEGER NOT NULL,     -- epoch seconds
///       created_at   INTEGER NOT NULL,     -- epoch seconds
///       tz_offset    INTEGER NOT NULL,     -- seconds from GMT
///       device_id    INTEGER NULL
///   );
///
/// This struct maps the schema to human‑friendly field names while preserving
/// column names via #[sqlx(rename = "...")] where needed.
#[derive(Debug, Clone, FromRow)]
pub struct LocalUsageRow {
    /// Stable unique identifier of the event (copied from Knowledge DB ZOBJECT.Z_PK).
    pub event_id: i64,

    /// Application identifier or name.
    pub app_name: String,

    /// Duration in seconds as stored in the `amount` column.
    #[sqlx(rename = "amount")]
    pub duration_secs: i64,

    /// Start time as Unix epoch seconds.
    #[sqlx(rename = "start_time")]
    pub start_unix: i64,

    /// End time as Unix epoch seconds (computed in SELECT as start_time + amount).
    #[sqlx(rename = "end_time")]
    pub end_unix: i64,

    /// Creation time as Unix epoch seconds.
    #[sqlx(rename = "created_at")]
    pub created_unix: i64,

    /// Seconds offset from GMT stored in the `tz_offset` column.
    #[sqlx(rename = "tz_offset")]
    pub tz_offset_seconds: i32,

    /// Optional device identifier string.
    pub device_id: Option<String>,

    /// Device model string.
    pub device_model: String,
}

impl LocalUsageRow {
    /// Start time as whole seconds since Unix epoch.
    pub fn start_ts(&self) -> i64 {
        self.start_unix
    }

    /// End time as whole seconds since Unix epoch.
    pub fn end_ts(&self) -> i64 {
        self.end_unix
    }

    /// Creation time as whole seconds since Unix epoch.
    pub fn created_ts(&self) -> i64 {
        self.created_unix
    }

    /// Start time as OffsetDateTime (None if out of range).
    pub fn start_datetime(&self) -> Option<OffsetDateTime> {
        OffsetDateTime::from_unix_timestamp(self.start_ts()).ok()
    }

    /// End time as OffsetDateTime (None if out of range).
    pub fn end_datetime(&self) -> Option<OffsetDateTime> {
        OffsetDateTime::from_unix_timestamp(self.end_ts()).ok()
    }

    /// Creation time as OffsetDateTime (None if out of range).
    pub fn created_datetime(&self) -> Option<OffsetDateTime> {
        OffsetDateTime::from_unix_timestamp(self.created_ts()).ok()
    }
}
