use super::brief::Brief;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Summary {
    pub(crate) summary_title: String,
    #[serde(alias = "announcements")]
    pub(crate) summarized: Vec<Brief>,
}
