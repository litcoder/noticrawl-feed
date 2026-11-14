use crate::collectors::{Collector, CollectorContext, PostSource};
use crate::noti::Announcement;

use async_trait::async_trait;
use chrono::{Datelike, Local};
use serde_json::Value;
use std::sync::Once;

static ENARA_COLLECTOR_INIT: Once = Once::new();

pub struct EnaraCollector {
    context: CollectorContext,
}

impl EnaraCollector {
    pub fn from_context(context: CollectorContext) -> Self {
        Self { context }
    }
}

#[async_trait]
impl Collector for EnaraCollector {
    fn api_key_env_name() -> &'static str {
        "ENARA_KEY"
    }

    fn type_source() -> PostSource {
        PostSource::ENARA
    }

    fn init_once_guard() -> &'static Once {
        &ENARA_COLLECTOR_INIT
    }

    fn from_context(context: CollectorContext) -> Self {
        EnaraCollector::from_context(context)
    }

    fn context(&self) -> &CollectorContext {
        &self.context
    }

    fn response_items<'a>(&self, body: &'a Value) -> Option<&'a [Value]> {
        let item = body
            .get("response")?
            .get("body")?
            .get("items")?
            .get("item")?;
        let arr = item.as_array()?;
        Some(arr.as_slice())
    }

    fn map_item_to_announcement(&self, item: &Value) -> Option<Announcement> {
        let title = item.get("PBLANC_NM")?.as_str()?.to_string();

        // Content
        let mut content = item.get("BSNS_SMRY")?.as_str()?.to_string();
        for kw in vec![
            "BSNS_PURPS_CN",
            "EXPC_EFFECT_CN",
            "SPORT_CN",
            "SLCTN_STDR_DC",
            "SPORT_TRGET_CN",
        ] {
            let c = item.get(kw)?.as_str().unwrap();
            content.push_str(c);
        }
        let budget = item.get("SPORT_BGAMT")?.as_str().unwrap();
        content.push_str(&format!(" 혜택금액: {}원", budget));

        // Email address to replace the post_url.
        let email = item
            .get("CHARGER_EMAIL")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // dates
        let begin_date_str = item.get("RCEPT_BEGIN_DE")?.as_str()?;
        let begin_date = super::try_convert_to_date(begin_date_str)?;

        let end_date_str = item.get("RCEPT_END_DE")?.as_str()?;
        let end_date = super::try_convert_to_date(end_date_str)?;

        Some(Announcement {
            id: None,
            post_id: None,
            post_source: PostSource::ENARA,
            post_url: None,
            email,
            title,
            content,
            begin_date,
            end_date,
        })
    }

    fn query_params(&self, search_term: Option<&str>) -> Vec<(&str, String)> {
        let config = Self::load_config();
        let search_cnt = config.num_announcements.unwrap_or(10);

        // Current year
        let now = Local::now();
        let current_year = now.year();

        // Following four parameters are required.
        let mut q_params = vec![
            ("serviceKey", self.api_key().to_string()),
            ("resultType", "json".to_string()),
            ("numOfRows", search_cnt.to_string()),
            ("pageNo", "1".to_string()),
            ("bsnsyear", current_year.to_string()),
        ];

        if let Some(st) = search_term {
            q_params.push(("pblanc_nm", st.to_string()));
        }
        q_params
    }
}

#[cfg(test)]
mod enara_test {
    use crate::collectors;

    use super::*;
    use mockito::{Matcher, Server};
    use std::env::set_var;

    #[test]
    fn test_collector_construction() {
        let c = EnaraCollector::new(None, None);
        assert_eq!(EnaraCollector::type_identifier(), "enara");
        assert_eq!(
            "https://apis.data.go.kr/1051000/MoefOpenAPI/T_OPD_PBNS",
            c.base_url()
        );
    }

    #[test]
    fn test_url_build() {
        unsafe {
            set_var("ENARA_KEY", "dummy_enara_key");
        }

        let c = EnaraCollector::new(None, None);
        let urls = c.get_fetch_urls();

        assert_eq!(1, urls.len());

        // The url should contains necessary query parameters.
        let url = urls[0].clone();
        assert!(url.contains("serviceKey=dummy_enara_key"));
        assert!(url.contains("numOfRows="));
        assert!(url.contains("resultType=json"));
        assert!(url.contains("bsnsyear"));
        assert!(url.contains("pageNo="));
    }

    #[tokio::test]
    async fn test_fetch_no_data() {
        // Empty data received.
        let rcv_data = r#"{
            "response": {
                "header": {
                    "resultCode": "00",
                    "resultMsg": "NORMAL SERVICE"
                },
                "body": {
                    "pageNo": 1,
                    "totalCount": 0,
                    "items": {
                        "item": []
                    },
                    "numOfRows": 10
                }
            }
        }"#;

        let mut server = Server::new_async().await;
        server
            .mock("GET", "/api")
            .match_query(Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(rcv_data)
            .create_async()
            .await;

        let collector = EnaraCollector::new(Some(format!("{}/api", server.url())), None);
        let announcements = collector.fetch_latest().await.unwrap();
        assert!(announcements.is_empty());
    }

    #[tokio::test]
    async fn test_fetched_data_processing() {
        unsafe {
            set_var("ENARA_KEY", "dummy_enara_key");
        }

        let rcv_data = r#"{
            "response": {
                "header": {
                    "resultCode": "00",
                    "resultMsg": "NORMAL SERVICE"
                },
                "body": {
                    "pageNo": 1,
                    "totalCount": 1,
                    "items": {
                        "item": [
                            {
                                "REALM_CODE": "110",
                                "PSSRP_INSTT_NM": "중소벤처기업부",
                                "RCEPT_END_DE": "20260121",
                                "PBLANC_BEGIN_DE": "20260106",
                                "RCEPT_BEGIN_DE": "20260106",
                                "SLCTN_RESULT_DSPTH_MTH_CN": "중기부 홈페이지 공지",
                                "QUALF_VRIFY_REQUST_TRGET_AT": "Y",
                                "RCEPT_END_TM": "1800",
                                "DDTLBZ_NM": "몽골 울란바타르 스타트업 타운조성 및 액셀러레이팅",
                                "CHARGER_EMAIL": "varuna23@korea.kr",
                                "PRTN_PROCSS_CN": "공고 및 신청접수(1.6~1.21) - 서류 및 발표평가(1.22~2.13) - 선정결과 공고(2월 셋째주 중)",
                                "DSPTH_DE": "20260213",
                                "BSNSYEAR": "2026",
                                "PSSRP_INSTT_SE": "중앙부처",
                                "SPORT_CN": "ㅇ 스타트업육성 중장기 로드맵 수립\r\nㅇ 스타트업 타운 조성 및 운영\r\nㅇ 창업지원 제도 개선 지원\r\nㅇ 스타트업 역량강화 및 엑셀러레이팅 프로그램 운영\r\nㅇ 창업생태계 구축\r\nㅇ 사업관리 및 사후관리 ",
                                "RCEPT_PD_DC": "’26. 1. 6.(화)~‘26. 1. 21.(수) 18:00까지",
                                "SPORT_BGAMT": "707000000",
                                "PBLANC_NM": "2026년 국제개발협력(ODA) 사업  수행기관 공모(몽골 울란바타르 스타트업 타운조성 및 액셀러레이팅)",
                                "SLCTN_STDR_DC": "(선정요건) \r\nㅇ 수원국과 협력사업 운영경험 및 현지 네트워크 보유 등 사업추진 역량 보유\r\nㅇ 사업수행기관 내 국제개발협력사업 운영을 위한 전담조직 구성 및 운영\r\n\r\n(선정 방법) \r\n서류 및 발표 평가를 토대로 선정",
                                "REALM_NM": "산업·중소기업및에너지",
                                "PBLANC_END_DE": "20260121",
                                "SLCTN_DE": "20260213",
                                "BSNS_DCMTG_CN": "해당없음",
                                "BSNS_PURPS_CN": "  ㅇ 몽골 울란바타르 지역에 창업 인프라와 액셀러레이팅 체계를 구축하여 청년‧기술창업 중심의 지속가능한 창업생태계 조성",
                                "REQST_RCEPT_MTH_CN": "e나라도움을 통한 접수",
                                "BSNS_BEGIN_DE": "20260101",
                                "PRESENTN_PAPERS_GUIDANCE_CN": "ㅇ 사업수행기관 신청서(별지 제1호 서식) 1부\r\nㅇ 사업계획서(별지 제2호 서식) 1부\r\nㅇ 사업자등록증, 법인 등기부등본 각 1부",
                                "STDR_DE": "20260408",
                                "SPORT_TRGET_CN": "국제개발협력(ODA) 사업을 수행할 수 있는 역량 및 인프라를 보유한 기관(단체) ",
                                "BSNS_END_DE": "20261231",
                                "EXCL_TRGET_CN": "｢국고보조금 통합관리지침｣ 제13조 제4항 등 관련 법령에 따른 결격사유 및 다음 각 호의 어느 하나에 해당되는 경우 신청 불가",
                                "BSNS_SMRY": "2026년 국제개발협력(ODA)-몽골 울란바타르 스타트업 타운조성 및 액셀러레이팅",
                                "CHARGER_TELNO": "044-204-7557",
                                "CHARGER_NM": "양정윤",
                                "EXPC_EFFECT_CN": "중소벤처기업부 정책목표에 부합하는 ODA 사업을 추진하여 한국과 수원국 정부 간 경제협력 및 우리 중소기업의 해외진출과 수출 지원"
                            }
                        ]
                    },
                    "numOfRows": 10
                }
            }
        }
        "#;

        let mut server = Server::new_async().await;
        server
            .mock("GET", "/api")
            .match_query(Matcher::Any)
            .with_body(rcv_data)
            .create_async()
            .await;
        let collector = EnaraCollector::new(Some(format!("{}/api", server.url())), None);
        let announcements = collector.fetch_latest().await.unwrap();

        // Something has been parsed.
        assert!(!announcements.is_empty());
        assert_eq!(1, announcements.len());

        let a = &announcements[0];
        assert!(a.id.is_some());
        assert_eq!(
            "2026년 국제개발협력(ODA) 사업  수행기관 공모(몽골 울란바타르 스타트업 타운조성 및 액셀러레이팅)",
            a.title
        );

        let mut exp_content = String::from(
            "2026년 국제개발협력(ODA)-몽골 울란바타르 스타트업 타운조성 및 액셀러레이팅",
        );
        exp_content.push_str("  ㅇ 몽골 울란바타르 지역에 창업 인프라와 액셀러레이팅 체계를 구축하여 청년‧기술창업 중심의 지속가능한 창업생태계 조성");
        exp_content.push_str("중소벤처기업부 정책목표에 부합하는 ODA 사업을 추진하여 한국과 수원국 정부 간 경제협력 및 우리 중소기업의 해외진출과 수출 지원");
        exp_content.push_str("ㅇ 스타트업육성 중장기 로드맵 수립\r\nㅇ 스타트업 타운 조성 및 운영\r\nㅇ 창업지원 제도 개선 지원\r\nㅇ 스타트업 역량강화 및 엑셀러레이팅 프로그램 운영\r\nㅇ 창업생태계 구축\r\nㅇ 사업관리 및 사후관리 ");
        exp_content.push_str("(선정요건) \r\nㅇ 수원국과 협력사업 운영경험 및 현지 네트워크 보유 등 사업추진 역량 보유\r\nㅇ 사업수행기관 내 국제개발협력사업 운영을 위한 전담조직 구성 및 운영\r\n\r\n(선정 방법) \r\n서류 및 발표 평가를 토대로 선정");
        exp_content.push_str(
            "국제개발협력(ODA) 사업을 수행할 수 있는 역량 및 인프라를 보유한 기관(단체) ",
        );
        exp_content.push_str(" 혜택금액: 707000000원");
        assert_eq!(exp_content, a.content);

        // e나라도움 doesn't provide a posting URL, but does provide contact email.
        assert_eq!(None, a.post_url);
        assert_eq!(Some("varuna23@korea.kr".to_string()), a.email);

        // Date
        let exp_begin_date = collectors::convert_to_date("20260106");
        let exp_end_date = collectors::convert_to_date("20260121");
        assert_eq!(exp_begin_date, a.begin_date);
        assert_eq!(exp_end_date, a.end_date);
    }
}
