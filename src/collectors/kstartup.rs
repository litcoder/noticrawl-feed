use crate::collectors::{Collector, CollectorContext, PostSource};
use crate::noti::Announcement;

use async_trait::async_trait;
use serde_json::Value;
use std::sync::Once;

static KSTARTUP_COLLECTOR_INIT: Once = Once::new();

pub struct KStartupCollector {
    context: CollectorContext,
}

impl KStartupCollector {
    pub fn from_context(context: CollectorContext) -> Self {
        Self { context }
    }

    /// Maps JSON responded data to the Announcement struct.
    fn map_item_to_announcement(item: &Value) -> Option<Announcement> {
        let title = item.get("intg_pbanc_biz_nm")?.as_str()?.to_string();
        let content = item.get("pbanc_ctnt")?.as_str()?.to_string();
        let post_id = Some(item.get("id")?.as_i64()?.to_string());
        let post_url = item
            .get("detl_pg_url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let begin_date = super::try_convert_to_date(item.get("pbanc_rcpt_bgng_dt")?.as_str()?)?;
        let end_date = super::try_convert_to_date(item.get("pbanc_rcpt_end_dt")?.as_str()?)?;

        Some(Announcement {
            id: None,
            post_id,
            post_source: PostSource::KSTARTUP,
            post_url,
            email: None,
            title,
            content,
            begin_date,
            end_date,
        })
    }
}

#[async_trait]
impl Collector for KStartupCollector {
    fn api_key_env_name() -> &'static str {
        "KSTARTUP_KEY"
    }

    fn type_source() -> PostSource {
        PostSource::KSTARTUP
    }

    fn init_once_guard() -> &'static Once {
        &KSTARTUP_COLLECTOR_INIT
    }

    fn from_context(context: CollectorContext) -> Self {
        KStartupCollector::from_context(context)
    }

    fn context(&self) -> &CollectorContext {
        &self.context
    }

    fn response_items<'a>(&self, body: &'a Value) -> Option<&'a [Value]> {
        let items = body.get("data")?.as_array()?;
        Some(items.as_slice())
    }

    fn map_item_to_announcement(&self, item: &Value) -> Option<Announcement> {
        KStartupCollector::map_item_to_announcement(item)
    }

    fn query_params(&self, search_term: Option<&str>) -> Vec<(&str, String)> {
        let mut q_params = vec![
            ("serviceKey", self.api_key().to_string()),
            ("returnType", "json".to_string()),
            ("cond[rcrt_prgs_yn::EQ]", "Y".to_string()),
        ];

        if let Some(st) = search_term {
            q_params.push(("cond[biz_pbanc_nm::LIKE]", st.to_string()));
        }
        q_params
    }
}

#[cfg(test)]
mod kstartup_tests {
    use crate::collectors::convert_to_date;

    use super::*;
    use mockito::{Matcher, Server};
    use std::env::set_var;

    #[test]
    fn test_collector_construction() {
        unsafe {
            // A dummy key for unittest.
            set_var("KSTARTUP_KEY", "dummy_kstartup_key");
        }

        let c = KStartupCollector::new(None, None);
        assert_eq!(KStartupCollector::type_identifier(), "kstartup");
        assert_eq!(
            "https://apis.data.go.kr/B552735/kisedKstartupService01/getAnnouncementInformation01",
            c.base_url()
        );
        assert_eq!(None, c.search_terms());

        // KStart up collector with the search term.
        // config injection
        let mut config = KStartupCollector::load_config().clone();
        config.search_terms = Some(vec!["search_term1".to_string(), "search_term2".to_string()]);

        let c = <KStartupCollector as Collector>::from_config(None, None, &config);
        let words = c.search_terms().expect("Search terms shouldn't be empty.");
        assert!(words.contains(&"search_term1".to_string()));
        assert!(words.contains(&"search_term2".to_string()));
    }

    #[test]
    fn test_default_url_builder() {
        unsafe {
            set_var("KSTARTUP_KEY", "dummy_kstartup_key");
        }

        let c = KStartupCollector::new(None, None);
        let urls = c.get_fetch_urls();

        // Default URL configuration doesn't have search terms.
        assert_eq!(None, c.search_terms());

        // Single URL.
        assert_eq!(1, urls.len());
        assert_eq!(
            r#"https://apis.data.go.kr/B552735/kisedKstartupService01/getAnnouncementInformation01?serviceKey=dummy_kstartup_key&returnType=json&cond[rcrt_prgs_yn::EQ]=Y"#,
            urls[0]
        );
    }

    #[test]
    fn test_search_url_builder() {
        unsafe {
            set_var("KSTARTUP_KEY", "dummy_kstartup_key");
        }

        // Config injection.
        let mut config = KStartupCollector::load_config().clone();
        config.search_terms = Some(vec!["search_term1".to_string(), "search_term2".to_string()]);
        let c = <KStartupCollector as Collector>::from_config(None, None, &config);
        let urls = c.get_fetch_urls();

        let words = c.search_terms().expect("Search terms shouldn't be empty.");
        for (i, u) in urls.iter().enumerate() {
            let mut search_url = r#"https://apis.data.go.kr/B552735/kisedKstartupService01/getAnnouncementInformation01?serviceKey=dummy_kstartup_key&returnType=json&cond[rcrt_prgs_yn::EQ]=Y&cond[biz_pbanc_nm::LIKE]="#.to_string();
            search_url.push_str(&words[i]);
            assert_eq!(search_url, *u);
        }
    }

    #[tokio::test]
    async fn test_fetch_no_data() {
        let rcv_data = r#"{
            "currentCount": 0,
            "data": [],
            "matchCount": 0,
            "page": 1,
            "perPage": 10,
            "totalCount": 27636
        }"#;

        unsafe {
            set_var("KSTARTUP_KEY", "dummy_kstartup_key");
        }

        let mut server = Server::new_async().await;
        let _mock = server
            .mock("GET", "/some_api")
            .match_query(Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(rcv_data)
            .create_async()
            .await;

        let collector = KStartupCollector::new(Some(format!("{}/some_api", server.url())), None);

        let announcements = collector.fetch_latest().await.unwrap();
        assert!(announcements.is_empty());
    }

    #[tokio::test]
    async fn test_fetched_data_processing() {
        unsafe {
            set_var("KSTARTUP_KEY", "dummy_kstartup_key");
        }

        let rcv_data = r#"{
            "currentCount": 1,
            "data": [ 
                {
                    "aply_excl_trgt_ctnt": null,
                    "aply_mthd_eml_rcpt_istc": null,
                    "aply_mthd_etc_istc": null,
                    "aply_mthd_fax_rcpt_istc": null,
                    "aply_mthd_onli_rcpt_istc": "https://bai.or.kr/bipatb/02_test/test10_view.asp?idx=1&page=1&params=",
                    "aply_mthd_pssr_rcpt_istc": null,
                    "aply_mthd_vst_rcpt_istc": null,
                    "aply_trgt": "대학,연구기관,일반기업,1인 창조기업",
                    "aply_trgt_ctnt": "부산 소재 AI솔루션 보유 기업, 스타트업, AI개발사(자), 대학원생 등",
                    "biz_aply_url": null,
                    "biz_enyy": "예비창업자,1년미만,2년미만,3년미만,5년미만,7년미만,10년미만",
                    "biz_gdnc_url": "bipa.kr/board/business/view?seq=84829",
                    "biz_pbanc_nm": "2026년 부산AI실증지원센터 고성능  컴퓨터 이용 지원사업 모집 공고",
                    "biz_prch_dprt_nm": "AI·AX단",
                    "biz_trgt_age": "만 20세 미만,만 20세 이상 ~ 만 39세 이하,만 40세 이상",
                    "detl_pg_url": "https://www.k-startup.go.kr/web/contents/bizpbanc-ongoing.do?schM=view&pbancSn=176563",
                    "id": 5,
                    "intg_pbanc_biz_nm": "2026년 부산AI실증지원센터 고성능  컴퓨터 이용 지원사업 모집 공고",
                    "intg_pbanc_yn": "N",
                    "pbanc_ctnt": "부산AI실증지원센터는 AI 기술 실증 인프라 지원을 위해 고성능 컴퓨터 확보가 어려운 부산 AI 기업·개발자 등을 대상으로 본 지원사업을 추진하오니 많은 관심과 참여 바랍니다.",
                    "pbanc_ntrp_nm": "(재)부산정보산업진흥원",
                    "pbanc_rcpt_bgng_dt": "20260306",
                    "pbanc_rcpt_end_dt": "20261231",
                    "pbanc_sn": 176563,
                    "prch_cnpl_no": "0517499304",
                    "prfn_matr": null,
                    "rcrt_prgs_yn": "Y",
                    "sprv_inst": "공공기관",
                    "supt_biz_clsfc": "시설ㆍ공간ㆍ보육",
                    "supt_regin": "부산"
                }],
            "matchCount": 1,
            "page": 1,
            "perPage": 10,
            "totalCount": 27636
        }"#;

        let mut server = Server::new_async().await;
        let _mock = server
            .mock("GET", "/some_api")
            .match_query(Matcher::Any)
            .with_body(rcv_data)
            .create_async()
            .await;

        let server_url = format!("{}/some_api", server.url());
        let collector = KStartupCollector::new(Some(server_url), None);

        let announcements = collector.fetch_latest().await.unwrap();
        assert!(!announcements.is_empty());
        assert_eq!(1, announcements.len());

        let a = &announcements[0];
        // announcement's id should be set with a unique sequence value.
        assert!(a.id.is_some());

        // posting id.
        assert!(a.id.is_some());
        assert_eq!("5", a.post_id.clone().unwrap());

        // Posting source.
        assert_eq!(PostSource::KSTARTUP, a.post_source);

        assert_eq!(
            "2026년 부산AI실증지원센터 고성능  컴퓨터 이용 지원사업 모집 공고",
            a.title
        );

        assert_eq!(
            "부산AI실증지원센터는 AI 기술 실증 인프라 지원을 위해 고성능 컴퓨터 확보가 어려운 부산 AI 기업·개발자 등을 대상으로 본 지원사업을 추진하오니 많은 관심과 참여 바랍니다.",
            a.content
        );

        assert_eq!(
            r#"https://www.k-startup.go.kr/web/contents/bizpbanc-ongoing.do?schM=view&pbancSn=176563"#,
            a.post_url.clone().expect("Post URL should be available")
        );

        // Date
        let begin_date = convert_to_date("20260306");
        let end_date = convert_to_date("20261231");

        assert_eq!(begin_date, a.begin_date);
        assert_eq!(end_date, a.end_date);
    }
}
