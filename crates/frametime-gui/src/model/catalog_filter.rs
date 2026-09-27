use super::PROFILE_PREFERENCES;
pub fn profile_preference_is_valid(value: &str) -> bool {
    PROFILE_PREFERENCES.contains(&value)
}
/// The native ListView filter deliberately searches the operator-facing category and status columns, not hidden detail text.
pub fn catalog_row_matches_filter(category: &str, status: &str, filter: &str) -> bool {
    let filter = filter.trim().to_ascii_lowercase();
    filter.is_empty()
        || category.to_ascii_lowercase().contains(&filter)
        || status.to_ascii_lowercase().contains(&filter)
}
pub const fn catalog_filter_accessible_name() -> &'static str {
    "Category and status filter"
}
#[cfg(test)]
pub const STANDARD_TAB_ORDER: [&str; 4] = [
    "area navigation",
    "area actions",
    "Category and status filter",
    "catalog table",
];
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filter_and_accessibility_contracts_are_stable() {
        assert!(catalog_row_matches_filter(
            "Phase 1",
            "complete / expected",
            "phase"
        ));
        assert!(catalog_row_matches_filter("Phase 1", "Complete", "COMP"));
        assert!(!catalog_row_matches_filter("Phase 1", "Ready", "recovery"));
        assert!(catalog_row_matches_filter("Phase 1", "Ready", "   "));
        assert_eq!(
            catalog_filter_accessible_name(),
            "Category and status filter"
        );
        assert_eq!(STANDARD_TAB_ORDER[2], catalog_filter_accessible_name());
        assert_eq!(STANDARD_TAB_ORDER.last(), Some(&"catalog table"));
    }
}
