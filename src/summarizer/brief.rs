use chrono::NaiveDate;
use serde::Deserialize;
use std::fmt;

#[derive(Debug, Deserialize)]
pub struct Brief {
    pub id: i64,
    pub project_title: String,
    pub eligibilities: Vec<String>,
    pub benefits: Vec<String>,
    pub source: Option<String>,
    pub url: Option<String>,
    pub email: Option<String>,
    pub begin_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
}

/// Concat the elements in list as a markdown unordered list string.
fn to_markdown_list(elements: &[String]) -> String {
    if elements.is_empty() {
        String::from("- <명시 없음>")
    } else {
        elements
            .iter()
            .map(|el| format!("- {}", el))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl fmt::Display for Brief {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "**[{}] {}**{}\n\n마감일: {}\n\n자격\n{}\n\n혜택\n{}\n\n",
            self.source.as_ref().unwrap(),
            self.project_title,
            if self.url.is_some() {
                format!(" - [원문 보기]({})", self.url.as_ref().unwrap())
            } else if self.email.is_some() {
                format!(" - [이메일](mailto:{})", self.email.as_ref().unwrap())
            } else {
                String::from("")
            },
            self.end_date.unwrap(),
            to_markdown_list(&self.eligibilities),
            to_markdown_list(&self.benefits),
        )
    }
}

#[cfg(test)]
mod brief_tests {
    use super::*;
    use crate::collectors::{PostSource, convert_to_date};

    // Utility function to create Brief conveniently.
    fn fake_briefs(
        id: i64,
        num_eligibilities: i64,
        num_benefits: i64,
        has_email: bool,
        has_url: bool,
    ) -> Brief {
        let mut benefits: Vec<String> = vec![];
        for i in 0..num_benefits {
            benefits.push(format!("BENEFIT{}", i));
        }

        let mut eligibilities: Vec<String> = vec![];
        for i in 0..num_eligibilities {
            eligibilities.push(format!("ELIGIBILITY{}", i));
        }

        Brief {
            id,
            project_title: format!("PROJECT{}", id),
            eligibilities: eligibilities,
            benefits: benefits,
            source: Some(String::from(PostSource::UNKNOWN.descriptive_name())),
            url: if has_url {
                Some(format!("https://www.example{}.com", id))
            } else {
                None
            },
            email: if has_email {
                Some(format!("contact{}@example.com", id))
            } else {
                None
            },
            begin_date: Some(convert_to_date("20260101")),
            end_date: Some(convert_to_date("20261231")),
        }
    }

    #[test]
    fn test_process_summary() {
        // No URL, no email
        let sum_ann = fake_briefs(0, 1, 1, false, false);
        assert_eq!(
            "**[Unknown] PROJECT0**\n\n마감일: 2026-12-31\n\n자격\n- ELIGIBILITY0\n\n혜택\n- BENEFIT0\n\n",
            format!("{}", sum_ann)
        );

        // Has URL, no email
        let sum_ann = fake_briefs(0, 1, 1, false, true);
        assert_eq!(
            "**[Unknown] PROJECT0** - [원문 보기](https://www.example0.com)\n\n마감일: 2026-12-31\n\n자격\n- ELIGIBILITY0\n\n혜택\n- BENEFIT0\n\n",
            format!("{}", sum_ann)
        );

        // No URL, has email.
        let sum_ann = fake_briefs(0, 1, 1, true, false);
        assert_eq!(
            "**[Unknown] PROJECT0** - [이메일](mailto:contact0@example.com)\n\n마감일: 2026-12-31\n\n자격\n- ELIGIBILITY0\n\n혜택\n- BENEFIT0\n\n",
            format!("{}", sum_ann)
        );

        // URL comes first when there're URL and email information
        let sum_ann = fake_briefs(0, 1, 1, true, true);
        assert_eq!(
            "**[Unknown] PROJECT0** - [원문 보기](https://www.example0.com)\n\n마감일: 2026-12-31\n\n자격\n- ELIGIBILITY0\n\n혜택\n- BENEFIT0\n\n",
            format!("{}", sum_ann)
        );
    }

    #[test]
    fn test_process_summary_no_indented_bullets() {
        let sum_ann = fake_briefs(0, 2, 2, false, false);
        let processed_content = format!("{}", sum_ann);
        let s = vec![
            "",
            "자격",
            "- ELIGIBILITY0",
            "- ELIGIBILITY1",
            "",
            "혜택",
            "- BENEFIT0",
            "- BENEFIT1",
        ]
        .join("\n");
        assert!(processed_content.contains(&s));
    }

    #[test]
    fn test_process_summary_no_eligibilities() {
        let sum_ann = fake_briefs(0, 0, 1, false, false);
        let processed_content = format!("{}", sum_ann);
        let s = vec!["", "자격", "- <명시 없음>", "", "혜택", "- BENEFIT0"].join("\n");
        assert!(processed_content.contains(&s));
    }

    #[test]
    fn test_process_summary_no_benefits() {
        let sum_ann = fake_briefs(0, 1, 0, false, false);
        let processed_content = format!("{}", sum_ann);
        let s = vec!["", "자격", "- ELIGIBILITY0", "", "혜택", "- <명시 없음>"].join("\n");
        assert!(processed_content.contains(&s));
    }

    #[test]
    fn test_process_summary_multiple_eligibilities_and_benefits() {
        let sum_ann = fake_briefs(0, 2, 3, false, false);
        let processed_content = format!("{}", sum_ann);
        let s = vec![
            "",
            "자격",
            "- ELIGIBILITY0",
            "- ELIGIBILITY1",
            "",
            "혜택",
            "- BENEFIT0",
            "- BENEFIT1",
            "- BENEFIT2",
        ]
        .join("\n");
        assert!(processed_content.contains(&s));
    }
}
