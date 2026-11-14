use async_trait::async_trait;
use indexmap::IndexSet;
use reqwest::{Client, ClientBuilder, RequestBuilder, Response};
use serde::Serialize;
use std::error::Error;

use crate::noti::Announcement;

#[async_trait]
pub(super) trait SummaryProvider {
    type RequestBody: Serialize + Send + Sync;
    type SummaryOutput;

    /// Returns URL for the summary service.
    /// # Returns
    /// Requesting URL for the summary.
    fn query_url(&self) -> &'static str;

    /// Convert the Announcements into single String.
    /// # Arguments
    /// * announcements: Announcement set to be summarized.
    ///
    /// # Returns
    /// Combined string that will be fed to the summarizer.
    fn as_query_string(&self, announcements: &IndexSet<Announcement>) -> String {
        let mut ann_text = String::new();

        for ann in announcements {
            let ann_id = ann.id.expect("Announcement should have unique ID");
            let article = format!(
                "Announcement {}\nTitle: {}\nContent: {}\n\n",
                ann_id, ann.title, ann.content,
            );

            ann_text.push_str(&article);
        }
        ann_text
    }

    /// Creates request body in JSON.
    ///
    /// # Arguments
    /// * prompt: summary direction prompt.
    /// * query: query string that will be transferred to server.
    ///
    /// # Returns
    /// Formatted JSON that includes the given query argument.
    fn build_request(&self, prompt: &str, query: &str) -> Self::RequestBody;

    /// Allows each provider to configure shared HTTP client options.
    fn configure_client(&self, builder: ClientBuilder) -> Result<ClientBuilder, Box<dyn Error>> {
        Ok(builder)
    }

    /// Builds the outbound HTTP request for provider-specific auth/headers/query params.
    fn build_http_request(
        &self,
        client: &Client,
        url: &str,
        body: &Self::RequestBody,
    ) -> Result<RequestBuilder, Box<dyn Error>> {
        Ok(client.post(url).json(body))
    }

    /// Handles server response.
    ///
    /// # Arguments
    /// * response: Response data to be parsed.
    ///
    /// # Returns
    /// Summary type output.
    async fn process_response(
        &self,
        response: Response,
    ) -> Result<Self::SummaryOutput, Box<dyn Error>>;
}

#[cfg(test)]
mod summarizer_interface {
    use super::*;
    use crate::collectors::{PostSource, convert_to_date};
    use crate::noti::Announcement;
    use serde_json::{Value, json};

    struct MockSummaryProvider;

    #[async_trait]
    impl SummaryProvider for MockSummaryProvider {
        type RequestBody = Value;
        type SummaryOutput = ();

        fn query_url(&self) -> &'static str {
            "https://example.com/summarize"
        }

        fn build_request(&self, prompt: &str, query: &str) -> Self::RequestBody {
            json!({
                "prompt": prompt,
                "query": query,
            })
        }

        async fn process_response(
            &self,
            _response: Response,
        ) -> Result<Self::SummaryOutput, Box<dyn Error>> {
            Ok(())
        }
    }

    #[test]
    fn test_query_string_builder_check_original_id() {
        let provider = MockSummaryProvider;
        let mut anns = IndexSet::new();

        anns.insert(Announcement {
            id: Some(101),
            post_id: Some("345".to_string()),
            post_source: PostSource::KSTARTUP,
            post_url: Some("kstartup.example.com".to_string()),
            email: None,
            title: "KStartup title".to_string(),
            content: "KStartup content".to_string(),
            begin_date: convert_to_date("20260101"),
            end_date: convert_to_date("20260101"),
        });

        anns.insert(Announcement {
            id: Some(201),
            post_id: Some("223".to_string()),
            post_source: PostSource::BIZINFO,
            post_url: Some("bizinfo.example.com".to_string()),
            email: None,
            title: "BizInfo title".to_string(),
            content: "BizInfo content".to_string(),
            begin_date: convert_to_date("20260101"),
            end_date: convert_to_date("20261231"),
        });

        // Convert to query string and check the contents.
        let query = provider.as_query_string(&anns);

        // ID
        assert!(query.contains("Announcement 101"));
        assert!(query.contains("Announcement 201"));

        // TItle
        assert!(query.contains("Title: KStartup title"));
        assert!(query.contains("Title: BizInfo title"));

        // Contents
        assert!(query.contains("Content: KStartup content"));
        assert!(query.contains("Content: BizInfo content"));
    }
}
