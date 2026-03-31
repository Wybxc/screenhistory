#![forbid(unsafe_code)]

//! Visualization module for activity history.
//!
//! This module provides data aggregation for displaying usage statistics:
//! - Weekly summaries organized by application and day
//! - Async database queries and aggregation
//!
//! The module focuses on data layer only; UI rendering is handled separately.

pub mod aggregator;

use std::collections::HashMap;
use time::Date;

pub use aggregator::load_weekly_stats;

/// Representation of a single app's usage on a specific day.
#[derive(Debug, Clone)]
pub struct DailyAppUsage {
    /// The day of the usage (date).
    pub day: Date,
    /// Total usage duration in seconds for this app on this day.
    pub duration_secs: u64,
}

/// Weekly aggregated statistics for all applications.
#[derive(Debug, Clone)]
pub struct WeeklyStats {
    /// The Monday (start) of the week (ISO 8601).
    pub week_start: Date,
    /// Mapping from app name to sorted list of daily usage records for the week.
    /// Each entry should have exactly 7 days (Mon-Sun).
    pub apps: HashMap<String, Vec<DailyAppUsage>>,
}

impl WeeklyStats {
    /// Create a new WeeklyStats for a given week start date.
    pub fn new(week_start: Date) -> Self {
        Self {
            week_start,
            apps: HashMap::new(),
        }
    }

    /// Insert or update app usage for a specific day.
    /// If the entry doesn't exist, it's created; otherwise, accumulated.
    pub fn add_usage(&mut self, app_name: String, day: Date, duration_secs: u64) {
        self.apps
            .entry(app_name)
            .or_insert_with(Vec::new)
            .push(DailyAppUsage { day, duration_secs });
    }

    /// Get a sorted list of app names (for consistent UI rendering).
    pub fn sorted_app_names(&self) -> Vec<String> {
        let mut names: Vec<_> = self.apps.keys().cloned().collect();
        names.sort();
        names
    }

    /// Get the usage duration for a specific app on a specific day, or 0 if not found.
    pub fn get_usage(&self, app_name: &str, day: Date) -> u64 {
        self.apps
            .get(app_name)
            .and_then(|usages| {
                usages
                    .iter()
                    .find(|u| u.day == day)
                    .map(|u| u.duration_secs)
            })
            .unwrap_or(0)
    }
}
