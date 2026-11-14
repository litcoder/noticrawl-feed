use chrono::NaiveDate;

/// Converts a YYYYMMDD or YYYY-MM-DD string to local NaiveDate.
/// Panics on parse failure - use only with known-valid input (e.g. config, test fixtures).
pub(crate) fn convert_to_date(datetime: &str) -> NaiveDate {
    try_convert_to_date(datetime)
        .unwrap_or_else(|| panic!("Failed to parse date string: '{}'", datetime))
}

/// Tries to convert a YYYYMMDD or YYYY-MM-DD string to NaiveDate.
/// Returns None on any parse failure, suitable for use with live API data.
pub(crate) fn try_convert_to_date(datetime: &str) -> Option<NaiveDate> {
    let combined_str = datetime.split('-').collect::<Vec<_>>().join("");
    NaiveDate::parse_from_str(&combined_str, "%Y%m%d").ok()
}
