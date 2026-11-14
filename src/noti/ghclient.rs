use crate::config::Config;
use base64::Engine;
use base64::engine::general_purpose;
use log::{debug, error, info, warn};
use reqwest::Client;
use serde_json::json;
use std::env::var;
use std::fs::read_to_string;
use std::path::PathBuf;
use url::Url;

const NOTICRAWL_AGENT_STRING: &str = "noticrawl";
const NOTICRAWL_FEED_ARCHIVE_MESSAGE: &str = "chore: new rss file has been archived";
const NOTICRAWL_FEED_RSS_UPDATE_MESSAGE: &str = "chore: rss file updated";

pub struct GitHubClient {
    // GitHub PAT.
    publication_key: String,

    // Base URL to the GitHub repo contents.
    pub(super) base_url: Url,
}

impl GitHubClient {
    pub fn new(base_url: Option<&String>) -> Self {
        let conf = Config::instance();

        // PAT for the Publication.
        // Raise panic if not exist.
        let publication_key = var("PUBLICATION_KEY")
            .unwrap_or_else(|_| panic!("Unable to get PUBLICATION_KEY from the environment"));

        let url_str = base_url.unwrap_or(&conf.publication.url);
        match Url::parse(url_str) {
            Ok(base_url) => {
                info!("GitHubClient created successful");
                return Self {
                    base_url,
                    publication_key,
                };
            }
            Err(_) => {
                panic!("Failed to create GitHubClient")
            }
        }
    }

    /// Append paths in the paths then return it in a URL.
    fn append_path_to_base_url(&self, paths: Vec<&str>) -> Url {
        // new url.
        let mut url = self.base_url.clone();
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| "Parse error in url")
                .unwrap();

            // append paths to the url.
            for p in paths {
                segments.push(p);
            }
        }
        debug!("Appended path: {url}");

        url
    }

    /// Returns the Authorization header value for GitHub API calls.
    fn authorization_header_value(&self) -> String {
        if self.publication_key.starts_with("Bearer ") || self.publication_key.starts_with("token ")
        {
            return self.publication_key.clone();
        }

        format!("Bearer {}", self.publication_key)
    }

    /// Returns archive URL from given file name.
    fn archive_url(&self, source_file_path: &PathBuf) -> Url {
        let file_name = source_file_path.file_name().unwrap().to_string_lossy();

        self.append_path_to_base_url(vec!["archive", &file_name])
    }

    /// Returns rss URL from given file name.
    fn rss_url(&self) -> Url {
        // rss_url
        self.append_path_to_base_url(vec!["rss.xml"])
    }

    pub async fn archive_rss(
        &self,
        rss_filepath: &PathBuf,
        datetime_str: &String,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let conf = Config::instance();
        let a_url = self.archive_url(rss_filepath);
        info!("Archiving to {}..", a_url.to_string());

        let rss_content = read_to_string(rss_filepath.as_path())?;
        let encoded_content = general_purpose::STANDARD.encode(rss_content);
        let body = json!({
            "message": format!("{} - {}", NOTICRAWL_FEED_ARCHIVE_MESSAGE, datetime_str),
            "content": encoded_content,
            "branch": conf.publication.branch,
        });

        info!("Posting data..");
        let client = Client::new();
        let response = client
            .put(a_url.clone())
            .header("Authorization", self.authorization_header_value())
            .header("User-Agent", NOTICRAWL_AGENT_STRING)
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        let response_text = response
            .text()
            .await
            .unwrap_or_else(|_| String::from("<failed to read response body>"));

        if !status.is_success() {
            error!(
                "Failed to archive RSS. status={}, body={}",
                status, response_text
            );
            return Err(format!("Failed to archive RSS: {}", status).into());
        }

        info!(
            "RSS file archiving was successful: {}, status={}, body={}",
            a_url, status, response_text
        );
        Ok(())
    }

    pub async fn update_rss(
        &self,
        rss_filepath: &PathBuf,
        datetime_str: &String,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let client = Client::new();
        let url = self.rss_url();
        let content = read_to_string(rss_filepath.as_path())?;
        let encoded = general_purpose::STANDARD.encode(content);
        let mut body = json!({
            "message": format!("{}-{}", NOTICRAWL_FEED_RSS_UPDATE_MESSAGE, datetime_str),
            "content": encoded,
        });
        info!("Updating rss: {url}");

        // Update SHA
        if let Ok(sha) = self.get_file_sha(url.clone()).await {
            info!("SHA fetched: {sha}");
            body["sha"] = json!(sha);
        }

        info!("Updating rss.xml with the SHA..");
        let res = client
            .put(url)
            .header("Authorization", self.authorization_header_value())
            .header("User-Agent", NOTICRAWL_AGENT_STRING)
            .json(&body)
            .send()
            .await?;

        if res.status().is_success() {
            info!("RSS file update successful");
            Ok(())
        } else {
            error!("Failed to update RSS file.");
            Err(format!("Failed to upload: {}", res.status()).into())
        }
    }

    /// Gets SHA of given file.
    async fn get_file_sha(&self, file_url: Url) -> Result<String, Box<dyn std::error::Error>> {
        info!("Getting a SHA for {file_url}");

        let client = Client::new();
        let response = client
            .get(file_url)
            .header("Authorization", self.authorization_header_value())
            .header("User-Agent", NOTICRAWL_AGENT_STRING)
            .send()
            .await
            .unwrap();

        if response.status() == 200 {
            info!("Response 200, successful.");
            let json: serde_json::Value = response.json().await.unwrap();
            if let Some(sha) = json["sha"].as_str() {
                return Ok(sha.to_string());
            }

            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Missing `sha` in GitHub response",
            )
            .into());
        }

        // Return an empty string unless there's an error.
        warn!("The file not exist, returning empty SHA");
        Ok(String::from(""))
    }
}

#[cfg(test)]
mod gh_test {
    use super::*;
    use std::env::set_var;

    #[test]
    fn test_github_default_urls() {
        unsafe {
            set_var("PUBLICATION_KEY", "dummy_pub_key");
        }

        // Use default if not set.
        let gh = GitHubClient::new(None);
        assert_eq!(
            "https://api.github.com/repos/litcoder/noticrawl-feed/contents",
            gh.base_url.as_str()
        );
        assert_eq!(
            "https://api.github.com/repos/litcoder/noticrawl-feed/contents/rss.xml",
            gh.rss_url().to_string()
        );

        // local file path.
        let rss_source_path = PathBuf::from("/tmp/dir/dir2/noticrawl-rss-20260425-010204.xml");
        assert_eq!(
            "https://api.github.com/repos/litcoder/noticrawl-feed/contents/archive/noticrawl-rss-20260425-010204.xml",
            gh.archive_url(&rss_source_path).to_string()
        );
    }

    #[test]
    fn test_user_given_urls() {
        unsafe {
            set_var("PUBLICATION_KEY", "dummy_pub_key");
        }

        // User given custom URL.
        let gh = GitHubClient::new(Some(&"http://some-url.com".to_string()));
        assert_eq!("http://some-url.com/", gh.base_url.as_str());
        assert_eq!("http://some-url.com/rss.xml", gh.rss_url().to_string());

        // local file path.
        let rss_source_path = PathBuf::from("/tmp/dir/dir2/noticrawl-rss-20260425-010204.xml");
        assert_eq!(
            "http://some-url.com/archive/noticrawl-rss-20260425-010204.xml",
            gh.archive_url(&rss_source_path).to_string()
        );
    }
}
