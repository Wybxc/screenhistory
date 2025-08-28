#![forbid(unsafe_code)]

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use time::OffsetDateTime;

pub use models::{ExportFilters, SyncSummary, UsageRecord};

/// Default local DB path: ~/.screenhistory.sqlite
pub fn default_local_db_path() -> Result<PathBuf> {
    paths::default_local_db_path()
}

/// Default Apple Screen Time DB path: ~/Library/Application Support/Knowledge/knowledgeC.db
pub fn default_knowledge_db_path() -> Result<PathBuf> {
    paths::default_knowledge_db_path()
}

/// Create/migrate the local DB. Returns the resolved path.
pub async fn init_db(path: Option<&Path>) -> Result<PathBuf> {
    api::init_db(path).await
}

/// Incremental sync from Apple's Screen Time DB into the local DB.
pub async fn sync(
    knowledge_db_path: Option<&Path>,
    local_db_path: Option<&Path>,
) -> Result<SyncSummary> {
    api::sync(knowledge_db_path, local_db_path).await
}

/// Last synced end_time (max) from the local DB.
pub async fn last_sync(local_db_path: Option<&Path>) -> Result<Option<OffsetDateTime>> {
    api::last_sync(local_db_path).await
}

/// Export matched rows to CSV. Writes to the provided writer.
pub async fn export_csv(
    local_db_path: Option<&Path>,
    filters: &ExportFilters,
    out: &mut dyn Write,
) -> Result<()> {
    api::export_csv(local_db_path, filters, out).await
}

/// Export matched rows to JSON array. Writes to the provided writer.
pub async fn export_json(
    local_db_path: Option<&Path>,
    filters: &ExportFilters,
    out: &mut dyn Write,
) -> Result<()> {
    api::export_json(local_db_path, filters, out).await
}

/* ========================== modules ========================== */

mod paths {
    use std::path::PathBuf;

    use anyhow::{Context, Result};
    use directories::BaseDirs;

    const LOCAL_DB_FILE_NAME: &str = ".screenhistory.sqlite";

    pub fn default_local_db_path() -> Result<PathBuf> {
        let base = BaseDirs::new().context("Failed to get user directories")?;
        Ok(base.home_dir().join(LOCAL_DB_FILE_NAME))
    }

    pub fn default_knowledge_db_path() -> Result<PathBuf> {
        let base = BaseDirs::new().context("Failed to get user directories")?;
        Ok(base
            .home_dir()
            .join("Library")
            .join("Application Support")
            .join("Knowledge")
            .join("knowledgeC.db"))
    }
}

mod models {
    use serde::Serialize;
    use time::OffsetDateTime;

    #[derive(Debug, Clone, Serialize)]
    pub struct UsageRecord {
        pub event_id: i64,
        pub app_name: String,
        pub amount: f64,
        pub start_time: i64,
        pub end_time: i64,
        pub created_at: i64,
        pub tz_offset: i32,
        pub device_id: Option<String>,
        pub device_model: String,
    }

    #[derive(Debug, Default, Clone)]
    pub struct ExportFilters {
        pub from: Option<OffsetDateTime>,
        pub to: Option<OffsetDateTime>,
        pub app: Option<String>,
    }

    #[derive(Debug, Default, Clone, Serialize)]
    pub struct SyncSummary {
        pub scanned: u64,
        pub inserted: u64,
        pub skipped: u64,
    }
}

mod db {
    use std::fs;
    use std::path::Path;

    use anyhow::{Context, Result};
    use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous};
    use sqlx::{Acquire, Connection, Executor, Row, Sqlite, SqliteConnection, Transaction};

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

    pub async fn open_local_ro(path: &Path) -> Result<SqliteConnection> {
        let opts = SqliteConnectOptions::new().filename(path).read_only(true);
        let conn = SqliteConnection::connect_with(&opts)
            .await
            .with_context(|| format!("Opening local DB (ro) at {:?}", path))?;
        Ok(conn)
    }

    pub async fn open_knowledge_ro(path: &Path) -> Result<SqliteConnection> {
        let opts = SqliteConnectOptions::new().filename(path).read_only(true);
        let conn = SqliteConnection::connect_with(&opts)
            .await
            .with_context(|| format!("Opening knowledge DB at {:?}", path))?;
        Ok(conn)
    }

    pub async fn migrate(conn: &mut SqliteConnection) -> Result<()> {
        let version: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&mut *conn)
            .await
            .context("Reading PRAGMA user_version")?;

        if version == 0 {
            let mut tx = sqlx::Connection::begin(&mut *conn).await?;
            tx.execute(
                r#"
                CREATE TABLE IF NOT EXISTS usage (
                    event_id     INTEGER PRIMARY KEY,
                    app_name     TEXT NOT NULL,
                    amount       REAL NOT NULL,
                    start_time   REAL NOT NULL,
                    end_time     REAL NOT NULL,
                    created_at   REAL NOT NULL,
                    tz_offset    INTEGER NOT NULL,
                    device_id    TEXT NULL,
                    device_model TEXT NOT NULL
                );
            "#,
            )
            .await?;
            tx.execute(
                r#"
                CREATE INDEX IF NOT EXISTS idx_usage_end_time ON usage(end_time);
            "#,
            )
            .await?;
            tx.execute(
                r#"
                CREATE INDEX IF NOT EXISTS idx_usage_app_name ON usage(app_name);
            "#,
            )
            .await?;
            tx.execute(
                r#"
                CREATE TABLE IF NOT EXISTS meta (
                    key   TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );
            "#,
            )
            .await?;
            tx.execute("PRAGMA user_version = 1").await?;
            tx.commit().await?;
        }

        Ok(())
    }

    pub async fn max_local_event_id(conn: &mut SqliteConnection) -> Result<Option<i64>> {
        let val: Option<i64> = sqlx::query_scalar("SELECT MAX(event_id) FROM usage")
            .fetch_one(conn)
            .await?;
        Ok(val)
    }

    pub async fn max_local_end_time(conn: &mut SqliteConnection) -> Result<Option<i64>> {
        let val: Option<i64> = sqlx::query_scalar("SELECT MAX(end_time) FROM usage")
            .fetch_one(conn)
            .await?;
        Ok(val)
    }

    pub async fn insert_usage<'a, E>(
        exec: E,
        event_id: i64,
        app_name: &str,
        amount: f64,
        start_time: i64,
        end_time: i64,
        created_at: i64,
        tz_offset: i32,
        device_id: Option<&str>,
        device_model: &str,
    ) -> Result<u64>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let res = sqlx::query(
            r#"
            INSERT OR IGNORE INTO usage (
                event_id, app_name, amount, start_time, end_time, created_at, tz_offset, device_id, device_model
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        "#,
        )
        .bind(event_id)
        .bind(app_name)
        .bind(amount)
        .bind(start_time)
        .bind(end_time)
        .bind(created_at)
        .bind(tz_offset)
        .bind(device_id)
        .bind(device_model)
        .execute(exec)
        .await?;

        Ok(res.rows_affected())
    }

    pub type Tx<'a> = Transaction<'a, Sqlite>;
}

mod ingest {
    use std::path::Path;

    use anyhow::Result;
    use futures_util::TryStreamExt;
    use sqlx::Row;

    use super::db;
    use super::models::SyncSummary;

    pub async fn sync_impl(
        knowledge_db_path: Option<&Path>,
        local_db_path: Option<&Path>,
        default_knowledge: &Path,
        default_local: &Path,
    ) -> Result<SyncSummary> {
        let knowledge_path = knowledge_db_path.unwrap_or(default_knowledge);
        let local_path = local_db_path.unwrap_or(default_local);

        let mut local = db::open_local_rw(local_path).await?;
        db::migrate(&mut local).await?;
        let last_event_id = db::max_local_event_id(&mut local).await?;

        let mut knowledge = db::open_knowledge_ro(knowledge_path).await?;

        let base = r#"
            SELECT
                ZOBJECT.Z_PK AS event_id,
                ZOBJECT.ZVALUESTRING AS app_name,
                (ZOBJECT.ZENDDATE - ZOBJECT.ZSTARTDATE) AS amount_seconds,
                (ZOBJECT.ZSTARTDATE + 978307200) AS start_time,
                (ZOBJECT.ZENDDATE + 978307200) AS end_time,
                (ZOBJECT.ZCREATIONDATE + 978307200) AS created_at,
                ZOBJECT.ZSECONDSFROMGMT AS tz_offset,
                ZSOURCE.ZDEVICEID AS device_id,
                ZSYNCPEER.ZMODEL AS device_model
            FROM ZOBJECT
            LEFT JOIN ZSTRUCTUREDMETADATA ON ZOBJECT.ZSTRUCTUREDMETADATA = ZSTRUCTUREDMETADATA.Z_PK
            LEFT JOIN ZSOURCE ON ZOBJECT.ZSOURCE = ZSOURCE.Z_PK
            LEFT JOIN ZSYNCPEER ON ZSOURCE.ZDEVICEID = ZSYNCPEER.ZDEVICEID
            WHERE ZOBJECT.ZSTREAMNAME = '/app/usage'
        "#;

        let (sql, bind_event): (&str, Option<i64>) = if let Some(eid) = last_event_id {
            (
                concat!(
                    r#"
                    SELECT
                        ZOBJECT.Z_PK AS event_id,
                        ZOBJECT.ZVALUESTRING AS app_name,
                        (ZOBJECT.ZENDDATE - ZOBJECT.ZSTARTDATE) AS amount_seconds,
                        (ZOBJECT.ZSTARTDATE + 978307200) AS start_time,
                        (ZOBJECT.ZENDDATE + 978307200) AS end_time,
                        (ZOBJECT.ZCREATIONDATE + 978307200) AS created_at,
                        ZOBJECT.ZSECONDSFROMGMT AS tz_offset,
                        ZSOURCE.ZDEVICEID AS device_id,
                        ZSYNCPEER.ZMODEL AS device_model
                    FROM ZOBJECT
                    LEFT JOIN ZSTRUCTUREDMETADATA ON ZOBJECT.ZSTRUCTUREDMETADATA = ZSTRUCTUREDMETADATA.Z_PK
                    LEFT JOIN ZSOURCE ON ZOBJECT.ZSOURCE = ZSOURCE.Z_PK
                    LEFT JOIN ZSYNCPEER ON ZSOURCE.ZDEVICEID = ZSYNCPEER.ZDEVICEID
                    WHERE ZOBJECT.ZSTREAMNAME = '/app/usage'
                    "#,
                    "\nAND ZOBJECT.Z_PK > ?1\nORDER BY ZOBJECT.ZSTARTDATE ASC"
                ),
                Some(eid),
            )
        } else {
            (
                concat!(
                    r#"
                    SELECT
                        ZOBJECT.Z_PK AS event_id,
                        ZOBJECT.ZVALUESTRING AS app_name,
                        (ZOBJECT.ZENDDATE - ZOBJECT.ZSTARTDATE) AS amount_seconds,
                        (ZOBJECT.ZSTARTDATE + 978307200) AS start_time,
                        (ZOBJECT.ZENDDATE + 978307200) AS end_time,
                        (ZOBJECT.ZCREATIONDATE + 978307200) AS created_at,
                        ZOBJECT.ZSECONDSFROMGMT AS tz_offset,
                        ZSOURCE.ZDEVICEID AS device_id,
                        ZSYNCPEER.ZMODEL AS device_model
                    FROM ZOBJECT
                    LEFT JOIN ZSTRUCTUREDMETADATA ON ZOBJECT.ZSTRUCTUREDMETADATA = ZSTRUCTUREDMETADATA.Z_PK
                    LEFT JOIN ZSOURCE ON ZOBJECT.ZSOURCE = ZSOURCE.Z_PK
                    LEFT JOIN ZSYNCPEER ON ZSOURCE.ZDEVICEID = ZSYNCPEER.ZDEVICEID
                    WHERE ZOBJECT.ZSTREAMNAME = '/app/usage'
                    "#,
                    "\nORDER BY ZOBJECT.ZSTARTDATE ASC"
                ),
                None,
            )
        };

        let mut summary = SyncSummary::default();

        let mut tx = sqlx::Connection::begin(&mut local).await?;
        let mut rows = if let Some(eid) = bind_event {
            sqlx::query(sql).bind(eid).fetch(&mut knowledge)
        } else {
            sqlx::query(sql).fetch(&mut knowledge)
        };

        while let Some(row) = rows.try_next().await? {
            summary.scanned += 1;

            let event_id: i64 = row.try_get("event_id")?;
            let app_name: String = row.try_get("app_name")?;
            let amount_seconds: f64 = row.try_get("amount_seconds")?;
            let start_time: f64 = row.try_get("start_time")?;
            let end_time: f64 = row.try_get("end_time")?;
            let created_at: f64 = row.try_get("created_at")?;
            let tz_offset: Option<i32> = row.try_get("tz_offset").ok();
            let device_id: Option<String> = row.try_get("device_id").ok();
            let device_model: Option<String> = row.try_get("device_model").ok();

            let inserted = db::insert_usage(
                &mut *tx,
                event_id,
                &app_name,
                amount_seconds,
                start_time as i64,
                end_time as i64,
                created_at as i64,
                tz_offset.unwrap_or(0),
                device_id.as_deref(),
                device_model.unwrap_or_default().as_str(),
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
}

mod export {
    use std::io::Write;
    use std::path::Path;

    use anyhow::Result;
    use csv::Writer;
    use futures_util::TryStreamExt;
    use sqlx::Row;

    use super::db;
    use super::models::{ExportFilters, UsageRecord};

    enum Param {
        I64(i64),
        Str(String),
    }

    pub async fn export_csv_impl(
        local_db_path: Option<&Path>,
        filters: &ExportFilters,
        default_local: &Path,
        out: &mut dyn Write,
    ) -> Result<()> {
        let local_path = local_db_path.unwrap_or(default_local);
        let mut conn = db::open_local_ro(local_path).await?;
        let (sql, binds) = build_select(filters);

        let mut wtr = Writer::from_writer(out);
        wtr.write_record(&[
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

        let mut q = sqlx::query(&sql);
        for b in binds {
            q = match b {
                Param::I64(v) => q.bind(v),
                Param::Str(s) => q.bind(s),
            };
        }

        let mut rows = q.fetch(&mut conn);
        while let Some(row) = rows.try_next().await? {
            let event_id: i64 = row.try_get("event_id")?;
            let app_name: String = row.try_get("app_name")?;
            let amount: f64 = row.try_get("amount")?;
            let start_time: i64 = row.try_get::<f64, _>("start_time")? as i64;
            let end_time: i64 = row.try_get::<f64, _>("end_time")? as i64;
            let created_at: i64 = row.try_get::<f64, _>("created_at")? as i64;
            let tz_offset: i32 = row.try_get("tz_offset")?;
            let device_id: Option<String> = row.try_get("device_id").ok();
            let device_model: String = row.try_get("device_model")?;

            wtr.write_record(&[
                event_id.to_string(),
                app_name,
                amount.to_string(),
                start_time.to_string(),
                end_time.to_string(),
                created_at.to_string(),
                tz_offset.to_string(),
                device_id.unwrap_or_default(),
                device_model,
            ])?;
        }

        wtr.flush()?;
        Ok(())
    }

    pub async fn export_json_impl(
        local_db_path: Option<&Path>,
        filters: &ExportFilters,
        default_local: &Path,
        out: &mut dyn Write,
    ) -> Result<()> {
        let local_path = local_db_path.unwrap_or(default_local);
        let mut conn = db::open_local_ro(local_path).await?;
        let (sql, binds) = build_select(filters);

        let mut q = sqlx::query(&sql);
        for b in binds {
            q = match b {
                Param::I64(v) => q.bind(v),
                Param::Str(s) => q.bind(s),
            };
        }

        let mut rows = q.fetch(&mut conn);
        let mut first = true;
        out.write_all(b"[")?;

        while let Some(row) = rows.try_next().await? {
            let rec = UsageRecord {
                event_id: row.try_get("event_id")?,
                app_name: row.try_get("app_name")?,
                amount: row.try_get("amount")?,
                start_time: row.try_get::<f64, _>("start_time")? as i64,
                end_time: row.try_get::<f64, _>("end_time")? as i64,
                created_at: row.try_get::<f64, _>("created_at")? as i64,
                tz_offset: row.try_get("tz_offset")?,
                device_id: row.try_get::<Option<String>, _>("device_id")?,
                device_model: row.try_get("device_model")?,
            };

            if !first {
                out.write_all(b",")?;
            }
            serde_json::to_writer(&mut *out, &rec)?;
            first = false;
        }

        out.write_all(b"]")?;
        Ok(())
    }

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

        sql.push_str("ORDER BY end_time ASC");
        (sql, binds)
    }
}

mod api {
    use std::io::Write;
    use std::path::Path;

    use anyhow::Result;
    use time::OffsetDateTime;

    use super::db;
    use super::export::{export_csv_impl, export_json_impl};
    use super::ingest::sync_impl;
    use super::models::SyncSummary;
    use super::paths::{default_knowledge_db_path, default_local_db_path};

    pub async fn init_db(path: Option<&Path>) -> Result<std::path::PathBuf> {
        let local = match path {
            Some(p) => p.to_path_buf(),
            None => default_local_db_path()?,
        };
        let mut conn = db::open_local_rw(&local).await?;
        db::migrate(&mut conn).await?;
        Ok(local)
    }

    pub async fn sync(
        knowledge_db_path: Option<&Path>,
        local_db_path: Option<&Path>,
    ) -> Result<SyncSummary> {
        let default_knowledge = default_knowledge_db_path()?;
        let default_local = default_local_db_path()?;
        sync_impl(
            knowledge_db_path,
            local_db_path,
            &default_knowledge,
            &default_local,
        )
        .await
    }

    pub async fn last_sync(local_db_path: Option<&Path>) -> Result<Option<OffsetDateTime>> {
        let local = match local_db_path {
            Some(p) => p.to_path_buf(),
            None => default_local_db_path()?,
        };
        let mut conn = db::open_local_ro(&local).await?;
        let max = db::max_local_end_time(&mut conn).await?;
        Ok(max.and_then(|s| OffsetDateTime::from_unix_timestamp(s).ok()))
    }

    pub async fn export_csv(
        local_db_path: Option<&Path>,
        filters: &super::models::ExportFilters,
        out: &mut dyn Write,
    ) -> Result<()> {
        export_csv_impl(local_db_path, filters, &default_local_db_path()?, out).await
    }

    pub async fn export_json(
        local_db_path: Option<&Path>,
        filters: &super::models::ExportFilters,
        out: &mut dyn Write,
    ) -> Result<()> {
        export_json_impl(local_db_path, filters, &default_local_db_path()?, out).await
    }
}
