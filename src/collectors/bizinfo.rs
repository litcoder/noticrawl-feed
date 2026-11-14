use crate::collectors::{Collector, CollectorContext, PostSource};
use crate::noti::Announcement;

use async_trait::async_trait;
use chrono::NaiveDate;
use serde_json::Value;
use std::sync::Once;

static BIZINFO_COLLECTOR_INIT: Once = Once::new();

pub struct BizInfoCollector {
    context: CollectorContext,
}

impl BizInfoCollector {
    pub fn from_context(context: CollectorContext) -> Self {
        Self { context }
    }

    fn parse_begin_end_date(period: &str) -> Option<(NaiveDate, NaiveDate)> {
        let (begin_raw, end_raw) = period.split_once('~')?;
        let begin_date = super::try_convert_to_date(begin_raw.trim())?;
        let end_date = super::try_convert_to_date(end_raw.trim())?;
        Some((begin_date, end_date))
    }

    /// Maps JSON responded data to the Announcement struct.
    fn map_item_to_announcement(item: &Value) -> Option<Announcement> {
        let title = item.get("pblancNm")?.as_str()?.to_string();
        let content = item.get("bsnsSumryCn")?.as_str()?.to_string();
        let begin_end_date = item.get("reqstBeginEndDe")?.as_str()?;
        let (begin_date, end_date) = Self::parse_begin_end_date(begin_end_date)?;
        let post_url = item
            .get("pblancUrl")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        Some(Announcement {
            id: None,
            post_id: None,
            post_source: PostSource::BIZINFO,
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
impl Collector for BizInfoCollector {
    fn api_key_env_name() -> &'static str {
        "BIZINFO_KEY"
    }

    fn type_source() -> PostSource {
        PostSource::BIZINFO
    }

    fn init_once_guard() -> &'static Once {
        &BIZINFO_COLLECTOR_INIT
    }

    fn from_context(context: CollectorContext) -> Self {
        BizInfoCollector::from_context(context)
    }

    fn context(&self) -> &CollectorContext {
        &self.context
    }

    fn context_search_terms(_config: &crate::config::CollectorConfig) -> Option<Vec<String>> {
        None
    }

    fn response_items<'a>(&self, body: &'a Value) -> Option<&'a [Value]> {
        let items = body.get("jsonArray")?.as_array()?;
        Some(items.as_slice())
    }

    fn map_item_to_announcement(&self, item: &Value) -> Option<Announcement> {
        BizInfoCollector::map_item_to_announcement(item)
    }

    fn query_params(&self, _search_term: Option<&str>) -> Vec<(&str, String)> {
        let config = Self::load_config();
        let search_count = config.num_announcements.unwrap_or(10);

        vec![
            ("crtfcKey", self.api_key().to_string()),
            ("dataType", "json".to_string()),
            ("searchCnt", search_count.to_string()),
            ("searchLclasId", "02".to_string()),
        ]
    }
}

#[cfg(test)]
mod bizinfo_tests {
    use super::*;
    use mockito::{Matcher, Server};
    use std::env::set_var;

    use crate::collectors::convert_to_date;

    #[test]
    fn test_collector_construction() {
        BizInfoCollector::new(None, None);
        assert_eq!(BizInfoCollector::type_identifier(), "bizinfo");
    }

    #[test]
    fn test_url_build() {
        unsafe {
            set_var("BIZINFO_KEY", "dummy_bizinfo_key");
        }

        let c = BizInfoCollector::new(None, None);
        let urls = c.get_fetch_urls();

        assert_eq!(1, urls.len());
        assert_eq!(
            "https://www.bizinfo.go.kr/uss/rss/bizinfoApi.do?crtfcKey=dummy_bizinfo_key&dataType=json&searchCnt=10&searchLclasId=02",
            urls[0]
        );
    }

    #[tokio::test]
    async fn test_fetched_data_processing() {
        unsafe {
            set_var("BIZINFO_KEY", "dummy_bizinfo_key");
        }

        let rcv_data = r#"{
            "jsonArray": [
                {
                    "trgetNm": "중소기업",
                    "updtPnttm": "2026-03-27 16:23:43",
                    "hashtags": "기술,서울,부산,대구,인천,광주,대전,울산,세종,경기,강원,충북,충남,전북,전남,경북,경남,제주,2026,산업통상부,한국산업기술진흥원,AI,솔루션,실증,수요기업,공급기업,데이터진단,산업데이터",
                    "inqireCo": 2845,
                    "creatPnttm": "2026-03-27 15:09:38",
                    "pblancNm": "2026년 산업AI 솔루션 실증ㆍ확산 지원 시행계획 공고",
                    "pblancId": "PBLN_000000000120087",
                    "printFlpthNm": "https://www.bizinfo.go.kr/cmm/fms/getImageFile.do?atchFileId=FILE_000000000748887&fileSn=0",
                    "refrncNm": "(사업문의) 한국산업기술진흥원 산업인공지능혁신실 02-6009-3642 / (사업관리시스템 관련 문의) 유지보수팀 02-6009-4319~4322 / (소관부처) 산업통상부 산업인공지능정책과 044-203-3832",
                    "rceptEngnHmpgUrl": "https://www.k-pass.kr/notice/ancView.do?ancId=P2982&gubun=NEW&pageIndex=1&searchAncTitNm=&searchAncGrCd=&searchPrgWorkCd=&searchSrtDtm=&searchEndDtm=#",
                    "fileNm": "[붙임3] 국가연구개발사업 및 산업기술혁신사업 관련 규정.zip@[붙임2] 2026년 산업AI 솔루션 실증·확산 지원 신청서식.hwp",
                    "pblancUrl": "https://www.bizinfo.go.kr/sii/siia/selectSIIA200Detail.do?pblancId=PBLN_000000000120087",
                    "jrsdInsttNm": "산업통상부",
                    "excInsttNm": "한국산업기술진흥원",
                    "totCnt": 225,
                    "reqstMthPapersCn": "온라인 접수 (KIAT 과제관리시스템)",
                    "pldirSportRealmLclasCodeNm": "기술",
                    "reqstBeginEndDe": "2026-03-19 ~ 2026-04-20",
                    "flpthNm": "https://www.bizinfo.go.kr/cmm/fms/getImageFile.do?atchFileId=FILE_000000000748888&fileSn=0@https://www.bizinfo.go.kr/cmm/fms/getImageFile.do?atchFileId=FILE_000000000748888&fileSn=1",
                    "bsnsSumryCn": "<p>산업통상부에서 「2026년 산업AI 솔루션 실증ㆍ확산 지원 시행계획」신규사업을 아래와 같이 공고하오니, 참여를 희망하는 기관은 신청하여 주시기 바랍니다.</p><p><br></p><p style=\"line-height: 1.8;\">☞&nbsp;전 제조 및 제조 서비스 업종 대상</p><p style=\"line-height: 1.8;\">- 주관연구개발기관 : 전문연구기관, 업종별 협ㆍ단체 등 비영리기관</p><p style=\"line-height: 1.8;\">-&nbsp;공동연구개발기관 : 산업 AI 수요기업, 산업 AI 공급기업, 기타 비영리기관 등으로 자율 구성</p><p style=\"line-height: 1.8;\">※ 자세한 지원대상 공고문 참조</p><p><br></p><p style=\"line-height: 1.8;\">☞&nbsp;기업의 데이터 진단부터 AI 솔루션 실증까지 산업데이터의 확보 및 솔루션 구축 전반에 필요한 비용 지원</p><p style=\"line-height: 1.8;\">※ 자세한 지원내용 공고문 참조</p>",
                    "pldirSportRealmMlsfcCodeNm": "공동기술개발",
                    "printFileNm": "[붙임1] 2026년 산업AI 솔루션 실증·확산 지원 시행계획.hwp"
                }
            ]
        }
        "#;

        let mut server = Server::new_async().await;
        let _mock = server
            .mock("GET", "/some_api")
            .match_query(Matcher::Any)
            .with_body(rcv_data)
            .create_async()
            .await;
        let server_url = format!("{}/some_api", server.url());
        let collector = BizInfoCollector::new(Some(server_url), None);

        let announcements = collector.fetch_latest().await.unwrap();
        assert!(!announcements.is_empty());
        assert_eq!(1, announcements.len());

        let a = &announcements[0];
        assert!(a.id.is_some());
        assert!(a.post_id.is_none()); // No post_id.
        assert_eq!(PostSource::BIZINFO, a.post_source);
        assert_eq!(
            "2026년 산업AI 솔루션 실증ㆍ확산 지원 시행계획 공고",
            a.title
        );
        assert_eq!(
            "<p>산업통상부에서 「2026년 산업AI 솔루션 실증ㆍ확산 지원 시행계획」신규사업을 아래와 같이 공고하오니, 참여를 희망하는 기관은 신청하여 주시기 바랍니다.</p><p><br></p><p style=\"line-height: 1.8;\">☞&nbsp;전 제조 및 제조 서비스 업종 대상</p><p style=\"line-height: 1.8;\">- 주관연구개발기관 : 전문연구기관, 업종별 협ㆍ단체 등 비영리기관</p><p style=\"line-height: 1.8;\">-&nbsp;공동연구개발기관 : 산업 AI 수요기업, 산업 AI 공급기업, 기타 비영리기관 등으로 자율 구성</p><p style=\"line-height: 1.8;\">※ 자세한 지원대상 공고문 참조</p><p><br></p><p style=\"line-height: 1.8;\">☞&nbsp;기업의 데이터 진단부터 AI 솔루션 실증까지 산업데이터의 확보 및 솔루션 구축 전반에 필요한 비용 지원</p><p style=\"line-height: 1.8;\">※ 자세한 지원내용 공고문 참조</p>",
            a.content
        );
        assert_eq!(
            "https://www.bizinfo.go.kr/sii/siia/selectSIIA200Detail.do?pblancId=PBLN_000000000120087",
            a.post_url.clone().expect("post_url should be valid")
        );

        // Date
        let begin_date = convert_to_date("20260319");
        let end_date = convert_to_date("20260420");

        assert_eq!(begin_date, a.begin_date);
        assert_eq!(end_date, a.end_date);
    }
}
