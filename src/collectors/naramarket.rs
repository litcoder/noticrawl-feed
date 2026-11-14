use crate::collectors::{Collector, CollectorContext, PostSource};
use crate::noti::Announcement;

use async_trait::async_trait;
use chrono::{Local, Months};
use serde_json::Value;
use std::sync::Once;

static NARAMARKET_COLLECTOR_INIT: Once = Once::new();

pub struct NaraMarketCollector {
    context: CollectorContext,
}

impl NaraMarketCollector {
    pub fn from_context(context: CollectorContext) -> Self {
        Self { context }
    }
}

#[async_trait]
impl Collector for NaraMarketCollector {
    fn api_key_env_name() -> &'static str {
        "NARAMARKET_KEY"
    }

    fn type_source() -> PostSource {
        PostSource::NARAMARKET
    }

    fn init_once_guard() -> &'static Once {
        &NARAMARKET_COLLECTOR_INIT
    }

    fn from_context(context: CollectorContext) -> Self {
        NaraMarketCollector::from_context(context)
    }

    fn context(&self) -> &CollectorContext {
        &self.context
    }

    fn response_items<'a>(&self, body: &'a Value) -> Option<&'a [Value]> {
        let item = body.get("response")?.get("body")?.get("items")?;
        let arr = item.as_array()?;
        Some(arr.as_slice())
    }

    fn map_item_to_announcement(&self, item: &Value) -> Option<Announcement> {
        let title = item.get("bidNtceNm")?.as_str()?.to_string();
        let post_id = Some(item.get("bidNtceNo")?.as_str()?.to_string());
        let post_url = Some(item.get("bidNtceUrl")?.as_str()?.to_string());
        let email = Some(item.get("dmndInsttOfclEmailAdrs")?.as_str()?.to_string());

        // Content.
        // Gemini can read content from the given URL.
        let budget = item.get("asignBdgtAmt")?.as_str().unwrap();
        let mut content = post_url.clone().unwrap();
        content.push_str(&format!("\n혜택금액: {}원", budget));

        let begin_date_str = item.get("bidBeginDate")?.as_str()?;
        let begin_date = super::try_convert_to_date(begin_date_str)?;
        let end_date_str = item.get("bidClseDate")?.as_str()?;
        let end_date = super::try_convert_to_date(end_date_str)?;

        Some(Announcement {
            id: None,
            post_id,
            post_source: PostSource::NARAMARKET,
            post_url,
            email,
            title,
            content,
            begin_date,
            end_date,
        })
    }

    fn query_params(&self, _search_term: Option<&str>) -> Vec<(&str, String)> {
        let config = Self::load_config();
        let search_cnt = config.num_announcements.unwrap_or(10);

        // Search for 1 month from now.
        let now = Local::now();
        let one_moneth_ago = now
            .checked_sub_months(Months::new(1))
            .expect("Failed to calculate sub one month");

        let end_str = now.format("%Y%m%d%H%M").to_string();
        let begin_str = one_moneth_ago.format("%Y%m%d%H%M").to_string();

        let q_params = vec![
            ("serviceKey", self.api_key().to_string()),
            ("type", "json".to_string()),
            ("numOfRows", search_cnt.to_string()),
            ("pageNo", "1".to_string()),
            ("bidNtceBgnDt", begin_str),
            ("bidNtceEndDt", end_str),
        ];

        q_params
    }
}

#[cfg(test)]
mod naramarket_test {
    use crate::collectors::convert_to_date;

    use super::*;
    use mockito::{Matcher, Server};
    use std::env::set_var;

    #[test]
    fn test_collector_construction() {
        unsafe {
            set_var("NARAMARKET_KEY", "dummy_naramarket_key");
        }

        let c = NaraMarketCollector::new(None, None);
        assert_eq!(NaraMarketCollector::type_identifier(), "naramarket");
        assert_eq!(
            "https://apis.data.go.kr/1230000/ao/PubDataOpnStdService/getDataSetOpnStdBidPblancInfo",
            c.base_url()
        );
    }

    #[test]
    fn test_url_build() {
        unsafe {
            set_var("NARAMARKET_KEY", "dummy_naramarket_key");
        }

        let c = NaraMarketCollector::new(None, None);
        let urls = c.get_fetch_urls();
        assert_eq!(1, urls.len());

        let url = &urls[0];
        assert!(url.contains("serviceKey=dummy_naramarket_key"));
        assert!(url.contains("numOfRows="));
        assert!(url.contains("type=json"));
        assert!(url.contains("pageNo=1"));
        assert!(url.contains("bidNtceBgnDt="));
        assert!(url.contains("bidNtceEndDt="));
    }

    #[tokio::test]
    async fn test_fetch_no_data() {
        unsafe {
            set_var("NARAMARKET_KEY", "dummy_naramarket_key");
        }

        // Empty data.
        let rcv_data = r#"{
            "response": {
                "header": {
                    "resultCode": "00",
                    "resultMsg": "정상"
                },
                "body": {
                    "items": [],
                    "numOfRows": 100,
                    "pageNo": 1,
                    "totalCount": 0
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

        let collector = NaraMarketCollector::new(Some(format!("{}/api", server.url())), None);
        let announcements = collector
            .fetch_latest()
            .await
            .expect("Information should be fetched");
        assert!(announcements.is_empty());
    }

    #[tokio::test]
    async fn test_fetched_data_processing() {
        unsafe {
            set_var("NARAMARKET_KEY", "dummy_naramarket_key");
        }

        let rcv_data = r#"{
            "response": {
                "header": {
                    "resultCode": "00",
                    "resultMsg": "정상"
                },
                "body": {
                    "items": [
                        {
                            "bidNtceNo": "R26BK01459315",
                            "bidNtceOrd": "000",
                            "refNtceNo": "R26BK01459315",
                            "refNtceOrd": "000",
                            "ppsNtceYn": "Y",
                            "bidNtceNm": "2026년도 부산지방우정청 집배 우의 제조·구매(긴급)",
                            "bidNtceSttusNm": "일반공고",
                            "bidNtceDate": "2026-04-13",
                            "bidNtceBgn": "15:45",
                            "bsnsDivNm": "물품",
                            "intrntnlBidYn": "N",
                            "cmmnCntrctYn": "Y",
                            "cmmnReciptMethdNm": "",
                            "elctrnBidYn": "Y",
                            "cntrctCnclsSttusNm": "총액계약",
                            "cntrctCnclsMthdNm": "제한경쟁",
                            "bidwinrDcsnMthdNm": "규격가격동시입찰",
                            "ntceInsttNm": "조달청",
                            "ntceInsttCd": "1230000",
                            "ntceInsttOfclDeptNm": "",
                            "ntceInsttOfclNm": "김현욱",
                            "ntceInsttOfclTel": "042-724-7256",
                            "ntceInsttOfclEmailAdrs": "woogi321@korea.kr",
                            "dmndInsttNm": "과학기술정보통신부 우정사업본부 부산지방우정청",
                            "dmndInsttCd": "1721818",
                            "dmndInsttOfclDeptNm": "",
                            "dmndInsttOfclNm": "이나현",
                            "dmndInsttOfclTel": "0515593256",
                            "dmndInsttOfclEmailAdrs": "julia7800@korea.kr",
                            "presnatnOprtnYn": "",
                            "presnatnOprtnDate": "",
                            "presnatnOprtnTm": "",
                            "presnatnOprtnPlce": "",
                            "bidPrtcptQlfctRgstClseDate": "2026-04-29",
                            "bidPrtcptQlfctRgstClseTm": "18:00",
                            "cmmnReciptAgrmntClseDate": "",
                            "cmmnReciptAgrmntClseTm": "",
                            "bidBeginDate": "2026-04-28",
                            "bidBeginTm": "10:00",
                            "bidClseDate": "2026-04-30",
                            "bidClseTm": "10:00",
                            "opengDate": "2026-04-30",
                            "opengTm": "11:00",
                            "opengPlce": "",
                            "asignBdgtAmt": "156078370",
                            "presmptPrce": "140727273",
                            "rsrvtnPrceDcsnMthdNm": "복수예가",
                            "rgnLmtYn": "N",
                            "prtcptPsblRgnNm": "",
                            "indstrytyLmtYn": "N",
                            "bidprcPsblIndstrytyNm": "",
                            "bidNtceUrl": "https://www.g2b.go.kr/link/PNPE027_01/single/?bidPbancNo=R26BK01459315&bidPbancOrd=000",
                            "dataBssDate": "2026-04-15"
                        }
                    ],
                    "numOfRows": 100,
                    "pageNo": 1,
                    "totalCount": 55892
                }
            }
        }
        "#;

        let mut server = Server::new_async().await;
        server
            .mock("GET", "/api")
            .match_query(Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(rcv_data)
            .create_async()
            .await;

        let collector = NaraMarketCollector::new(Some(format!("{}/api", server.url())), None);
        let announcements = collector.fetch_latest().await.unwrap();
        // Shouldn't be empty.
        assert_eq!(1, announcements.len());

        let a = &announcements[0];
        assert!(a.id.is_some());
        assert_eq!("2026년도 부산지방우정청 집배 우의 제조·구매(긴급)", a.title);

        // Content (URL + budget)
        assert_eq!(
            "https://www.g2b.go.kr/link/PNPE027_01/single/?bidPbancNo=R26BK01459315&bidPbancOrd=000\n혜택금액: 156078370원",
            a.content
        );

        assert_eq!(
            "https://www.g2b.go.kr/link/PNPE027_01/single/?bidPbancNo=R26BK01459315&bidPbancOrd=000",
            a.post_url.clone().unwrap()
        );
        assert_eq!("julia7800@korea.kr", a.email.clone().unwrap());

        // Datetime
        let exp_begin_dt = convert_to_date("2026-04-28");
        let exp_end_dt = convert_to_date("2026-04-30");

        assert_eq!(exp_begin_dt, a.begin_date);
        assert_eq!(exp_end_dt, a.end_date);
    }
}
