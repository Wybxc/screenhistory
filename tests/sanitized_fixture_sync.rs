use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use screenhistory::{export_json, init_db, sync, ExportFilters};

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
    std::env::temp_dir().join(format!("screenhistory-test-{}-{}.sqlite", std::process::id(), nanos))
}

#[tokio::test]
async fn sync_and_export_with_sanitized_fixture() -> Result<()> {
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

    assert!(summary.scanned > 0, "fixture should contain usage rows");
    assert!(summary.inserted > 0, "sync should insert rows into local db");

    let mut out = Vec::new();
    export_json(Some(Path::new(&local)), &ExportFilters::default(), &mut out).await?;
    let value: serde_json::Value = serde_json::from_slice(&out).context("parsing export json")?;

    let arr = value
        .as_array()
        .context("export json should be an array")?;
    assert!(!arr.is_empty(), "exported records should not be empty");

    for item in arr {
        let app_name = item
            .get("app_name")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        assert!(app_name.starts_with("app_"), "app_name should be sanitized");

        if let Some(device_id) = item.get("device_id").and_then(|v| v.as_str()) {
            if !device_id.is_empty() {
                assert!(
                    device_id.starts_with("dev_"),
                    "device_id should be sanitized when present"
                );
            }
        }

        let device_model = item
            .get("device_model")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        assert!(
            device_model.is_empty() || device_model.starts_with("model_"),
            "device_model should be sanitized when present"
        );
    }

    let _ = std::fs::remove_file(&local);
    Ok(())
}
