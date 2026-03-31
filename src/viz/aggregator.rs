use std::path::Path;

use anyhow::Result;
use time::{Date, Duration, OffsetDateTime};

use crate::db;
use super::WeeklyStats;

/// Load weekly usage statistics for the week containing the given date.
///
/// This function:
/// 1. Calculates the Monday (week start) for the given date
/// 2. Queries the local database for all usage records in that week
/// 3. Aggregates by app and day
/// 4. Returns a [`WeeklyStats`] struct with the aggregated data
///
/// # Arguments
/// * `local_db_path` - Path to the local SQLite database
/// * `date` - Any date in the target week (will be normalized to Monday)
///
/// # Returns
/// A `WeeklyStats` struct containing the aggregated weekly usage data
pub async fn load_weekly_stats(local_db_path: &Path, date: Date) -> Result<WeeklyStats> {
    let mut conn = db::open_local_ro(local_db_path).await?;

    // Calculate week boundaries (ISO 8601: Monday=start, Sunday=end)
    let week_start = week_start_date(date);
    let week_end = week_start + Duration::days(6);

    // Convert to Unix timestamps for database queries
    // Use midnight UTC for boundaries
    let week_start_ts = OffsetDateTime::new_utc(week_start, time::Time::MIDNIGHT)
        .unix_timestamp();
    // End of last day of the week
    let week_end_ts = OffsetDateTime::new_utc(
        week_end,
        time::Time::from_hms(23, 59, 59).expect("valid time"),
    )
    .unix_timestamp();

    // Query: All usage records in the week, joined with app names
    let mut stats = WeeklyStats::new(week_start);

    // Build and execute query manually to avoid compile-time DB check
    let rows = sqlx::query_as::<_, (String, i64, Option<i64>)>(
        r#"
        SELECT
            a.name as app_name,
            u.start_time,
            SUM(u.amount) as total_duration
        FROM usage u
        JOIN apps a ON u.app_id = a.id
        WHERE u.start_time >= ? AND u.start_time <= ?
        GROUP BY a.id, date(u.start_time, 'unixepoch')
        ORDER BY a.name, u.start_time
        "#,
    )
    .bind(week_start_ts)
    .bind(week_end_ts + 86400) // Include full last day
    .fetch_all(&mut conn)
    .await?;

    // Aggregate into WeeklyStats
    for (app_name, start_time, total_duration) in rows {
        if let Ok(day_date) = OffsetDateTime::from_unix_timestamp(start_time).map(|dt| dt.date())
        {
            let duration = total_duration.unwrap_or(0) as u64;
            stats.add_usage(app_name, day_date, duration);
        }
    }

    Ok(stats)
}

/// Helper: Calculate the Monday of the week containing `date` (ISO 8601).
#[inline]
fn week_start_date(date: Date) -> Date {
    let days_from_monday = date.weekday().number_days_from_monday() as i64;
    date - Duration::days(days_from_monday)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;
    use time::Weekday;

    #[test]
    fn test_week_start() {
        // 2026-03-31 is a Tuesday
        let date = date!(2026 - 03 - 31);
        let ws = week_start_date(date);
        // Monday before should be 2026-03-30
        assert_eq!(ws.weekday(), Weekday::Monday);
    }
}
