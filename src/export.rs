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
/// - out: writer to stream CSV content into
pub async fn export_csv_impl<W: Write>(
    local_db_path: &Path,
    filters: &ExportFilters,
    out: &mut W,
) -> Result<()> {
    let mut conn = db::open_local_ro(local_db_path).await?;
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
        let rec: UsageRecord = row.into();
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
/// - out: writer to stream JSON content into
pub async fn export_json_impl<W: Write>(
    local_db_path: &Path,
    filters: &ExportFilters,
    out: &mut W,
) -> Result<()> {
    let mut conn = db::open_local_ro(local_db_path).await?;
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
        let rec: UsageRecord = row.into();
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
            u.event_id,
            a.name AS app_name,
            u.amount,
            u.start_time,
            (u.start_time + u.amount) AS end_time,
            u.created_at,
            u.tz_offset,
            d.external_id AS device_id,
            COALESCE(d.model, '') AS device_model
        FROM usage AS u
        JOIN apps AS a ON a.id = u.app_id
        LEFT JOIN devices AS d ON d.id = u.device_id
        "#,
    );

    let mut clauses: Vec<&str> = Vec::new();
    let mut binds: Vec<Param> = Vec::new();

    if let Some(from) = filters.from {
        clauses.push("(u.start_time + u.amount) >= ?");
        binds.push(Param::I64(from.unix_timestamp()));
    }
    if let Some(to) = filters.to {
        clauses.push("(u.start_time + u.amount) <= ?");
        binds.push(Param::I64(to.unix_timestamp()));
    }
    if let Some(app) = &filters.app {
        clauses.push("a.name = ?");
        binds.push(Param::Str(app.clone()));
    }

    if !clauses.is_empty() {
        sql.push_str("WHERE ");
        sql.push_str(&clauses.join(" AND "));
        sql.push('\n');
    }

    sql.push_str("ORDER BY (u.start_time + u.amount) ASC\n");
    (sql, binds)
}

/// Parameter wrapper for binding mixed types in order.
enum Param {
    I64(i64),
    Str(String),
}
