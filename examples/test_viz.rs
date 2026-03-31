// Quick test of the visualization module
use screenhistory::viz::load_weekly_stats;
use std::path::PathBuf;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let db_path = PathBuf::from(format!("{}/.screenhistory.sqlite", home));

    println!("Testing load_weekly_stats from: {:?}", db_path);
    println!("Current date: {}", time::OffsetDateTime::now_utc().date());

    let stats = load_weekly_stats(&db_path, time::OffsetDateTime::now_utc().date()).await?;

    println!("\n Week Start: {}", stats.week_start);
    println!("Apps count: {}", stats.apps.len());

    let sorted_apps = stats.sorted_app_names();
    println!("\nFirst 10 apps:");

    for (idx, app) in sorted_apps.iter().take(10).enumerate() {
        let usages = &stats.apps[app];
        let total: u64 = usages.iter().map(|u| u.duration_secs).sum();
        println!(
            "  {}. {} - {} entries, {} total hours",
            idx + 1,
            app,
            usages.len(),
            total / 3600
        );
        for usage in usages.iter().take(3) {
            println!(
                "     {} at {}: {} mins",
                usage.day,
                "00:00",
                usage.duration_secs / 60
            );
        }
    }

    Ok(())
}
