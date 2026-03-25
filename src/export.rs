use std::io::Write;
use std::path::Path;

use anyhow::Result;
use csv::Writer;
use futures_util::TryStreamExt;
use sqlx::{QueryBuilder, Sqlite};

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
    let mut q = build_select_query(filters);

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

    let mut rows = q.build_query_as::<LocalUsageRow>().fetch(&mut conn);
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
    let mut q = build_select_query(filters);
    let mut rows = q.build_query_as::<LocalUsageRow>().fetch(&mut conn);

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
fn build_select_query(filters: &ExportFilters) -> QueryBuilder<'static, Sqlite> {
    let mut query = QueryBuilder::<Sqlite>::new(
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
        WHERE 1 = 1
        "#,
    );

    if let Some(from) = filters.from {
        query.push(" AND (u.start_time + u.amount) >= ");
        query.push_bind(from.unix_timestamp());
    }
    if let Some(to) = filters.to {
        query.push(" AND (u.start_time + u.amount) <= ");
        query.push_bind(to.unix_timestamp());
    }
    if let Some(app) = &filters.app {
        query.push(" AND a.name = ");
        query.push_bind(app.clone());
    }

    query.push(" ORDER BY (u.start_time + u.amount) ASC\n");
    query
}
