use crate::collectors::{Collector, CollectorContext, PostSource};
use crate::noti::Announcement;

use async_trait::async_trait;
use chrono::{Duration, Local};
use serde_json::Value;
use std::sync::Once;

static SMTECH_COLLECTOR_INIT: Once = Once::new();

pub struct SmTechCollector {
    context: CollectorContext,
}

impl SmTechCollector {
    pub fn from_context(context: CollectorContext) -> Self {
        Self { context }
    }
}

#[async_trait]
impl Collector for SmTechCollector {
    fn api_key_env_name() -> &'static str {
        "SMTECH_KEY"
    }

    fn type_source() -> PostSource {
        PostSource::SMTECH
    }

    fn init_once_guard() -> &'static Once {
        &SMTECH_COLLECTOR_INIT
    }

    fn from_context(context: CollectorContext) -> Self {
        SmTechCollector::from_context(context)
    }

    fn context(&self) -> &CollectorContext {
        &self.context
    }

    fn response_items<'a>(&self, body: &'a Value) -> Option<&'a [Value]> {
        let arr = body.get("data")?.as_array()?;
        Some(arr.as_slice())
    }

    fn map_item_to_announcement(&self, item: &Value) -> Option<Announcement> {
        let title = item.get("pblancNm")?.as_str()?.to_string();

        let post_id = Some(match item.get("pblancSeq")? {
            Value::String(value) => value.clone(),
            Value::Number(value) => value.to_string(),
            _ => return None,
        });

        // Need to clone the object.
        let desc_url = item.get("pblancDtlUrl")?.as_str()?.to_string();
        let post_url = Some(desc_url.clone());

        // Content
        let content = desc_url;

        // Date
        let begin_date_str = item.get("pblancBgnDt")?.as_str()?;
        let begin_date = super::try_convert_to_date(begin_date_str)?;

        let end_date_str = item.get("pblancEndDt")?.as_str()?;
        let end_date = super::try_convert_to_date(end_date_str)?;

        Some(Announcement {
            id: None,
            post_id,
            post_source: PostSource::SMTECH,
            post_url,
            email: None,
            title,
            content,
            begin_date,
            end_date,
        })
    }

    fn query_params(&self, _search_term: Option<&str>) -> Vec<(&str, String)> {
        // Search from a week before from now.
        let now = Local::now();
        let yesterday = now - Duration::days(1);
        let end_str = now.format("%Y%m%d").to_string();
        let begin_str = yesterday.format("%Y%m%d").to_string();

        vec![
            ("token", self.api_key().to_string()),
            ("strDt", begin_str),
            ("endDt", end_str),
            ("html", "no".to_string()),
        ]
    }
}

#[cfg(test)]
mod smtech_test {
    use mockito::{Matcher, Server};

    use super::*;
    use crate::collectors;
    use crate::collectors::Collector;
    use std::env::set_var;

    #[test]
    fn test_collector_construction() {
        unsafe {
            set_var("SMTECH_KEY", "dummy_smtech_key");
        }

        let c = SmTechCollector::new(None, None);
        assert_eq!(SmTechCollector::type_identifier(), "smtech");
        assert_eq!(
            "https://www.smes.go.kr/fnct/apiReqst/extPblancInfo",
            c.base_url()
        );
    }

    #[test]
    fn test_url_build() {
        unsafe {
            set_var("SMTECH_KEY", "dummy_smtech_key");
        }

        let c = SmTechCollector::new(None, None);
        let urls = c.get_fetch_urls();

        assert_eq!(1, urls.len());
        let url = urls[0].clone();
        assert!(url.contains("token=dummy_smtech_key"));
        assert!(url.contains("strDt="));
        assert!(url.contains("endDt="));
        assert!(url.contains("html=no"));
    }

    #[tokio::test]
    async fn test_fetch_no_data() {
        unsafe {
            set_var("SMTECH_KEY", "dummy_smtech_key");
        }

        // Empty data received
        let rcv_data = r#"{
            "resultCd": "0",
            "data": [],
            "resultMsg": "정상적으로 조회되었습니다."
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

        let collector = SmTechCollector::new(Some(format!("{}/api", server.url())), None);
        let announcements = collector.fetch_latest().await.unwrap();
        assert!(announcements.is_empty());
    }

    #[tokio::test]
    async fn test_fetched_data_processing() {
        unsafe {
            set_var("SMTECH_KEY", "dummy_smtech_key");
        }

        let rcv_data = r#"{
            "resultCd": "0",
            "data": [
                {
                    "pblancSeq": 266104652,
                    "creatDt": "2026-04-14 10:30:45",
                    "pblancDtlUrl": "https://www.smes.go.kr/sanhakin/websquare/wq_smesDtlBsns.do?NTTSN=2710&BBSCLCODESE=00000018&BSNSCLCODESE=0000003L",
                    "pblancNm": "(경기) '중앙하이츠 원종역' 중소기업장기근속자 주택특별공급 안내",
                    "detailBsnsNm": "(경기) '중앙하이츠 원종역' 중소기업장기근속자 주택특별공급 안내",
                    "policyCnts": "□주택명:중앙하이츠원종역□공급위치:경기도부천시오정구원종동276-2□공급일정:[입주자모집공고]’26.5.8.(금)→[청약홈청약신청]‘26.5.18.(월)*분양일정및공급내역은인·허가및입주자모집공고에따라변경될수있음□배정세대수:2세대(예비10세대)",
                    "sportMg": "",
                    "sportCnts": "",
                    "sportTrget": "(1)입주자모집공고일기준,「중소기업기본법」제2조에따른중소기업에재직중인근로자*제외업종:①일반유흥주점업②무도유흥주점업③기타주점업④기타사행시설 관리및운영업⑤무도장운영업(2)과거근무경력을포함하여「중소기업기본법」제2조제1항에따른 중소기업에서재직한기간이5년(또는동일중소기업근무3년)이상인자(3)「근로기준법」제2조제1항제1호에따라임금을목적으로근로를 제공하는자(고용보험가입자)(대표자및법인임원*제외)*다만,①소기업(「중소기업기본법」시행령제8조)재직②고용보험가입③실질적 근로제공(급여수령)시이사및감사는신청가능(4)입주자모집공고일기준무주택세대구성원인자*본인과세대원전원이무주택인자여야하며,신청인의배우자가주민등록상세대 분리된경우에도배우자및그세대원을포함(5)수도권(서울·경기·인천)거주자*이며,지역․면적별예치금기준등 청약통장(종합저축,청약저축)가입요건을충족한자(시행사측안내문참고)*(예외)건설예정지역이’평택시‘인주택은신청자거주지제한없음",
                    "reqstRcept": "□신청기간:2026.4.14.(화)~2026.4.17.(금)18:00□신청방법:온라인신청",
                    "refrnc": "",
                    "updDt": "2026-04-15 09:53:10",
                    "pblancBgnDt": "2026-04-14",
                    "pblancEndDt": "2026-04-17",
                    "pblancAttach": "https://www.smes.go.kr/sanhakin/common/fileDownload.do?DOWNLOAD_KEY_IF=CCFFDDF8-98D7-4CB1-A0B1-C36D1440D812|https://www.smes.go.kr/sanhakin/common/fileDownload.do?DOWNLOAD_KEY_IF=7A60F416-6B66-44E2-97B7-72E8F421444B|https://www.smes.go.kr/sanhakin/common/fileDownload.do?DOWNLOAD_KEY_IF=8C3360C5-FEB5-4071-845D-6657E3229F2D|https://www.smes.go.kr/sanhakin/common/fileDownload.do?DOWNLOAD_KEY_IF=75176A9D-2216-49E1-8377-6118BBA68D7F|https://www.smes.go.kr/sanhakin/common/fileDownload.do?DOWNLOAD_KEY_IF=697BA123-8F4A-4680-8D36-7B5F2DC69FBC|https://www.smes.go.kr/sanhakin/common/fileDownload.do?DOWNLOAD_KEY_IF=4E325FF5-27BF-4121-82D9-0303E0E0C1BC",
                    "pblancAttachNm": "[경기청] 중소기업장기근속자 주택특별공급 안내문_중앙하이츠 원종역.pdf|[시행사] 중앙하이츠 원종역 기관추천 특별공급 안내문.pdf|[참고1] 뿌리산업의 범위.pdf|[참고2] 국가기술자격종목.pdf|[참고3] 중소기업 장기근속자 주택특별공급에 관한 지침 개정 안내문('23.1.1.).pdf|[참고4] 배점기준표('23년 개정).pdf",
                    "reqstLinkInfo": "https://www.smes.go.kr/sanhakin/websquare/wq_smesDtlBsns.do?NTTSN=2710&BBSCLCODESE=00000018&BSNSCLCODESE=0000003L&RQST=Y",
                    "pblancPdCnts": "2026-04-14 ~ 2026-04-17",
                    "sportInsttNm": "중소벤처기업부",
                    "sportInsttCd": "SP16",
                    "bizType": "인력",
                    "bizTypeCd": "PC30",
                    "sportType": "인력지원",
                    "sportTypeCd": "RT07",
                    "lifeCyclDvsn": "",
                    "lifeCyclDvsnCd": "",
                    "areaNm": "경기도",
                    "areaCd": "4100000000",
                    "salsAmt": "5억미만|5억이상~10억미만|10억이상~20억미만|20억이상~50억미만|50억이상~100억미만|100억이상~ 300억미만|300억이상",
                    "salsAmtCd": "SI01|SI02|SI03|SI04|SI05|SI06|SI07",
                    "minSalsAmt": "",
                    "maxSalsAmt": "",
                    "ablbiz": "3년미만|3년이상~5년미만|5년이상~7년미만|7년이상~10년미만|10년이상~20년미만|20년이상",
                    "ablbizCd": "OI01|OI02|OI03|OI04|OI05|OI06",
                    "minAblbiz": "",
                    "maxAblbiz": "",
                    "emplyCnt": "1~5명미만|5~10명미만|10~20명미만|20~50명미만|50~100명미만|100명이상",
                    "emplyCntCd": "EI01|EI02|EI03|EI04|EI05|EI06",
                    "minEmplyCnt": "",
                    "maxEmplyCnt": "",
                    "cmpScale": "중소기업",
                    "cmpScaleCd": "CC10",
                    "needCrtfn": "중소기업",
                    "needCrtfnCd": "EC04",
                    "cntcInsttCd": "BI09",
                    "cntcInsttNm": "중소기업인력지원사업종합관리시스템",
                    "induty": "",
                    "rpsntAge": "",
                    "minRpsntAge": "",
                    "maxRpsntAge": "",
                    "minInrst": "",
                    "maxInrst": "",
                    "minSportAmt": "",
                    "maxSportAmt": "",
                    "refrncUrl": "",
                    "refrncDept": "",
                    "refrncTel": "",
                    "refntnYn": "",
                    "fntnYn": "",
                    "fmleRpsntYn": "",
                    "pblancFileUrl": "https://www.smes.go.kr/comm/getFile?srvcId=CNTCWRTANC&upperNo=266104652&fileTy=CNTCATTACH&fileNo=1",
                    "pblancFileNm": "[경기청] 중소기업장기근속자 주택특별공급 안내문_중앙하이츠 원종역.pdf"
                }
            ],
            "resultMsg": "정상적으로 조회되었습니다."
        }
        "#;

        let mut server = Server::new_async().await;
        server
            .mock("GET", "/api")
            .match_query(Matcher::Any)
            .with_body(rcv_data)
            .create_async()
            .await;
        let collector = SmTechCollector::new(Some(format!("{}/api", server.url())), None);
        let announcements = collector.fetch_latest().await.unwrap();

        // Something has to be parsed.
        assert!(!announcements.is_empty());

        let a = &announcements[0];
        assert!(a.id.is_some());
        assert_eq!(
            "(경기) '중앙하이츠 원종역' 중소기업장기근속자 주택특별공급 안내",
            a.title
        );

        assert_eq!(
            Some(
                "https://www.smes.go.kr/sanhakin/websquare/wq_smesDtlBsns.do?NTTSN=2710&BBSCLCODESE=00000018&BSNSCLCODESE=0000003L".to_string()
            ),
            a.post_url
        );
        assert_eq!(None, a.email);

        // Contents contains URL
        assert_eq!("https://www.smes.go.kr/sanhakin/websquare/wq_smesDtlBsns.do?NTTSN=2710&BBSCLCODESE=00000018&BSNSCLCODESE=0000003L".to_string(),
        a.content);

        // Date
        let exp_begin_date = collectors::convert_to_date("20260414");
        let exp_end_date = collectors::convert_to_date("20260417");
        assert_eq!(exp_begin_date, a.begin_date);
        assert_eq!(exp_end_date, a.end_date);
    }
}
