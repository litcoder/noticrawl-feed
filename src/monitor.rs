use indexmap::IndexSet;
use log::{debug, error, info, warn};

use crate::collectors::Collector;
use crate::config::Config;
use crate::db::Storage;
use crate::noti::{Announcement, GitHubClient, Notifier};
use crate::summarizer;

pub(crate) struct Monitor;

impl Monitor {
    pub async fn check(
        collectors: &[Box<dyn Collector + Send + Sync>],
        db: &Storage,
    ) -> Result<(), String> {
        // Get a configuration.
        let conf = Config::instance();
        debug!("Entering monitoring cycle...");

        // Collected Announcement items to be notified.
        let mut new_ann_set = IndexSet::new();

        // for all registered collectors.
        for collector in collectors {
            // Collect data.
            info!("Checking source: {}", collector.descriptive_name());
            let announcements: IndexSet<Announcement> =
                collector.fetch_latest().await.map_err(|e| e.to_string())?;
            for announcement in announcements {
                // Is this announcement new?
                let is_new_item = db.is_new(&announcement).await;
                if is_new_item {
                    info!("Found a new announcement.");

                    // Mark to the database to prevent duplication.
                    db.mark_as_notified(&announcement)
                        .await
                        .map_err(|e| e.to_string())?;

                    // Save it for announcement.
                    new_ann_set.insert(announcement);
                } else {
                    debug!("No update, skipping.")
                }
            }
        }

        // Is there a new item to be notified?
        let num_post_ann = new_ann_set.iter().count();
        if num_post_ann > 0 {
            let ann_summary = summarizer::summarize(&new_ann_set)
                .await
                .map_err(|e| e.to_string())?;
            debug!("Summary title: {}", ann_summary.summary_title);
            debug!("Number of announcements: {}", ann_summary.summarized.len());

            let teams_url = &conf.notification.url;
            let gh = GitHubClient::new(None);
            let notifier = Notifier::new(ann_summary, &new_ann_set, gh);

            // RSS publication.
            if conf.publication.publish {
                debug!("Publishing RSS.");
                // File contents that published.
                match notifier.publish().await.map_err(|e| e.to_string()) {
                    Ok(rss_filepath) => {
                        info!("RSS publication was successful!");

                        // Notify to MS teams.
                        if conf.notification.notify {
                            info!("Crafting a summarization message.");
                            // Craft a MS teams notification message.
                            let absmsg = summarizer::abstract_summary(&rss_filepath)
                                .await
                                .map_err(|e| e.to_string())?;

                            debug!("Making a MSTeams notification.");
                            if let Err(e) = notifier
                                .notify_teams(Some(teams_url), &absmsg)
                                .await
                                .map_err(|e| e.to_string())
                            {
                                error!("Failed to notify teams: {e}");
                            }
                        } else {
                            warn!("Skipping MSTeams notification as configuration is set false.");
                        }
                    }
                    Err(e) => {
                        error!("Failed to publish the file.\n{e}");
                    }
                }
            } else {
                warn!("Skip publishing because configuration is set false.");
            }
        }

        info!("{num_post_ann} post(s) have been posted.");
        Ok(())
    }
}
