use std::path::Path;

use anyhow::Result;
use futures_util::TryStreamExt;

use crate::db;
use crate::models::rows::KnowledgeUsageRow;
use crate::models::SyncSummary;

/// Incrementally sync from Apple's Screen Time (Knowledge) database into the local database.
///
/// - Determines a watermark via MAX(event_id) from the local DB
/// - Reads newer rows from the Knowledge DB (ZOBJECT.Z_PK > watermark)
/// - Inserts usage rows with INSERT OR IGNORE for idempotency
/// - Timestamps are converted to Unix epoch seconds (REAL) in the SELECT
///
/// Params:
/// - knowledge_db_path: optional path override for the Knowledge DB; if None, `default_knowledge` is used
/// - local_db_path: optional path override for the local DB; if None, `default_local` is used
pub async fn sync_impl(knowledge_db_path: &Path, local_db_path: &Path) -> Result<SyncSummary> {
    // Open local DB (RW) and apply migrations.
    let mut local = db::open_local_rw(local_db_path).await?;
    db::migrate(&mut local).await?;
    let last_event_id = db::max_local_event_id(&mut local).await?;

    // Open Apple's Knowledge DB (RO).
    let mut knowledge = db::open_knowledge_ro(knowledge_db_path).await?;

    // Base SELECT with explicit REAL casts to avoid INTEGER/REAL decode mismatches.
    let base_select = r#"
        SELECT
            ZOBJECT.Z_PK AS event_id,
            ZOBJECT.ZVALUESTRING AS app_name,
            CAST((ZOBJECT.ZENDDATE - ZOBJECT.ZSTARTDATE) AS REAL) AS amount_seconds,
            CAST((ZOBJECT.ZSTARTDATE + 978307200) AS REAL) AS start_time,
            CAST((ZOBJECT.ZENDDATE + 978307200) AS REAL) AS end_time,
            CAST((ZOBJECT.ZCREATIONDATE + 978307200) AS REAL) AS created_at,
            ZOBJECT.ZSECONDSFROMGMT AS tz_offset,
            COALESCE(ZSOURCE.ZDEVICEID, ZSYNCPEER.ZDEVICEID) AS device_id,
            COALESCE(ZSYNCPEER.ZMODEL, ZSYNCPEER.ZRAPPORTID, ZSOURCE.ZSOURCEID) AS device_model
        FROM ZOBJECT
        LEFT JOIN ZSTRUCTUREDMETADATA ON ZOBJECT.ZSTRUCTUREDMETADATA = ZSTRUCTUREDMETADATA.Z_PK
        LEFT JOIN ZSOURCE ON ZOBJECT.ZSOURCE = ZSOURCE.Z_PK
        LEFT JOIN ZSYNCPEER ON (ZSOURCE.ZDEVICEID = ZSYNCPEER.ZDEVICEID OR ZSOURCE.ZSOURCEID = ZSYNCPEER.ZCLOUDID)
        WHERE ZOBJECT.ZSTREAMNAME = '/app/usage'
    "#;

    let (sql, bind_event): (String, Option<i64>) = if let Some(eid) = last_event_id {
        (
            format!("{base_select}\nAND ZOBJECT.Z_PK > ?1\nORDER BY ZOBJECT.ZSTARTDATE ASC"),
            Some(eid),
        )
    } else {
        (
            format!("{base_select}\nORDER BY ZOBJECT.ZSTARTDATE ASC"),
            None,
        )
    };

    let mut summary = SyncSummary::default();
    let mut tx = sqlx::Connection::begin(&mut local).await?;

    let mut rows = if let Some(eid) = bind_event {
        sqlx::query_as::<_, KnowledgeUsageRow>(&sql)
            .bind(eid)
            .fetch(&mut knowledge)
    } else {
        sqlx::query_as::<_, KnowledgeUsageRow>(&sql).fetch(&mut knowledge)
    };

    while let Some(rec) = rows.try_next().await? {
        summary.scanned += 1;

        let (event_id, app_name, amount, start_ts, end_ts, created_ts, tz, device_id, device_model) =
            rec.as_local_insert_args();

        let inserted = db::insert_usage(
            &mut *tx,
            event_id,
            app_name,
            amount,
            start_ts,
            end_ts,
            created_ts,
            tz,
            device_id,
            device_model,
        )
        .await?;

        if inserted > 0 {
            summary.inserted += 1;
        } else {
            summary.skipped += 1;
        }
    }

    tx.commit().await?;
    Ok(summary)
}
