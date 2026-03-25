use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous};
use sqlx::{Connection, Sqlite, SqliteConnection, Transaction};

/// Open (and create if needed) the local SQLite database for read/write.
/// - Ensures parent directory exists
/// - Sets WAL journal mode and NORMAL sync for a small, reliable local DB
pub async fn open_local_rw(path: &Path) -> Result<SqliteConnection> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("Creating {:?}", parent))?;
    }
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal);
    let conn = SqliteConnection::connect_with(&opts)
        .await
        .with_context(|| format!("Opening local DB at {:?}", path))?;
    Ok(conn)
}

/// Open the local SQLite database read-only.
pub async fn open_local_ro(path: &Path) -> Result<SqliteConnection> {
    let opts = SqliteConnectOptions::new().filename(path).read_only(true);
    let conn = SqliteConnection::connect_with(&opts)
        .await
        .with_context(|| format!("Opening local DB (ro) at {:?}", path))?;
    Ok(conn)
}

/// Open Apple's Screen Time (Knowledge) database read-only.
pub async fn open_knowledge_ro(path: &Path) -> Result<SqliteConnection> {
    let opts = SqliteConnectOptions::new().filename(path).read_only(true);
    let conn = SqliteConnection::connect_with(&opts)
        .await
        .with_context(|| format!("Opening knowledge DB at {:?}", path))?;
    Ok(conn)
}

/// Run schema migrations using sqlx Migrator from ./migrations.
/// Idempotent and ordered by migration filenames.
pub async fn migrate(conn: &mut SqliteConnection) -> Result<()> {
    static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
    MIGRATOR.run(conn).await?;
    Ok(())
}

/// Get the maximum known event_id (watermark) from the local DB.
pub async fn max_local_event_id(conn: &mut SqliteConnection) -> Result<Option<i64>> {
    let val: Option<i64> = sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(event_id) FROM usage")
        .fetch_one(conn)
        .await?;
    Ok(val)
}

/// Get the latest end_time from the local DB (as a second-based epoch).
pub async fn max_local_end_time(conn: &mut SqliteConnection) -> Result<Option<i64>> {
    let val: Option<i64> =
        sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(start_time + amount) FROM usage")
            .fetch_one(conn)
            .await?;
    Ok(val)
}

/// Insert a usage row into the local DB, ignoring duplicates by event_id.
/// Returns the number of rows affected (0 when ignored, 1 when inserted).
#[allow(clippy::too_many_arguments)]
pub async fn insert_usage<'a>(
    tx: &mut Transaction<'a, Sqlite>,
    event_id: i64,
    app_name: &str,
    amount: f64,
    start_time: i64,
    created_at: i64,
    tz_offset: i32,
    device_id: Option<&str>,
    device_model: &str,
) -> Result<u64> {
    sqlx::query("INSERT OR IGNORE INTO apps(name) VALUES (?1)")
        .bind(app_name)
        .execute(&mut **tx)
        .await?;

    if let Some(external_id) = device_id {
        sqlx::query("INSERT OR IGNORE INTO devices(external_id, model) VALUES (?1, ?2)")
            .bind(external_id)
            .bind(device_model)
            .execute(&mut **tx)
            .await?;

        sqlx::query(
            r#"
            UPDATE devices
            SET model = COALESCE(NULLIF(?2, ''), model)
            WHERE external_id = ?1
            "#,
        )
        .bind(external_id)
        .bind(device_model)
        .execute(&mut **tx)
        .await?;
    }

    let res = sqlx::query(
        r#"
        INSERT OR IGNORE INTO usage (
            event_id, app_id, amount, start_time, created_at, tz_offset, device_id
        ) VALUES (
            ?1,
            (SELECT id FROM apps WHERE name = ?2),
            CAST(?3 AS INTEGER),
            ?4,
            ?5,
            ?6,
            CASE WHEN ?7 IS NULL THEN NULL ELSE (SELECT id FROM devices WHERE external_id = ?7) END
        )
        "#,
    )
    .bind(event_id)
    .bind(app_name)
    .bind(amount)
    .bind(start_time)
    .bind(created_at)
    .bind(tz_offset)
    .bind(device_id)
    .execute(&mut **tx)
    .await?;

    Ok(res.rows_affected())
}
