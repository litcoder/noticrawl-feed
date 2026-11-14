use crate::collectors::PostSource;
use chrono::NaiveDate;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Announcement {
    pub id: Option<i64>,
    pub post_id: Option<String>,
    pub post_source: PostSource,
    pub post_url: Option<String>,
    pub email: Option<String>,
    pub title: String,
    pub content: String,
    pub begin_date: NaiveDate,
    pub end_date: NaiveDate,
}
