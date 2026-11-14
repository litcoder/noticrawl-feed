use super::filter_policy::FilterPolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(dead_code)] // For the UNKNOWN.
pub enum PostSource {
    UNKNOWN,
    KSTARTUP,
    BIZINFO,
    ENARA,
    NARAMARKET,
    SMTECH,
}

#[derive(Debug, Clone)]
pub struct CollectorContext {
    source: PostSource,
    base_url: String,
    api_key: String,
    pub(crate) filter_policy: Option<FilterPolicy>,
    search_terms: Option<Vec<String>>,
}

impl CollectorContext {
    pub fn new(
        source: PostSource,
        base_url: String,
        api_key: String,
        filter_policy: Option<FilterPolicy>,
        search_terms: Option<Vec<String>>,
    ) -> Self {
        Self {
            source,
            base_url,
            api_key,
            filter_policy,
            search_terms,
        }
    }

    pub fn source(&self) -> PostSource {
        self.source
    }

    pub fn search_terms(&self) -> Option<&[String]> {
        self.search_terms.as_deref()
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn api_key(&self) -> &str {
        &self.api_key
    }
}

impl PostSource {
    pub fn descriptive_name(&self) -> &'static str {
        match self {
            PostSource::UNKNOWN => "Unknown",
            PostSource::KSTARTUP => "KStartup",
            PostSource::BIZINFO => "기업마당",
            PostSource::ENARA => "e나라도움",
            PostSource::NARAMARKET => "나라장터",
            PostSource::SMTECH => "중소벤처24",
        }
    }

    pub fn identifier(&self) -> &'static str {
        match self {
            PostSource::UNKNOWN => "unknown",
            PostSource::KSTARTUP => "kstartup",
            PostSource::BIZINFO => "bizinfo",
            PostSource::ENARA => "enara",
            PostSource::NARAMARKET => "naramarket",
            PostSource::SMTECH => "smtech",
        }
    }
}
