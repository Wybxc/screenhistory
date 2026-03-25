use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use screenhistory::{export_csv, export_json, init_db, last_sync, sync, ExportFilters};
use time::OffsetDateTime;

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("knowledgeC.sanitized.db")
}

fn temp_local_db_path() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "screenhistory-export-test-{}-{}.sqlite",
        std::process::id(),
        nanos
    ))
}

#[tokio::test]
async fn export_with_filters_and_formats() -> Result<()> {
    let fixture = fixture_path();
    if !fixture.exists() {
        eprintln!(
            "Skipping test: missing fixture DB {}. Generate once with: python3 scripts/make_sanitized_fixture.py --source knowledgeC.db --out tests/fixtures/knowledgeC.sanitized.db --max-rows 1500",
            fixture.display()
        );
        return Ok(());
    }

    let local = temp_local_db_path();

    init_db(Some(Path::new(&local))).await?;
    let summary = sync(Some(Path::new(&fixture)), Some(Path::new(&local))).await?;
    assert!(summary.inserted > 0, "fixture sync should insert rows");

    // Baseline JSON export.
    let mut all_json = Vec::new();
    export_json(Some(Path::new(&local)), &ExportFilters::default(), &mut all_json).await?;
    let all_value: serde_json::Value =
        serde_json::from_slice(&all_json).context("parsing full export json")?;
    let all_items = all_value
        .as_array()
        .context("full export should be json array")?;
    assert!(!all_items.is_empty(), "full export should not be empty");

    // App filter branch.
    let sample_app = all_items[0]
        .get("app_name")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let app_filters = ExportFilters {
        app: Some(sample_app.clone()),
        ..ExportFilters::default()
    };
    let mut app_json = Vec::new();
    export_json(Some(Path::new(&local)), &app_filters, &mut app_json).await?;
    let app_value: serde_json::Value =
        serde_json::from_slice(&app_json).context("parsing app filtered json")?;
    let app_items = app_value
        .as_array()
        .context("app filtered export should be json array")?;
    assert!(!app_items.is_empty(), "app filtered export should not be empty");
    assert!(app_items.iter().all(|item| {
        item.get("app_name")
            .and_then(|v| v.as_str())
            .map(|s| s == sample_app)
            .unwrap_or(false)
    }));

    // from/to filter branches.
    let mut end_times: Vec<i64> = all_items
        .iter()
        .filter_map(|item| item.get("end_time").and_then(|v| v.as_i64()))
        .collect();
    end_times.sort_unstable();
    let pivot = end_times[end_times.len() / 2];

    let time_filters = ExportFilters {
        from: Some(OffsetDateTime::from_unix_timestamp(pivot)?),
        to: Some(OffsetDateTime::from_unix_timestamp(pivot)?),
        ..ExportFilters::default()
    };
    let mut time_json = Vec::new();
    export_json(Some(Path::new(&local)), &time_filters, &mut time_json).await?;
    let time_value: serde_json::Value =
        serde_json::from_slice(&time_json).context("parsing time filtered json")?;
    let time_items = time_value
        .as_array()
        .context("time filtered export should be json array")?;
    assert!(!time_items.is_empty(), "time filtered export should not be empty");
    assert!(time_items.iter().all(|item| {
        item.get("end_time")
            .and_then(|v| v.as_i64())
            .map(|t| t == pivot)
            .unwrap_or(false)
    }));

    // CSV export branch.
    let mut csv_out = Vec::new();
    export_csv(Some(Path::new(&local)), &ExportFilters::default(), &mut csv_out).await?;
    let csv_text = String::from_utf8(csv_out).context("csv should be utf-8")?;
    let mut lines = csv_text.lines();
    let header = lines.next().unwrap_or_default();
    assert_eq!(
        header,
        "event_id,app_name,amount,start_time,end_time,created_at,tz_offset,device_id,device_model"
    );
    assert!(lines.next().is_some(), "csv should contain at least one data row");

    // last_sync paths: empty db -> None, populated db -> Some.
    let empty_local = temp_local_db_path();
    init_db(Some(Path::new(&empty_local))).await?;
    assert!(last_sync(Some(Path::new(&empty_local))).await?.is_none());
    assert!(last_sync(Some(Path::new(&local))).await?.is_some());

    let _ = std::fs::remove_file(&empty_local);
    let _ = std::fs::remove_file(&local);
    Ok(())
}
