use chrono::FixedOffset;
use dotenvy::dotenv;
use env_logger::{Builder, Env};
use log::{error, info};
use std::sync::{Arc, Once};
use std::time::Duration;
use tokio_cron_scheduler::{Job, JobScheduler};

use crate::collectors::Collector;
use crate::collectors::bizinfo::BizInfoCollector;
use crate::collectors::enara::EnaraCollector;
use crate::collectors::interface::FilterPolicy;
use crate::collectors::kstartup::KStartupCollector;
use crate::collectors::naramarket::NaraMarketCollector;
use crate::collectors::smtech::SmTechCollector;
use crate::config::Config;
use crate::db::Storage;
use crate::monitor::Monitor;

mod collectors;
mod config;
mod db;
mod monitor;
mod noti;
mod summarizer;

/// Initializer
/// This can be called multiple times for initialization.
static MAIN_INIT: Once = std::sync::Once::new();
pub fn init_main() {
    MAIN_INIT.call_once(|| {
        // Environment variables.
        dotenv().ok();

        // Logger configuration.
        let log_filter = Env::new().default_filter_or("debug");
        Builder::from_env(log_filter).init();
    });
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // initialization.
    init_main();

    // Single-shot mode.
    let is_singleshot = std::env::args().any(|arg| arg == "--singleshot");

    info!("Starting Noticrawl...");

    // Get config.
    let conf = Config::instance();

    // Initialize database and shared resources
    let db = Arc::new(Storage::new(&conf.database.file_path).await?);

    // Apply filter policy.
    let filter_policy = Some(FilterPolicy::new(None, true));

    let collectors: Arc<Vec<Box<dyn Collector + Send + Sync>>> = Arc::new(vec![
        // Collectors.
        Box::new(KStartupCollector::new(None, filter_policy.clone())),
        Box::new(BizInfoCollector::new(None, filter_policy.clone())),
        Box::new(EnaraCollector::new(None, filter_policy.clone())),
        Box::new(NaraMarketCollector::new(None, filter_policy.clone())),
        Box::new(SmTechCollector::new(None, filter_policy.clone())),
    ]);

    // Single-shot
    if is_singleshot {
        info!("Running in single-shot mode. Executing one monitoring cycle and exiting.");
        Monitor::check(&collectors, &db)
            .await
            .map_err(std::io::Error::other)?;
        info!("Single-shot monitoring cycle completed. Exiting.");
        return Ok(());
    }

    // Setup the scheduler.
    let sched = JobScheduler::new().await?;
    let tz_offset =
        FixedOffset::east_opt(conf.cron.time_zone).expect("Time zone offset should be valid");

    // Schedule monitoring from config (6-field cron: sec min hour dom month dow).
    for cron_schedule in &conf.cron.schedules {
        let collectors_ref = Arc::clone(&collectors);
        let db_ref = Arc::clone(&db);

        let job = Job::new_async_tz(cron_schedule, tz_offset.clone(), move |_uuid, _l| {
            let collectors_ref = Arc::clone(&collectors_ref);
            let db_ref = Arc::clone(&db_ref);

            Box::pin(async move {
                info!("Starting monitoring cycle...");
                if let Err(e) = Monitor::check(&collectors_ref, &db_ref)
                    .await
                    .map_err(|e| e.to_string())
                {
                    error!("Error during the monitoring cycle: {}", e);
                }
            })
        })?;
        sched.add(job).await?;
    }
    sched.start().await?;

    info!(
        "Scheduler is active in UTC+{}. Monitoring runs with schedules: {:?}",
        conf.cron.time_zone / (60 * 60),
        conf.cron.schedules
    );

    // Keep the main thread alive.
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}
