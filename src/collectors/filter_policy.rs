use super::date::convert_to_date;
use crate::noti::Announcement;

use chrono::NaiveDate;
use log::{debug, warn};

// Filter policy
#[derive(Debug, Clone)]
pub struct FilterPolicy {
    end_date: Option<NaiveDate>,
    word_filter: bool,
}

impl FilterPolicy {
    /// Create a FilterPolicy instance.
    /// * filter_end_date: end date to be filtered(exclusively) in YYYYMMDD format.
    ///   Set it to None to skip date filtering.
    /// * word_filter: Use search_words in config if true.
    pub fn new(filter_end_date: Option<&str>, word_filter: bool) -> Self {
        FilterPolicy {
            end_date: filter_end_date.map(convert_to_date),
            word_filter,
        }
    }

    pub(crate) fn should_keep(
        &self,
        announcement: &Announcement,
        search_words: Option<&[String]>,
    ) -> bool {
        self.keep_by_date(announcement) && self.keep_by_keywords(announcement, search_words)
    }

    fn keep_by_date(&self, announcement: &Announcement) -> bool {
        match self.end_date {
            Some(end_date) => end_date < announcement.end_date,
            None => true,
        }
    }

    fn keep_by_keywords(
        &self,
        announcement: &Announcement,
        search_words: Option<&[String]>,
    ) -> bool {
        // Do not filter if word_filter is false.
        if !self.word_filter || search_words.is_none() {
            debug!("No keyword filtering(overridden)");
            return true;
        }

        let normalized_words: Vec<String> = search_words
            .unwrap()
            .iter()
            .map(|w| w.trim().to_string())
            .filter(|w| !w.is_empty())
            .collect();

        if normalized_words.is_empty() {
            warn!("Normalized words are empty!");
            return true;
        }

        let title = &announcement.title;
        let content = &announcement.content;

        normalized_words
            .iter()
            .any(|w| title.contains(w) || content.contains(w))
    }
}
