use std::path::PathBuf;

use anyhow::{Context, Result};
use directories::BaseDirs;

/// Default local database file name in the user's home directory.
pub const LOCAL_DB_FILE_NAME: &str = ".screenhistory.sqlite";

/// Returns the default path to the local database:
///   ~/.screenhistory.sqlite
pub fn default_local_db_path() -> Result<PathBuf> {
    let base = BaseDirs::new().context("Could not determine user directories")?;
    Ok(base.home_dir().join(LOCAL_DB_FILE_NAME))
}

/// Returns the default path to Apple's Screen Time (Knowledge) database:
///   ~/Library/Application Support/Knowledge/knowledgeC.db
pub fn default_knowledge_db_path() -> Result<PathBuf> {
    let base = BaseDirs::new().context("Could not determine user directories")?;
    Ok(base
        .home_dir()
        .join("Library")
        .join("Application Support")
        .join("Knowledge")
        .join("knowledgeC.db"))
}
