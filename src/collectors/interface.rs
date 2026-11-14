use crate::config::{CollectorConfig, Config};
use crate::noti::Announcement;

use async_trait::async_trait;
use indexmap::IndexSet;
use log::{debug, info};
use reqwest::Client;
use serde_json::Value;
use std::error::Error;
use std::sync::Once;
use std::sync::atomic::{AtomicI64, Ordering};

pub use super::filter_policy::FilterPolicy;
pub use super::types::{CollectorContext, PostSource};

// Counter for unique id number.
static ID_COUNTER: AtomicI64 = AtomicI64::new(0);

#[async_trait]
pub trait Collector {
    /// Environment variable name for this collector's API key.
    fn api_key_env_name() -> &'static str
    where
        Self: Sized;

    /// Source type for the collector.
    fn source(&self) -> PostSource {
        self.context().source()
    }

    /// Type-level source for static/default constructor flows.
    fn type_source() -> PostSource
    where
        Self: Sized;

    /// Per-collector once guard used by common constructor flow.
    fn init_once_guard() -> &'static Once
    where
        Self: Sized;

    /// Build concrete collector from a prepared collector context.
    fn from_context(context: CollectorContext) -> Self
    where
        Self: Sized;

    /// Build concrete collector from loaded config.
    fn from_config(
        base_url: Option<String>,
        filter_policy: Option<FilterPolicy>,
        config: &CollectorConfig,
    ) -> Self
    where
        Self: Sized,
    {
        let context = CollectorContext::new(
            Self::type_source(),
            base_url.unwrap_or_else(|| config.url.clone()),
            Self::load_api_key(),
            filter_policy,
            Self::context_search_terms(config),
        );
        Self::from_context(context)
    }

    /// Common collector configuration shared by concrete collectors.
    fn context(&self) -> &CollectorContext;

    /// Returns type-level identifier of the collector.
    /// Note: the name shouldn't have a space inside. This will be used as a hashmap key.
    fn type_identifier() -> &'static str
    where
        Self: Sized,
    {
        Self::type_source().identifier()
    }

    /// Returns object-safe identifier of the collector.
    fn identifier(&self) -> &'static str {
        self.source().identifier()
    }

    fn unique_id(&self) -> i64 {
        ID_COUNTER.fetch_add(1, Ordering::Relaxed)
    }

    /// A human-readable descriptive name for this collector .
    /// Derived from source() so collectors don't duplicate string literals.
    fn descriptive_name(&self) -> &'static str {
        self.source().descriptive_name()
    }

    /// Log message displayed on initialization.
    fn init_message() -> String
    where
        Self: Sized,
    {
        format!(
            "{} collector has been initialized.",
            Self::type_identifier()
        )
    }

    /// Initialize collector resources exactly once for each concrete collector.
    fn initialize_once(init: &Once)
    where
        Self: Sized,
    {
        init.call_once(|| {
            dotenvy::dotenv().ok();
            info!("{}", Self::init_message());
        });
        println!("{} collector initialized.", Self::type_identifier());
    }

    /// Common constructor for all REST API collectors.
    fn new(base_url: Option<String>, filter_policy: Option<FilterPolicy>) -> Self
    where
        Self: Sized,
    {
        Self::initialize_once(Self::init_once_guard());
        Self::from_config(base_url, filter_policy, Self::load_config())
    }

    /// Load API key from environment, panicking with a descriptive message if missing.
    fn load_api_key() -> String
    where
        Self: Sized,
    {
        std::env::var(Self::api_key_env_name()).unwrap_or_else(|_| {
            panic!(
                "API key for {} collector is not set via {}",
                Self::type_identifier(),
                Self::api_key_env_name()
            )
        })
    }

    /// Load collector settings from config, panicking if not found.
    /// Convenience method to reduce boilerplate in collector implementations.
    fn load_config() -> &'static CollectorConfig
    where
        Self: Sized,
    {
        let conf = Config::instance();
        conf.collector.get(Self::type_identifier()).expect(&format!(
            "Configuration for {}({}) collector not found",
            Self::type_source().descriptive_name(),
            Self::type_identifier()
        ))
    }

    /// Search terms loaded into collector context.
    /// Most collectors use configured terms as-is, while non-search collectors can override.
    fn context_search_terms(config: &CollectorConfig) -> Option<Vec<String>>
    where
        Self: Sized,
    {
        config.search_terms.clone()
    }

    /// Returns search terms vector.
    fn search_terms(&self) -> Option<&[String]> {
        self.context().search_terms()
    }

    /// Returns currently set information fetching URL.
    fn base_url(&self) -> &str {
        self.context().base_url()
    }

    /// REST API collectors should register their API keys.
    fn api_key(&self) -> &str {
        self.context().api_key()
    }

    /// Returns the list of raw JSON items from the HTTP response body.
    fn response_items<'a>(&self, body: &'a Value) -> Option<&'a [Value]>;

    /// Maps a raw JSON item to the common Announcement type.
    fn map_item_to_announcement(&self, item: &Value) -> Option<Announcement>;

    /// Optional collector-specific filter hook.
    /// Default behavior keeps all announcements that passed the shared filter.
    fn collector_specific_keep(&self, _announcement: &Announcement) -> bool {
        true
    }

    /// Returns query parameters as key-value pairs.
    /// Each collector defines its own parameter structure.
    ///
    /// # Arguments
    /// * search_term: Search term to be returned as a REST API parameter value.
    ///
    /// # Returns
    /// Rest API parameters and search term in case it's given.
    fn query_params(&self, search_term: Option<&str>) -> Vec<(&str, String)>;

    /// Gets fetching URLs by combining base address with query parameters.
    /// For the cases where the server doesn't support multiple term searching,
    /// this function returns multiple URLs for the search words respectively.
    fn get_fetch_urls(&self) -> Vec<String> {
        // Configure if the collector supports search query in Rest API.
        let conf = Config::instance()
            .collector
            .get(self.identifier())
            .expect("Configuration for collector not found");

        let mut fetch_urls = vec![];
        let addr = self.base_url();
        let build_url = |params: Vec<(&str, String)>| {
            let query_string = params
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("&");
            format!("{}?{}", addr, query_string)
        };

        if conf.search_supported
            && let Some(search_words) = self.search_terms()
        {
            for sw in search_words {
                let params = self.query_params(Some(sw.as_str()));
                fetch_urls.push(build_url(params));
            }
        } else {
            let params = self.query_params(None);
            fetch_urls.push(build_url(params));
        }
        fetch_urls
    }

    /// Shared fetch implementation for REST API collectors.
    async fn fetch_latest_from_api(&self) -> Result<IndexSet<Announcement>, Box<dyn Error>> {
        let mut announcements = IndexSet::new();

        info!(
            "Collector '{}' is getting the latest...",
            self.descriptive_name()
        );

        let client = Client::new();
        let fetch_urls = self.get_fetch_urls();
        let ctx = self.context();

        for url in fetch_urls {
            debug!("Fetching URL:'{}'", url);

            let resp = client.get(&url).send().await?;
            let body: Value = resp.json().await?;

            if let Some(items) = self.response_items(&body) {
                for item in items {
                    if let Some(mut announcement) = self.map_item_to_announcement(item) {
                        // Apply filter if available.
                        let should_keep = if let Some(filter_policy) = &ctx.filter_policy {
                            let keep = filter_policy.should_keep(&announcement, ctx.search_terms());
                            if !keep {
                                debug!("'{}' has been filtered out.", announcement.title);
                            }
                            keep
                        } else {
                            info!("Filter is not set, add everything.");
                            true
                        };

                        if should_keep && self.collector_specific_keep(&announcement) {
                            // Collector should set None to the id so that unique id can be given at here.
                            assert!(&announcement.id.is_none());

                            // Set a unique identifier.
                            announcement.id = Some(self.unique_id());
                            announcements.insert(announcement);
                        }
                    }
                }
            }
        }

        Ok(announcements)
    }

    /// Do fetch the latest information from the web.
    async fn fetch_latest(&self) -> Result<IndexSet<Announcement>, Box<dyn Error>> {
        self.fetch_latest_from_api().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::noti::Announcement;
    use async_trait::async_trait;
    use mockito::Server;
    use serde_json::Value;
    use std::sync::Once;

    static TEST_COLLECTOR_INIT: Once = Once::new();

    struct TestCollector {
        context: CollectorContext,
    }

    #[async_trait]
    impl Collector for TestCollector {
        fn api_key_env_name() -> &'static str {
            "TEST_KEY"
        }

        fn type_identifier() -> &'static str {
            "test_collector"
        }

        fn identifier(&self) -> &'static str {
            Self::type_identifier()
        }

        fn type_source() -> PostSource {
            PostSource::UNKNOWN
        }

        fn init_once_guard() -> &'static Once {
            &TEST_COLLECTOR_INIT
        }

        fn context(&self) -> &CollectorContext {
            &self.context
        }

        fn from_context(context: CollectorContext) -> Self {
            Self { context }
        }

        fn response_items<'a>(&self, body: &'a Value) -> Option<&'a [Value]> {
            let items = body.as_array()?;
            Some(items.as_slice())
        }

        fn map_item_to_announcement(&self, item: &Value) -> Option<Announcement> {
            let number: u32 = item.get("no")?.as_str()?.parse().ok()?;
            let start_date = item.get("start_date")?.as_str()?;
            let end_date = item.get("end_date")?.as_str()?;

            Some(Announcement {
                id: None,
                post_id: Some(format!("{}", number)),
                post_source: PostSource::UNKNOWN,
                post_url: Some(format!("http://example{:02}.com", number)),
                email: None,
                title: format!("title{:02}", number),
                content: format!("content{:02}", number),
                begin_date: crate::collectors::convert_to_date(start_date),
                end_date: crate::collectors::convert_to_date(end_date),
            })
        }

        fn query_params(&self, _search_term: Option<&str>) -> Vec<(&str, String)> {
            vec![]
        }

        fn get_fetch_urls(&self) -> Vec<String> {
            vec![self.base_url().to_string()]
        }
    }

    #[test]
    fn test_post_source_enum_string() {
        let kstartup = PostSource::KSTARTUP;
        assert_eq!("KStartup", kstartup.descriptive_name());
        assert_eq!("kstartup", kstartup.identifier());

        let bizinfo = PostSource::BIZINFO;
        assert_eq!("기업마당", bizinfo.descriptive_name());
        assert_eq!("bizinfo", bizinfo.identifier());

        let enara = PostSource::ENARA;
        assert_eq!("e나라도움", enara.descriptive_name());
        assert_eq!("enara", enara.identifier());

        let naramarket = PostSource::NARAMARKET;
        assert_eq!("나라장터", naramarket.descriptive_name());
        assert_eq!("naramarket", naramarket.identifier());

        let smtech = PostSource::SMTECH;
        assert_eq!("중소벤처24", smtech.descriptive_name());
        assert_eq!("smtech", smtech.identifier());
    }

    #[tokio::test]
    async fn test_contain_unique_sequence_id() {
        let mut server = Server::new_async().await;
        let _m1 = server
            .mock("GET", "/test_url1")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"no": "1", "start_date": "20260101", "end_date": "20260102"}]"#)
            .create();

        let _m2 = server
            .mock("GET", "/test_url2")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"no": "2", "start_date": "20260101", "end_date": "20260105"}]"#)
            .create();

        let c1 = TestCollector {
            context: CollectorContext::new(
                PostSource::UNKNOWN,
                format!("{}/test_url1", server.url()),
                "SOME_API_KEY".to_string(),
                None,
                None,
            ),
        };
        let ann_set1 = c1.fetch_latest().await.unwrap();

        let c2 = TestCollector {
            context: CollectorContext::new(
                PostSource::UNKNOWN,
                format!("{}/test_url2", server.url()),
                "SOME_API_KEY".to_string(),
                None,
                None,
            ),
        };
        let ann_set2 = c2.fetch_latest().await.unwrap();

        assert!(ann_set1[0].id.is_some());
        assert!(ann_set2[0].id.is_some());
        assert_ne!(ann_set1[0].id.unwrap(), ann_set2[0].id.unwrap());
    }

    #[tokio::test]
    async fn test_filter_applied_to_announcement() {
        // A mock server.
        // idx: starting and end date of the announcements.
        let mut server = Server::new_async().await;
        server
            .mock("GET", "/api")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"[
             {"no": "1", "start_date": "20250101", "end_date": "20250102"},
             {"no": "2", "start_date": "20260101", "end_date": "20260103"},
             {"no": "3", "start_date": "20260101", "end_date": "20260104"},
             {"no": "4", "start_date": "20260101", "end_date": "20260105"},
             {"no": "5", "start_date": "20260101", "end_date": "20260106"}
              ]"#,
            )
            .create();

        ////
        // A collector without a filter.
        ////
        let no_filter_collector = TestCollector::from_context(CollectorContext::new(
            PostSource::UNKNOWN,
            format!("{}/api", server.url()),
            "SOME_API_KEY".to_string(),
            None,
            None,
        ));
        let announcements = no_filter_collector.fetch_latest().await.unwrap();
        assert_eq!(5, announcements.len());

        ////
        // A collector with a date filter.
        ////
        let date_filter_collector = TestCollector::from_context(CollectorContext::new(
            PostSource::UNKNOWN,
            format!("{}/api", server.url()),
            "SOME_API_KEY".to_string(),
            Some(FilterPolicy::new(Some("20260104"), false)),
            None,
        ));
        // Should be filtered by the day.
        let announcements = date_filter_collector.fetch_latest().await.unwrap();
        assert_eq!(2, announcements.len());

        ////
        // A collector with search words.
        ////
        let word_filter_collector = TestCollector::from_context(CollectorContext::new(
            PostSource::UNKNOWN,
            format!("{}/api", server.url()),
            "SOME_API_KEY".to_string(),
            Some(FilterPolicy::new(None, true)),
            Some(vec![
                "title02".to_string(),
                "THIS NAVER MATCHES".to_string(),
            ]),
        ));
        // Should be filtered by the keyword.
        let announcements = word_filter_collector.fetch_latest().await.unwrap();
        assert_eq!(1, announcements.len());
        assert_eq!(Some("2".to_string()), announcements[0].post_id);

        ////
        // A collector with day and search words.
        ////
        let word_filter_collector = TestCollector::from_context(CollectorContext::new(
            PostSource::UNKNOWN,
            format!("{}/api", server.url()),
            "SOME_API_KEY".to_string(),
            Some(FilterPolicy::new(Some("20260104"), true)),
            Some(vec!["title".to_string(), "THIS NAVER MATCHES".to_string()]),
        ));
        // Should be filtered by the keyword.
        let announcements = word_filter_collector.fetch_latest().await.unwrap();
        assert_eq!(2, announcements.len());
        assert_eq!(Some("4".to_string()), announcements[0].post_id);
        assert_eq!(Some("5".to_string()), announcements[1].post_id);
    }
}
