use async_trait::async_trait;
use log::debug;
use reqwest::{Client, RequestBuilder, Response};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::error::Error;

use crate::config::Config;

use super::interface::SummaryProvider;
use super::summary::Summary;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GeminiRequest {
    contents: Vec<Content>,
    system_instruction: Option<Content>,
    generation_config: Option<GenerationConfig>,
}

#[derive(Serialize, Deserialize)]
struct Content {
    role: Option<String>,
    parts: Vec<Part>,
}

#[derive(Serialize, Deserialize)]
struct Part {
    text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerationConfig {
    temperature: f32,
    max_output_tokens: i32,
    response_mime_type: String,
}

pub(super) struct GeminiProvider;

fn parse_abstract_from_text(raw_text: &str) -> String {
    let trimmed = raw_text.trim();

    if let Ok(parsed) = serde_json::from_str::<Value>(trimmed) {
        // Try to parse as object with overview and titles fields
        if let Some(overview) = parsed.get("overview").and_then(|o| o.as_str()) {
            let mut lines = vec![overview.to_string()];

            // Append titles if they exist
            if let Some(titles) = parsed.get("titles").and_then(|t| t.as_array()) {
                let mut extracted_titles: Vec<String> = titles
                    .iter()
                    .filter_map(|title| title.as_str().map(str::to_string))
                    .collect();

                if let Some(last_title) = extracted_titles.last_mut() {
                    if !last_title.ends_with("...") {
                        last_title.push_str("...");
                    }
                }

                lines.extend(extracted_titles);
            }

            return lines.join("\n\n");
        }
    }

    trimmed.to_string()
}

fn parse_summary_from_text(raw_text: &str) -> Result<Summary, Box<dyn Error>> {
    let trimmed = raw_text.trim();
    debug!("{}", trimmed);

    match serde_json::from_str::<Summary>(trimmed) {
        Ok(summary) => Ok(summary),
        Err(e) => Err(format!("Gemini summary JSON parse failed. (error: {})", e).into()),
    }
}

#[async_trait]
impl SummaryProvider for GeminiProvider {
    type RequestBody = GeminiRequest;
    type SummaryOutput = Summary;

    fn query_url(&self) -> &'static str {
        // TODO: Move to config.
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash-lite:generateContent"
    }

    fn build_request(&self, prompt: &str, query: &str) -> Self::RequestBody {
        let conf = Config::instance();

        GeminiRequest {
            system_instruction: Some(Content {
                role: None,
                parts: vec![Part {
                    text: prompt.to_string(),
                }],
            }),
            contents: vec![Content {
                role: Some("user".to_string()),
                parts: vec![Part {
                    text: query.to_string(),
                }],
            }],
            generation_config: Some(GenerationConfig {
                temperature: conf.gemini.temperature,
                max_output_tokens: conf.gemini.max_output_tokens,
                response_mime_type: "application/json".to_string(),
            }),
        }
    }

    fn build_http_request(
        &self,
        client: &Client,
        url: &str,
        body: &Self::RequestBody,
    ) -> Result<RequestBuilder, Box<dyn Error>> {
        let api_key = self.api_key()?;
        Ok(client
            .post(url)
            .header("x-goog-api-key", api_key)
            .json(body))
    }

    async fn process_response(
        &self,
        response: Response,
    ) -> Result<Self::SummaryOutput, Box<dyn Error>> {
        let summary_text = self.extract_candidate_text(response).await?;
        parse_summary_from_text(&summary_text)
    }
}

impl GeminiProvider {
    pub(super) fn api_key(&self) -> Result<String, Box<dyn Error>> {
        std::env::var("GEMINI_API_KEY").map_err(|_| "API key for AI agent is not set.".into())
    }

    async fn extract_candidate_text(&self, response: Response) -> Result<String, Box<dyn Error>> {
        if response.status().is_success() {
            let body_text = response.text().await?;
            let parsed: Value = serde_json::from_str(&body_text)?;
            let raw_text = parsed
                .get("candidates")
                .and_then(|c| c.as_array())
                .and_then(|arr| arr.first())
                .and_then(|candidate| candidate.get("content"))
                .and_then(|content| content.get("parts"))
                .and_then(|parts| parts.as_array())
                .and_then(|arr| arr.first())
                .and_then(|part| part.get("text"))
                .and_then(|text| text.as_str())
                .ok_or_else(|| {
                    format!(
                        "Gemini response did not include candidate text. Raw response: {}",
                        body_text
                    )
                })?;
            return Ok(raw_text.to_string());
        }

        let err_text = response.text().await?;
        Err(format!("API Error: {}", err_text).into())
    }

    /// Extracts the raw candidate text from a Gemini response without JSON parsing.
    pub(super) async fn extract_text_response(
        &self,
        response: Response,
    ) -> Result<String, Box<dyn Error>> {
        let raw_text = self.extract_candidate_text(response).await?;
        Ok(parse_abstract_from_text(&raw_text))
    }
}

#[cfg(test)]
mod gemini_tests {
    use super::*;
    use mockito::{Matcher, Server};
    use reqwest::Client;

    #[test]
    fn test_query_url_include_api_key() {
        crate::init_main();

        let p = GeminiProvider;
        let url = p.query_url();

        // API key should not be embedded in URL.
        assert!(!url.contains("key="));
    }

    #[test]
    fn test_parsing_gemini_text_section() {
        // Parse the 'text' element in Gemini's response.
        let raw_text = r#"
        {
            "summary_title": "SUMMARY TITLE",
            "summarized": [
            {
                "id": 1,
                "project_title": "PROJECT TITLE",
                "eligibilities": [
                    "ELIGIBILITY"
                ],
                "benefits": [
                    "BENEFIT1",
                    "BENEFIT2"
                ],
                "begin_date": "2026-01-01",
                "end_date": "2026-12-31",
                "source": "SOURCE",
                "url": "SOME_URL"
            } ]
        }"#;
        let result = parse_summary_from_text(raw_text);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_summary_response_parsing() {
        let resp_sample = r#"{
            "candidates": [
                {
                    "content": {
                        "parts": [
                            {
                                "text": "{\n  \"summary_title\": \"2026년 창업 지원 사업 공고 요약 (19건)\",\n  \"announcements\": [\n    {\n      \"id\": 0,\n      \"project_title\": \"2026 SVC Seoul 멤버십(확장형) 기업 모집공고\",\n      \"eligibilities\": [\n        \"글로벌 진출을 희망하는 창업기업\"\n      ],\n      \"benefits\": [\n        \"공유오피스 및 회의공간 이용\",\n        \"전문 자문\",\n        \"오픈이노베이션 프로그램 참여 기회 제공\"\n      ]\n    },\n    {\n      \"id\": 1,\n      \"project_title\": \"2026 혁신성장 청년 예비 소상공인 지원 사업 참여자 모집\",\n      \"eligibilities\": [\n        \"연내 강북구에 점포 창업을 희망하는 청년 (예비) 창업자\"\n      ],\n      \"benefits\": [\n        \"창업 지원\"\n      ]\n    },\n    {\n      \"id\": 2,\n      \"project_title\": \"[동의대학교] 도심형 청년 창업,주거 복합공간 입주기업 모집\",\n      \"eligibilities\": [\n        \"(예비)창업자\"\n      ],\n      \"benefits\": [\n        \"도심형 청년 창업,주거 복합공간 입주\"\n      ]\n    },\n    {\n      \"id\": 3,\n      \"project_title\": \"대학 특화 청년 창업 활성화 지원\",\n      \"eligibilities\": [\n        \"경상남도 내 의생명·의료기기 및 첨단바이오 분야 청년 창업기업\",\n        \"(예비)창업기업\"\n      ],\n      \"benefits\": [\n        \"성장 지원\",\n        \"기술사업화 촉진\",\n        \"투자 연계\",\n        \"창업 생태계 활성화\"\n      ],\n      \"begin_date\": \"2026-04-02\",\n      \"end_date\": \"2026-04-15\",\n      \"source\": \"KStartup\",\n      \"url\": \"https://www.k-startup.go.kr/web/contents/bizpbanc-ongoing.do?schM=view&pbancSn=177037\"\n    },\n    {\n      \"id\": 4,\n      \"project_title\": \"울산창업 L 시리즈\",\n      \"eligibilities\": [\n        \"예비창업자\",\n        \"창업기업\"\n      ],\n      \"benefits\": [\n        \"울산 지역 중심의 혁신창업기반 구축\",\n        \"창업문화 조성\",\n        \"딥테크 유망기술 창업 지원\",\n        \"후속연계 집중 지원\",\n        \"지역발 딥테크 창업활성화\"\n      ]\n    },\n    {\n      \"id\": 5,\n      \"project_title\": \"「고신뢰 반도체 상용화를 위한 검증ㆍ확인」사업 기업 지원 프로그램\",\n      \"eligibilities\": [\n        \"비수도권 소재의 팹리스 기업\"\n      ],\n      \"benefits\": [\n        \"기업지원\",\n        \"전문인력양성\"\n      ]\n    },\n    {\n      \"id\": 6,\n      \"project_title\": \"2026 SBA X 메르세데스 벤츠 코리아 게임 산업 상생 플랫폼 활용 게임개발 지원 참가기업 모집\",\n      \"eligibilities\": [\n        \"서울 소재 우수 게임기업\"\n      ],\n      \"benefits\": [\n        \"게임콘텐츠의 신규 플랫폼 진출 지원\",\n        \"성공 사례 창출\",\n        \"글로벌 진출 역량 강화\"\n      ]\n    },\n    {\n      \"id\": 7,\n      \"project_title\": \"2026년 글로벌 인턴십 지원사업 참가기업 추가 모집(2차) 공고\",\n      \"eligibilities\": [\n        \"동남아 진출을 희망하는 스타트업\",\n        \"우수 스타트업\"\n      ],\n      \"benefits\": [\n        \"글로벌 시장 탐색 기회 제공\",\n        \"네트워킹 기회 확대\"\n      ]\n    },\n    {\n      \"id\": 8,\n      \"project_title\": \"충북대학교 『글로벌(G)테크벤처센터(BI) 』 입주기업 모집\",\n      \"eligibilities\": [\n        \"우수한 기술력과 사업 역량을 가진 유망 벤처기업\",\n        \"신기술 창업자\"\n      ],\n      \"benefits\": [\n        \"지역 유망 중소․벤처기업 발굴 육성\",\n        \"지역경제 활성화 기여\",\n        \"고용창출 선도\",\n        \"글로벌테크벤처센터(POST-BI) 입주\"\n      ]\n    },\n    {\n      \"id\": 9,\n      \"project_title\": \"지역소통협력팀 통합 지원사업 <2026 전주살LAB> 참가자 모집\",\n      \"eligibilities\": [\n        \"참여자\"\n      ],\n      \"benefits\": [\n        \"지역 소통 협력 프로그램 지원\"\n      ]\n    },\n    {\n      \"id\": 10,\n      \"project_title\": \"2026년 온디바이스 AI 서비스 실증ㆍ확산 사업 공고\",\n      \"eligibilities\": [\n        \"온디바이스 AI를 활용한 서비스에 대한 수요가 있는 지자체ㆍ지역산업진흥기관ㆍ민간기업 등으로 구성된 컨소시엄\"\n      ],\n      \"benefits\": [\n        \"AI반도체를 탑재한 온디바이스 AI 디바이스를 활용한 신규 서비스 개발ㆍ실증 지원\",\n        \"AI반도체 기반의 온디바이스 AI 기술을 활용한 혁신 AI 서비스 발굴\",\n        \"온디바이스 AI 서비스 개발ㆍ제작, 실증 및 검증, 서비스 확산 등 관련 사업비 지원\"\n      ]\n    },\n    {\n      \"id\": 11,\n      \"project_title\": \"2026년 1차 보건신기술(NET)인증기술 신청 및 인증기간 연장신청 접수 공고\",\n      \"eligibilities\": [\n        \"보건신기술을 인증 받고자 하는 자 (기업, 국공립(연), 정부출연(연), 대학 등)\",\n        \"의과학, 생명공학, 의약품, 의료기기, 식품위생, 화장품, 한방 분야\",\n        \"아직 상용화 되지 않았으나 향후 2년 이내 상용화 가능한 기술 보유자\",\n        \"실증화시험을 통하여 정량적 평가지표를 확보한 개발완료기술 보유자\",\n        \"제품 생산성이나 품질 향상 공정기술 보유자\"\n      ],\n      \"benefits\": [\n        \"보건신기술(NET) 인증\",\n        \"향후 2년 이내 상용화 지원\",\n        \"기존 제품 성능 개선 지원\",\n        \"생산성 및 품질 향상 공정 지원\"\n      ]\n    },\n    {\n      \"id\": 12,\n      \"project_title\": \"2026년 과학기술정보통신부 GovTech 창업기업 AI 실증ㆍ사업화 지원사업 모집 공고\",\n      \"eligibilities\": [\n        \"GovTech 창업기업 과 수요기관 컨소시엄\",\n        \"7년 이내의 국내 창업기업 (주관기관)\",\n        \"국내 정부, 공공기관, 지자체 등 (수요기관, 참여기관)\"\n      ],\n      \"benefits\": [\n        \"공공부문 대상 GovTech AI 서비스ㆍ솔루션 개발 및 실증ㆍ사업화 자금 지원\",\n        \"창업기업 역량강화 프로그램 (전문 지원기관 통한 성장 지원)\"\n      ]\n    },\n    {\n      \"id\": 13,\n      \"project_title\": \"2026년 SaaS 개발ㆍ사업화 지원 사업 공고\",\n      \"eligibilities\": [\n        \"국내 스타트업\",\n        \"국내 중소SW 기업\",\n        \"AI-Native SaaS 개발을 시작하는 SaaS 기업\"\n      ],\n      \"benefits\": [\n        \"AI-Native SaaS 개발 지원\",\n        \"AI 기반 지능형 클라우드 서비스 개발 지원\",\n        \"글로벌 진출 지원\"\n      ]\n    },\n    {\n      \"id\": 14,\n      \"project_title\": \"AI 응용제품 신속 상용화(보건분야ㆍ디지털 의료기기) 지원사업 참여기업 모집 공고\",\n      \"eligibilities\": [\n        \"디지털 의료기기 또는 의료기기 제조기업\",\n        \"국내 식약처에 제조업 허가를 받은 의료기기, 디지털 의료기기, 체외진단 의료기기 기업\",\n        \"주관기업+의료기관(2개 기관 이상)과 컨소시엄 구성 필수\"\n      ],\n      \"benefits\": [\n        \"시장 진입 각 단계 준비 비용 지원 (다기관 임상 실증, 경제성 평가, 마케팅 등)\",\n        \"과제당 연간 최대 950백만원 지원\"\n      ]\n    },\n    {\n      \"id\": 15,\n      \"project_title\": \"2026년 광물자원분야 신규지원대상 연구개발과제 공고\",\n      \"eligibilities\": [\n        \"기업, 대학, 연구기관, 연구조합, 사업자단체, 의료기관 등\"\n      ],\n      \"benefits\": [\n        \"배터리공정스크랩재자원화ㆍ재제조기술개발 지원\",\n        \"자원안보를 위한 고부가가치 한국형 텅스텐 전주기 기술개발 및 실증 지원\"\n      ]\n    },\n    {\n      \"id\": 16,\n      \"project_title\": \"[제주] 2026년 지역문화산업연구센터(CRC) 지원사업 참여기업 모집 공고\",\n      \"eligibilities\": [\n        \"공고일 현재, 본사 또는 지사를 제주특별자치도에 두고 있는 중소기업(법인)\",\n        \"문화산업 분야 (콘텐츠 개발ㆍ제작 가능)\"\n      ],\n      \"benefits\": [\n        \"BM R&D 고도화 지원\",\n        \"콘텐츠ㆍ기술ㆍ인증 기반 지원\",\n        \"New BM 실증ㆍ개발 지원\",\n        \"콘텐츠/기술 사업화ㆍ상용화 지원\"\n      ]\n    },\n    {\n      \"id\": 17,\n      \"project_title\": \"[경기] 2026년 섬유분야 유해물질 시험분석 및 인증지원 공고\",\n      \"eligibilities\": [\n        \"경기도 내 섬유 관련 중소기업ㆍ소기업 및 소상공인\"\n      ],\n      \"benefits\": [\n        \"시험수수료 지원 (1기업 당 최대 3백만원)\",\n        \"제품 신뢰성 확보 솔루션 제공\"\n      ]\n    },\n    {\n      \"id\": 18,\n      \"project_title\": \"2026년 융복합 기술 오픈이노베이션 지원 참여기업 모집 공고\",\n      \"eligibilities\": [\n        \"민간기술 보유 중소기업\",\n        \"창업기업(스타트업)\",\n        \"모집분야 : 전기트랙터(EV), AX플랫폼, AI 기술지원\"\n      ],\n      \"benefits\": [\n        \"선도기업과의 협력을 통한 실증 및 사업화 기회 제공\",\n        \"자금, 인프라 등 PoC 지원\",\n        \"선도기업 협업(기술검증 등)\",\n        \"후속 상용화 연계 등 스케일업 기회 제공\"\n      ]\n    }\n  ]\n}\n"
                            }
                        ],
                        "role": "model"
                    },
                    "finishReason": "STOP",
                    "index": 0
                }
            ],
            "usageMetadata": {
                "promptTokenCount": 0,
                "candidatesTokenCount": 0,
                "totalTokenCount": 0,
                "promptTokensDetails": [
                    {
                        "modality": "TEXT",
                        "tokenCount": 0
                    }
                ]
            },
            "modelVersion": "gemini-2.5-flash-lite",
            "responseId": "SOME_RESPONSE_ID"
        }"#;

        let mut server = Server::new_async().await;
        let _mock = server
            .mock("GET", "/gemini")
            .match_header("accept", Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(resp_sample)
            .create_async()
            .await;

        let response = Client::new()
            .get(format!("{}/gemini", server.url()))
            .send()
            .await
            .expect("mock response should be created");

        let gemini_provider = GeminiProvider;
        let summary = gemini_provider
            .process_response(response)
            .await
            .expect("sample response should be parsed");

        assert_eq!(
            "2026년 창업 지원 사업 공고 요약 (19건)",
            summary.summary_title
        );
        assert_eq!(19, summary.summarized.len());

        // id, project_title, eligibilities, benefits
        assert_eq!(0, summary.summarized[0].id);
        // project title
        assert_eq!(
            "2026 SVC Seoul 멤버십(확장형) 기업 모집공고",
            summary.summarized[0].project_title
        );
        // eligibility
        assert_eq!(
            "글로벌 진출을 희망하는 창업기업",
            summary.summarized[0].eligibilities[0]
        );
        // benefits
        assert_eq!(
            "공유오피스 및 회의공간 이용",
            summary.summarized[0].benefits[0]
        );
        assert_eq!("전문 자문", summary.summarized[0].benefits[1]);
        assert_eq!(
            "오픈이노베이션 프로그램 참여 기회 제공",
            summary.summarized[0].benefits[2]
        );

        // id 1
        assert_eq!(1, summary.summarized[1].id);
        // project title
        assert_eq!(
            "2026 혁신성장 청년 예비 소상공인 지원 사업 참여자 모집",
            summary.summarized[1].project_title
        );
        // Eligibility
        assert_eq!(
            "연내 강북구에 점포 창업을 희망하는 청년 (예비) 창업자",
            summary.summarized[1].eligibilities[0]
        );
        // Benefits
        assert_eq!(1, summary.summarized[1].benefits.len());
        assert_eq!("창업 지원", summary.summarized[1].benefits[0]);
    }

    #[tokio::test]
    async fn test_abstract_response_parsing() {
        let resp_sample = r#"{
            "candidates": [
                {
                    "content": {
                        "parts": [
                            {
                                "text": "{\n  \"overview\": \"2026년 4월 27일 기준, 총 8건의 창업 및 기업 지원 공고가 발표되었습니다. 이번 공고들은 지역별 특화 산업 육성, 기술 개발 및 고도화 지원, 판로 개척, 글로벌 시장 진출 지원 등 다양한 분야를 포괄하며, 특히 인공지능(AI) 및 스타트업 생태계 활성화를 위한 지원책도 포함되어 있습니다. 차세대 특화망 융합서비스 확산 사업, AI 기반 슬립테크 국제협력 실증 사업 등 첨단 기술을 활용한 혁신 성장 지원이 눈에 띕니다. 또한, 소재부품, 방위산업, 섬유산업, 뿌리산업 등 전통적인 기간산업의 혁신을 위한 공정 장비 개발 및 기술 지원도 병행됩니다. 지역별로는 경남, 경북, 울산, 전남, 강원 지역 기업들을 대상으로 맞춤형 지원이 제공됩니다.\",\n  \"titles\": [\n    \"[경남] 2026년 소재부품 성장 잠재기업 육성사업 수요기업 모집 공고\",\n    \"[경북] 2026년 방위 R&D Spin-On 방산기업 시제품 고도화 지원 및 과제기획지원 참여기업 모집 공고\",\n    \"[울산] 2026년 지역기업 판로개척 패키지 지원사업 기술개발인증 지원사업 모집 공고\",\n    \"2026년 과학기술정보통신부 차세대 특화망 융합서비스 확산 사업 재공고\",\n    \"2026년 2차 뿌리산업 혁신공정장비 개발사업 신규지원 대상과제 공고\"\n  ]\n}"
                            }
                        ],
                        "role": "model"
                    },
                    "finishReason": "STOP",
                    "index": 0
                }
            ],
            "usageMetadata": {
                "promptTokenCount": 2887,
                "candidatesTokenCount": 389,
                "totalTokenCount": 3276,
                "promptTokensDetails": [
                    {
                        "modality": "TEXT",
                        "tokenCount": 2887
                    }
                ]
            },
            "modelVersion": "gemini-2.5-flash-lite",
            "responseId": "SOME_RESPONSE_ID"
        }"#;

        let mut server = Server::new_async().await;
        let _mock = server
            .mock("GET", "/gemini")
            .match_header("accept", Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(resp_sample)
            .create_async()
            .await;

        let response = Client::new()
            .get(format!("{}/gemini", server.url()))
            .send()
            .await
            .expect("mock response should be created");

        let gemini_provider = GeminiProvider;
        let abs_msg = gemini_provider
            .extract_text_response(response)
            .await
            .expect("sample response should be parsed");

        let abs_exp_str = r#"2026년 4월 27일 기준, 총 8건의 창업 및 기업 지원 공고가 발표되었습니다. 이번 공고들은 지역별 특화 산업 육성, 기술 개발 및 고도화 지원, 판로 개척, 글로벌 시장 진출 지원 등 다양한 분야를 포괄하며, 특히 인공지능(AI) 및 스타트업 생태계 활성화를 위한 지원책도 포함되어 있습니다. 차세대 특화망 융합서비스 확산 사업, AI 기반 슬립테크 국제협력 실증 사업 등 첨단 기술을 활용한 혁신 성장 지원이 눈에 띕니다. 또한, 소재부품, 방위산업, 섬유산업, 뿌리산업 등 전통적인 기간산업의 혁신을 위한 공정 장비 개발 및 기술 지원도 병행됩니다. 지역별로는 경남, 경북, 울산, 전남, 강원 지역 기업들을 대상으로 맞춤형 지원이 제공됩니다.

[경남] 2026년 소재부품 성장 잠재기업 육성사업 수요기업 모집 공고

[경북] 2026년 방위 R&D Spin-On 방산기업 시제품 고도화 지원 및 과제기획지원 참여기업 모집 공고

[울산] 2026년 지역기업 판로개척 패키지 지원사업 기술개발인증 지원사업 모집 공고

2026년 과학기술정보통신부 차세대 특화망 융합서비스 확산 사업 재공고

2026년 2차 뿌리산업 혁신공정장비 개발사업 신규지원 대상과제 공고..."#;
        assert_eq!(abs_exp_str, abs_msg);
    }
}
