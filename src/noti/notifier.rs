use crate::config::Config;
use crate::noti::Announcement;
use crate::noti::ghclient::GitHubClient;
use crate::summarizer::summary::Summary;

use chrono::Local;
use indexmap::IndexSet;
use log::{debug, error, info};
use quick_xml::se::to_string;
use serde::Serialize;
use serde_json::json;
use std::collections::HashMap;
use std::env::temp_dir;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use uuid::Uuid;

const NOTICRAWL_GUID_TAG_NAMESPACE: &str = "tag:noticrawl,2026:rss";
const NOTICRAWL_RSS_FILE_PREFIX: &str = "noticrawl-rss-";

#[derive(Debug, Serialize)]
#[serde(rename = "rss")]
struct Rss {
    #[serde(rename = "@xmlns:noticrawl")]
    xmlns_noticrawl: String,
    channel: Channel,
}
#[derive(Debug, Serialize)]
struct Channel {
    title: String,
    #[serde(rename = "noticrawl:version")]
    version: String,
    #[serde(rename = "noticrawl:pub_datetime")]
    pub_datetime: String,
    item: Vec<BriefItem>,
}
#[derive(Debug, Serialize)]
struct BriefItem {
    title: String,
    link: String,
    guid: String,
    #[serde(rename = "noticrawl:end_date")]
    end_date: String,
    #[serde(rename = "noticrawl:source")]
    source: String,
    #[serde(rename = "noticrawl:eligibilities")]
    eligibilities: Vec<String>,
    #[serde(rename = "noticrawl:benefits")]
    benefits: Vec<String>,
}

pub struct Notifier {
    ann_summary: Summary,
    ghclient: GitHubClient,
}

impl Notifier {
    pub fn new(
        mut summary: Summary,
        ann_set: &IndexSet<Announcement>,
        ghclient: GitHubClient,
    ) -> Self {
        // The summarized data that parsed from AI's response is omitting some of the optional fields
        // source, url, begin date and end date. To fill those information out, we pick the original
        // information from the IndexSet and copying it over to the summarized data.
        let hm: HashMap<i64, &Announcement> = ann_set
            .iter()
            .map(|ann| (ann.id.expect("ID should be set"), ann))
            .collect();

        for brief in &mut summary.summarized {
            let original = hm
                .get(&brief.id)
                .unwrap_or_else(|| panic!("Failed to find announcement id: {}", brief.id));

            brief.source = Some(original.post_source.descriptive_name().to_string());
            brief.url = original.post_url.clone();
            brief.email = original.email.clone();
            brief.begin_date = Some(original.begin_date);
            brief.end_date = Some(original.end_date);
        }

        Self {
            ann_summary: summary,
            ghclient: ghclient,
        }
    }

    fn summary_to_rss(&self, datetime_str: &str) -> Rss {
        let conf = Config::instance();
        let channel_title = self.ann_summary.summary_title.clone();
        let mut brief_items: Vec<BriefItem> = vec![];

        for b in self.ann_summary.summarized.iter() {
            let random_uuid = Uuid::new_v4();
            let guid = format!("{}:{}", NOTICRAWL_GUID_TAG_NAMESPACE, random_uuid);
            let eligibilities = b.eligibilities.iter().cloned().collect();
            let benefits = b.benefits.iter().cloned().collect();

            brief_items.push(BriefItem {
                title: b.project_title.clone(),
                link: b
                    .url
                    .clone()
                    .or_else(|| b.email.clone())
                    .unwrap_or_default(),
                source: b.source.clone().unwrap(),
                guid: guid,
                end_date: b
                    .end_date
                    .as_ref()
                    .map(|d| d.format("%Y-%m-%d").to_string())
                    .unwrap_or_default(),
                eligibilities,
                benefits,
            });
        }

        Rss {
            xmlns_noticrawl: NOTICRAWL_GUID_TAG_NAMESPACE.to_string(),
            channel: Channel {
                title: channel_title,
                version: conf.noticrawl_version.clone(),
                pub_datetime: datetime_str.to_string(),
                item: brief_items,
            },
        }
    }

    /// Send a notification with a title, content and url to Teams.
    pub async fn notify_teams(
        &self,
        teams_url: Option<&str>,
        message: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        debug!("Notifying teams...");
        let conf = Config::instance();

        // Design preview is available from https://adaptivecards.io/designer/
        let payload = json!(
        {
        "type": "message",
        "attachments": [
            {
            "contentType": "application/vnd.microsoft.card.adaptive",
            "content": {
                "type": "AdaptiveCard",
                "version": "1.4",
                "$schema": "http://adaptivecards.io",
                "body": [
                {
                    "type": "TextBlock",
                    "text": self.ann_summary.summary_title,
                    "weight": "Bolder",
                    "size": "Medium",
                    "wrap": true
                },
                {
                    "type": "TextBlock",
                    "text": message,
                    "wrap": true,
                    "spacing": "Medium"
                },
                ],
                "actions": [
                    {
                        "type": "Action.OpenUrl",
                        "title": "전체 목록 보기",
                        "url": conf.publication.page.clone()
                    }
                ]
            }
            }
        ]});

        // MS teams notification url from the config.
        let webhook = teams_url.unwrap_or_else(|| {
            let conf = Config::instance();
            teams_url.unwrap_or(&conf.notification.url)
        });

        // TODO: Call RSS publisher.

        debug!("Sending a teams message via {}", webhook);
        debug!("Message payload: {}", payload);

        let client = reqwest::Client::new();
        let response = client.post(webhook).json(&payload).send().await?;

        if !response.status().is_success() {
            return Err(format!("Webhook returned status: {}", response.status()).into());
        }

        Ok(())
    }

    /// Create a rss file with a predefined naming convention under temp directory
    /// then return path of it.
    /// * datetime_str: String for the date and time of creation.
    fn write_rss_file(&self, datetime_str: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
        // Convert summary to RSS data.
        let rss_feed = self.summary_to_rss(datetime_str);

        // Naming convention
        // <NOTICRAWL_RSS_FILE_PREFIX>-<YYYYDDMM>-<HHMMSS>.xml
        let mut rss_filepath = temp_dir();
        rss_filepath.push(format!("{NOTICRAWL_RSS_FILE_PREFIX}{datetime_str}.xml"));

        // Creat the RSS XML
        let mut rss_file = File::create(&rss_filepath)?;
        info!("RSS temp file: {}", rss_filepath.display());

        let xml = to_string(&rss_feed)?;
        let output = format!(r#"<?xml version="1.0" encoding="UTF-8"?>{}"#, xml);
        rss_file.write_all(output.as_bytes())?;

        info!("RSS has been successfully written.");
        Ok(rss_filepath)
    }

    /// Publish to GitHub.
    /// archive files under <GITHUB_URL>/archive/noticrawl-rss-xxxxxxxx-xxxxxx.xml
    /// the latest files under <GITHUB_URL>/rss.xml as a copy of the file.
    pub async fn publish(&self) -> Result<PathBuf, Box<dyn std::error::Error>> {
        // Create an RSS file from the content.
        let datetime_str = Local::now().format("%Y%m%d-%H%M%S").to_string();
        let rss_filepath = self.write_rss_file(&datetime_str)?;

        // Upload to archive.
        self.ghclient
            .archive_rss(&rss_filepath, &datetime_str)
            .await
            .map_err(|e| {
                let msg = format!("Failed to archive RSS: {e}");
                error!("{msg}");
                std::io::Error::other(msg)
            })?;

        // Update rss.xml
        self.ghclient
            .update_rss(&rss_filepath, &datetime_str)
            .await
            .map_err(|e| {
                let msg = format!("Failed to update rss.xml: {e}");
                error!("{msg}");
                std::io::Error::other(msg)
            })?;

        info!("Published!");
        Ok(rss_filepath)
    }
}

#[cfg(test)]
mod noti_tests {
    use super::*;
    use crate::{collectors::convert_to_date, noti::Announcement, summarizer::brief::Brief};
    use std::env::set_var;
    use std::fs::remove_file;

    // Utility function to create Brief conveniently.
    // Returns Original Announcement in IndexSet and Brief info
    fn fake_briefs(
        id: i64,
        num_eligibilities: i64,
        num_benefits: i64,
    ) -> (IndexSet<Announcement>, Brief) {
        let mut benefits: Vec<String> = vec![];
        for i in 0..num_benefits {
            benefits.push(format!("BENEFIT{}", i));
        }

        let mut eligibilities: Vec<String> = vec![];
        for i in 0..num_eligibilities {
            eligibilities.push(format!("ELIGIBILITY{}", i));
        }

        let mut ann_set = IndexSet::new();
        ann_set.insert(Announcement {
            id: Some(id),
            post_id: Some("123".to_string()),
            post_source: crate::collectors::PostSource::UNKNOWN,
            post_url: Some(format!("https://www.example{}.com", id)),
            email: None,
            title: format!("ANNOUNCEMENT_TITLE{}", id),
            content: format!("ANNOUNCEMENT_CONTENT{}", id),
            begin_date: convert_to_date("20260101"),
            end_date: convert_to_date("20261231"),
        });
        (
            ann_set,
            Brief {
                id,
                project_title: format!("PROJECT{}", id),
                eligibilities: eligibilities,
                benefits: benefits,
                source: None,
                url: None,
                email: None,
                begin_date: None,
                end_date: None,
            },
        )
    }

    #[tokio::test]
    async fn test_publish_rss() {
        unsafe {
            set_var("PUBLICATION_KEY", "dummy_pub_key");
        }

        let summary = Summary {
            summary_title: "TEST".to_string(),
            summarized: vec![],
        };
        let empty_set = IndexSet::<Announcement>::new();

        let mut server = mockito::Server::new_async().await;
        let gh = GitHubClient::new(Some(&server.url()));
        let n = Notifier::new(summary, &empty_set, gh);

        // archive_rss() — PUT /archive/noticrawl-rss-<datetime>.xml
        let mock_archive = server
            .mock(
                "PUT",
                mockito::Matcher::Regex(r"^/archive/noticrawl-rss-\d{8}-\d{6}\.xml$".to_string()),
            )
            .match_header("content-type", "application/json")
            .match_body(mockito::Matcher::Any)
            .with_status(201)
            .create_async()
            .await;

        // get_file_sha() — GET /rss.xml (returns 404 so SHA is skipped gracefully)
        let mock_sha = server
            .mock("GET", "/rss.xml")
            .with_status(404)
            .create_async()
            .await;

        // update_rss() — PUT /rss.xml
        let mock_rss = server
            .mock("PUT", "/rss.xml")
            .match_header("content-type", "application/json")
            .match_body(mockito::Matcher::Any)
            .with_status(200)
            .create_async()
            .await;

        let result = n.publish().await;
        assert!(result.is_ok(), "publish() failed: {:?}", result.err());

        mock_archive.assert_async().await;
        mock_sha.assert_async().await;
        mock_rss.assert_async().await;
    }

    #[test]
    fn test_rss_file_creation() {
        unsafe {
            set_var("PUBLICATION_KEY", "dummy_pub_key");
        }

        let summary = Summary {
            summary_title: "TEST".to_string(),
            summarized: vec![],
        };
        let empty_set = IndexSet::<Announcement>::new();
        let gh = GitHubClient::new(None);
        let n = Notifier::new(summary, &empty_set, gh);

        // The publish function is expected to write a temp file with the RSS contents in it.
        let fake_datetime_str = "20260425-010203";
        let rss_path = n
            .write_rss_file(fake_datetime_str)
            .expect("Failed to publish in test.");
        assert!(rss_path.exists());
        let rss_filename = rss_path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("RSS file name should be valid UTF-8.");
        assert!(
            rss_filename
                .strip_prefix(NOTICRAWL_RSS_FILE_PREFIX)
                .is_some_and(|suffix| {
                    suffix.ends_with(".xml")
                        && suffix.len() == 19   // 20260425-010203.xml
                        && suffix[..8].chars().all(|ch| ch.is_ascii_digit())
                        && &suffix[8..9] == "-"
                        && suffix[9..15].chars().all(|ch| ch.is_ascii_digit())
                }),
            "Unexpected RSS file name: {rss_filename}"
        );

        // Cleanup the file after the testing.
        remove_file(&rss_path).expect("Should be able to remove the temp file.");
    }

    #[test]
    fn test_summary_to_rss() {
        unsafe {
            set_var("PUBLICATION_KEY", "dummy_pub_key");
        }

        // Fake briefs.
        let (mut ann_set, sum_ann1) = fake_briefs(0, 2, 3);
        let (ann_set2, sum_ann2) = fake_briefs(1, 2, 3);
        let (ann_set3, sum_ann3) = fake_briefs(2, 2, 3);
        let (ann_set4, sum_ann4) = fake_briefs(3, 2, 3);
        let (ann_set5, sum_ann5) = fake_briefs(4, 2, 3);

        ann_set.extend(ann_set2);
        ann_set.extend(ann_set3);
        ann_set.extend(ann_set4);
        ann_set.extend(ann_set5);

        let summary = Summary {
            summary_title: "SUMMARY TITLE".to_string(),
            summarized: vec![sum_ann1, sum_ann2, sum_ann3, sum_ann4, sum_ann5],
        };

        let gh = GitHubClient::new(None);
        let n = Notifier::new(summary, &ann_set, gh);
        let r = n.summary_to_rss("11112233-010203");
        let ch = &r.channel;

        // Check chanel information.
        assert_eq!("SUMMARY TITLE", ch.title);
        assert_eq!(Config::instance().noticrawl_version, ch.version);
        // publish date & time.
        assert!(ch.pub_datetime[..8].chars().all(|ch| ch.is_ascii_digit()));
        assert_eq!("11112233", &ch.pub_datetime[..8]);
        assert_eq!("-", &ch.pub_datetime[8..9]);
        assert_eq!("010203", &ch.pub_datetime[9..15]);

        // Items.
        assert_eq!(5, ch.item.len());
        for (idx, bf) in ch.item.iter().enumerate() {
            assert_eq!(format!("PROJECT{idx}"), bf.title);
            assert!(bf.guid.contains(NOTICRAWL_GUID_TAG_NAMESPACE));
            assert_eq!("Unknown", &bf.source);
            assert!(
                bf.eligibilities
                    .join("\n")
                    .contains(&("ELIGIBILITY".to_string()))
            );
            assert!(bf.benefits.join("\n").contains(&("BENEFIT".to_string())));
        }
    }
}
