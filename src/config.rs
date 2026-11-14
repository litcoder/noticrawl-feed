use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::sync::OnceLock;
use toml::from_str;

const NOTICRAWL_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Deserialize, Clone)]
pub struct CollectorConfig {
    pub(crate) url: String,
    // Number of announcements to get from the collector.
    // This feature is not always supported by the collector.
    pub(crate) num_announcements: Option<i32>,
    // If the Rest API is supporting search query input.
    pub(crate) search_supported: bool,
    // Search terms for the collector (optional)
    pub(crate) search_terms: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct SqlPathConfig {
    pub(crate) create_table: String,
    pub(crate) check_exist: String,
    pub(crate) upsert: String,
}

#[derive(Debug, Deserialize)]
pub struct DatabaseConfig {
    pub(crate) file_path: String,
    pub(crate) sqlpath: HashMap<String, SqlPathConfig>,
}

#[derive(Debug, Deserialize)]
pub struct NotificationConfig {
    pub notify: bool,
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct PublicationConfig {
    pub publish: bool,
    pub url: String,
    pub page: String,
    pub branch: String,
}

#[derive(Debug, Deserialize)]
pub struct GeminiConfig {
    pub(crate) temperature: f32,
    pub(crate) max_output_tokens: i32,
    pub(crate) connection_timeout_secs: u64,
    pub(crate) request_timeout_secs: u64,
}

#[derive(Debug, Deserialize)]
pub struct CronConfig {
    pub(crate) schedules: Vec<String>,
    pub(crate) time_zone: i32,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(skip)]
    pub noticrawl_version: String,
    pub(crate) cron: CronConfig,
    pub(crate) collector: HashMap<String, CollectorConfig>,
    pub(crate) database: DatabaseConfig,
    pub(crate) notification: NotificationConfig,
    pub(crate) publication: PublicationConfig,
    pub(crate) gemini: GeminiConfig,
}

/// Singleton configuration
static INSTANCE: OnceLock<Config> = OnceLock::new();
impl Config {
    pub fn instance() -> &'static Config {
        INSTANCE.get_or_init(|| {
            let content = fs::read_to_string("config.toml")
                .expect("Failed to read configuration from config.toml.");
            let mut config: Config = from_str(&content).expect("Failed to parse config.toml");

            // Version string
            config.noticrawl_version = NOTICRAWL_VERSION.to_string();
            config
        })
    }
}

#[cfg(test)]
mod config_test {
    use super::*;

    #[test]
    fn test_singleton_config() {
        let inst1 = Config::instance();
        let inst2 = Config::instance();

        assert!(std::ptr::eq(inst1, inst2));
    }

    #[test]
    fn test_kstartup_collector_settings() {
        let conf = Config::instance();

        // K startup URL.
        let kstartup_conf = conf
            .collector
            .get("kstartup")
            .expect("Failed to get Kstartup settings from config");
        assert_eq!(
            "https://apis.data.go.kr/B552735/kisedKstartupService01/getAnnouncementInformation01",
            kstartup_conf.url.as_str()
        );

        // K start up uses default number.
        assert_eq!(None, kstartup_conf.num_announcements);

        // Search query is supported by the REST API.
        assert_eq!(true, kstartup_conf.search_supported);
    }

    #[test]
    fn test_bizinfo_collector_settings() {
        let conf = Config::instance();

        // BizInfo URL.
        let bizinfo_conf = conf
            .collector
            .get("bizinfo")
            .expect("Failed to get bizinfo settings from config");

        assert_eq!(
            "https://www.bizinfo.go.kr/uss/rss/bizinfoApi.do",
            bizinfo_conf.url
        );

        // number of announcements to get
        assert_eq!(10, bizinfo_conf.num_announcements.unwrap());

        // Search query is NOT supported by the REST API.
        assert_eq!(false, bizinfo_conf.search_supported);
    }

    #[test]
    fn test_enara_collector_settings() {
        let conf = Config::instance();

        // eNara URL.
        let enara_conf = conf
            .collector
            .get("enara")
            .expect("Failed to get enara settings from config");

        assert_eq!(
            "https://apis.data.go.kr/1051000/MoefOpenAPI/T_OPD_PBNS",
            enara_conf.url
        );

        // number of announcements to get
        assert_eq!(10, enara_conf.num_announcements.unwrap());

        // Search query is NOT supported by the REST API.
        assert_eq!(true, enara_conf.search_supported);
    }

    #[test]
    fn test_database_sqlpath() {
        let conf = Config::instance();
        if let Some(ann_sqlpath) = conf.database.sqlpath.get("announcement") {
            assert_eq!(
                ann_sqlpath.create_table,
                "sql/create_announcement_table.sql"
            );
            assert_eq!(ann_sqlpath.check_exist, "sql/check_announcement_exist.sql");
            assert_eq!(ann_sqlpath.upsert, "sql/upsert_announcement.sql");
        }
    }

    #[test]
    fn test_database_file_name_in_config() {
        let conf = Config::instance();
        assert_eq!("noticrawl.db", conf.database.file_path);
    }

    #[test]
    fn test_noti_config() {
        let conf = Config::instance();

        // Notification (MS teams)
        assert_eq!(
            r#"https://default9839fb39bbfe4b59937e2689d16653.67.environment.api.powerplatform.com:443/powerautomate/automations/direct/workflows/2feda4cfffa848068083d1aed701400a/triggers/manual/paths/invoke?api-version=1&sp=%2Ftriggers%2Fmanual%2Frun&sv=1.0&sig=Kjbc3qCgx-Jfjth5_mh_kCoVqLvc2-t3RjXHwFIgoTY"#,
            conf.notification.url
        );
        assert!(conf.notification.notify);

        // Publication
        assert_eq!(
            r#"https://api.github.com/repos/litcoder/noticrawl-feed/contents"#,
            conf.publication.url
        );
        assert!(conf.publication.publish);

        // branch
        assert_eq!("main", conf.publication.branch);

        // Github page URL
        assert_eq!(
            "https://litcoder.github.io/noticrawl-feed/",
            conf.publication.page
        )
    }

    #[test]
    fn test_gemini_config() {
        let conf = Config::instance();
        assert!(1.0 != conf.gemini.temperature);
        assert!(1000 != conf.gemini.max_output_tokens);
        assert!(0 < conf.gemini.connection_timeout_secs);
        assert!(0 < conf.gemini.request_timeout_secs);
    }

    #[test]
    fn test_cron_config() {
        let conf = Config::instance();

        // Schedules
        assert_eq!("0 30 17 * * *", conf.cron.schedules[0]);

        // Time zone.
        assert_eq!(9 * 60 * 60, conf.cron.time_zone);
    }
}
