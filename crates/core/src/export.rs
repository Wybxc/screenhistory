use std::io::Write;
use std::path::Path;

use anyhow::Result;
use csv::Writer;
use futures_util::TryStreamExt;

use crate::db;
use crate::models::rows::LocalUsageRow;
use crate::models::{ExportFilters, UsageRecord};

/// Export matching rows from the local database as CSV to the provided writer.
///
/// - local_db_path: optionally override the default local DB location; if None, use the provided default path
/// - filters: optional time/app filters
/// - default_local: default path to the local database (e.g. ~/.screenhistory.sqlite)
/// - out: writer to stream CSV content into
pub async fn export_csv_impl(
    local_db_path: Option<&Path>,
    filters: &ExportFilters,
    default_local: &Path,
    out: &mut dyn Write,
) -> Result<()> {
    let local_path = local_db_path.unwrap_or(default_local);
    let mut conn = db::open_local_ro(local_path).await?;
    let (sql, binds) = build_select(filters);

    // Header aligned with UsageRecord JSON keys for consistency.
    let mut wtr = Writer::from_writer(out);
    wtr.write_record([
        "event_id",
        "app_name",
        "amount",
        "start_time",
        "end_time",
        "created_at",
        "tz_offset",
        "device_id",
        "device_model",
    ])?;

    let mut q = sqlx::query_as::<_, LocalUsageRow>(&sql);
    for b in binds {
        q = match b {
            Param::I64(v) => q.bind(v),
            Param::Str(s) => q.bind(s),
        };
    }

    let mut rows = q.fetch(&mut conn);
    while let Some(row) = rows.try_next().await? {
        let rec = to_usage_record(&row);
        wtr.write_record(&[
            rec.event_id.to_string(),
            rec.app_name,
            rec.amount.to_string(),
            rec.start_time.to_string(),
            rec.end_time.to_string(),
            rec.created_at.to_string(),
            rec.tz_offset.to_string(),
            rec.device_id.unwrap_or_default(),
            rec.device_model,
        ])?;
    }

    wtr.flush()?;
    Ok(())
}

/// Export matching rows from the local database as a JSON array to the provided writer.
///
/// - local_db_path: optionally override the default local DB location; if None, use the provided default path
/// - filters: optional time/app filters
/// - default_local: default path to the local database (e.g. ~/.screenhistory.sqlite)
/// - out: writer to stream JSON content into
pub async fn export_json_impl(
    local_db_path: Option<&Path>,
    filters: &ExportFilters,
    default_local: &Path,
    out: &mut dyn Write,
) -> Result<()> {
    let local_path = local_db_path.unwrap_or(default_local);
    let mut conn = db::open_local_ro(local_path).await?;
    let (sql, binds) = build_select(filters);

    let mut q = sqlx::query_as::<_, LocalUsageRow>(&sql);
    for b in binds {
        q = match b {
            Param::I64(v) => q.bind(v),
            Param::Str(s) => q.bind(s),
        };
    }

    let mut rows = q.fetch(&mut conn);

    // Stream as a JSON array without buffering everything in memory.
    out.write_all(b"[")?;
    let mut first = true;

    while let Some(row) = rows.try_next().await? {
        let rec = to_usage_record(&row);
        if !first {
            out.write_all(b",")?;
        }
        serde_json::to_writer(&mut *out, &rec)?;
        first = false;
    }

    out.write_all(b"]")?;
    Ok(())
}

/// Build a SELECT statement with optional filters and corresponding bound parameters.
///
/// Filters:
/// - from/to: inclusive range on end_time (epoch seconds)
/// - app: exact match on app_name
fn build_select(filters: &ExportFilters) -> (String, Vec<Param>) {
    let mut sql = String::from(
        r#"
        SELECT
            event_id, app_name, amount, start_time, end_time, created_at, tz_offset, device_id, device_model
        FROM usage
        "#,
    );

    let mut clauses: Vec<&str> = Vec::new();
    let mut binds: Vec<Param> = Vec::new();

    if let Some(from) = filters.from {
        clauses.push("end_time >= ?");
        binds.push(Param::I64(from.unix_timestamp()));
    }
    if let Some(to) = filters.to {
        clauses.push("end_time <= ?");
        binds.push(Param::I64(to.unix_timestamp()));
    }
    if let Some(app) = &filters.app {
        clauses.push("app_name = ?");
        binds.push(Param::Str(app.clone()));
    }

    if !clauses.is_empty() {
        sql.push_str("WHERE ");
        sql.push_str(&clauses.join(" AND "));
        sql.push('\n');
    }

    sql.push_str("ORDER BY end_time ASC\n");
    (sql, binds)
}

/// Parameter wrapper for binding mixed types in order.
enum Param {
    I64(i64),
    Str(String),
}

/// Convert a LocalUsageRow into the normalized UsageRecord used for exports.
///
/// - Casts REAL timestamp columns to whole seconds (i64)
/// - Renames `amount` to `amount` (seconds) for output symmetry across CSV/JSON
fn to_usage_record(row: &LocalUsageRow) -> UsageRecord {
    UsageRecord {
        event_id: row.event_id,
        app_name: row.app_name.clone(),
        amount: row.duration_secs,
        start_time: row.start_unix as i64,
        end_time: row.end_unix as i64,
        created_at: row.created_unix as i64,
        tz_offset: row.tz_offset_seconds,
        device_id: row.device_id.clone(),
        device_model: row.device_model.clone(),
    }
}
