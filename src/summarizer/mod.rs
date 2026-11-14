use gemini::GeminiProvider;
use indexmap::IndexSet;
use interface::SummaryProvider;
use log::{debug, error};
use reqwest::Client;
use std::fs::read_to_string;
use std::path::PathBuf;
use std::time::Duration;

use crate::config::Config;
use crate::noti::Announcement;

pub mod brief;
mod gemini;
mod interface;
pub mod summary;

async fn send_gemini_request(
    provider: &GeminiProvider,
    request_name: &str,
    request_body: &gemini::GeminiRequest,
) -> Result<reqwest::Response, Box<dyn std::error::Error>> {
    let conf = Config::instance();
    let url = provider.query_url();

    let connection_timeout = Duration::from_secs(conf.gemini.connection_timeout_secs);
    let request_timeout = Duration::from_secs(conf.gemini.request_timeout_secs);
    let client_builder = Client::builder()
        .connect_timeout(connection_timeout)
        .timeout(request_timeout);
    let client = provider.configure_client(client_builder)?.build()?;

    let request = provider.build_http_request(&client, url, request_body)?;
    let response = request.send().await.map_err(|err| {
        error!("{} request failed: {}", request_name, err);
        err
    })?;

    Ok(response)
}

// Summarization prompt for AIs.
pub async fn summarize(
    announcements: &IndexSet<Announcement>,
) -> Result<summary::Summary, Box<dyn std::error::Error>> {
    // System prompt
    const SUMMARY_PROMPT: &str =
"귀하는 전문적인 비즈니스 데이터 분석가입니다. 임무는 제공되는 창업 공고문(Announcement)을 분석하여 단 하나의 정보 누락 없이 요약하는 것입니다.
[출력 규칙]
1. 응답은 반드시 JSON 형식으로 작성할 것.
2. 전체 내용을 포괄하는 제목을 'summary_title' 태그로 포함하고, 몇 건의 공고를 요약했는지 표기할 것.
3. 각 요약 아이템은 'announcements' 배열 내에 id, project_title, eligibilities 배열, benefits 배열 형식으로 작성할 것.
4. id의 값은 입력되는 Announcement의 번호 값을 그대로 유지할 것.
5. 요약 내용은 가독성을 위해 개조식(Bullet point) 구조의 문자열로 구성할 것.
6. Content 항목에 있는 URL은 그내용을 읽어서 요약할 것.
7. 혜택금액을 benefits에 포함하되 금액은 읽기 쉬운 단어로 표시 할 것.
8. 표(Table) 형식은 절대 사용하지 말 것.
9. 인공 지능 혹은 스타트업 관련 지원사항을 상위순서로 배치하고 지역 특화 사업은 하위에 배치할 것.";

    let provider = GeminiProvider;
    let query = provider.as_query_string(announcements);

    // Debug - printout the query itself.
    debug!("QUERY: {}", query);

    let request_body = provider.build_request(SUMMARY_PROMPT, &query);
    let response = send_gemini_request(&provider, "Summarizer", &request_body).await?;

    let s = provider.process_response(response).await?;
    Ok(s)
}

pub async fn abstract_summary(
    rss_filepath: &PathBuf,
) -> Result<String, Box<dyn std::error::Error>> {
    // Abstract summarization prompt: summarize the RSS feed as a readable plain-text digest.
    const ABSTRACT_SUMMARY_PROMPT: &str =
"제공되는 RSS 피드에는 여러 창업 공고의 요약이 포함되어 있습니다.아래 내용을 참고하여 전체 내용을 하나의 읽기 쉬운 텍스트로 요약하십시오.
[작성규칙]
1. 전체 공고를 아우르는 한 문단짜리 개요(overview)를 오늘 날짜를 포함하여 작성할 것.
2. 출력 언어는 한국어로 할 것.
3. 인공 지능 혹은 스타트업 관련사항은 개요에 포함되도록 할 것.
4. 각 공고의 제목만으로 상위 5개의 항목을 포함할 것.
5. JSON 형식으로 응답하고 overview, titles tag를 사용해서 응답할 것";

    let provider = GeminiProvider;

    let rss_content = read_to_string(rss_filepath).map_err(|e| {
        error!("Failed to read RSS file {}: {}", rss_filepath.display(), e);
        e
    })?;

    debug!(
        "abstract_summary: RSS content length = {}",
        rss_content.len()
    );

    let request_body = provider.build_request(ABSTRACT_SUMMARY_PROMPT, &rss_content);
    let response = send_gemini_request(&provider, "abstract_summary", &request_body).await?;

    let text = provider.extract_text_response(response).await?;
    Ok(text)
}
