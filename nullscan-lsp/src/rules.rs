//! The rule catalog served to editors through the `nullscan/rules` custom request.
//!
//! `nullscan_core::RULES` stays the single source of truth: this module only reshapes it
//! into a serializable payload and builds the documentation link for each rule.

use nullscan_core::{Rule, RULES};
use serde::Serialize;

/// The custom request that returns the catalog; parameters are ignored.
pub const RULES_METHOD: &str = "nullscan/rules";

/// Base of the public rule documentation; every rule appends `#<rule-id>`.
pub const DOCS_BASE_URL: &str = "https://github.com/bytezer/nullscan/blob/main/RULES.md";

/// One rule as an editor can render it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuleInfo {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub why: &'static str,
    /// Lowercase level: `error`, `warning` or `info`.
    pub severity: &'static str,
    pub docs_url: String,
}

/// Every registered rule, in registry order.
pub fn catalog() -> Vec<RuleInfo> {
    RULES.iter().map(rule_info).collect()
}

/// The `RULES.md` anchor for `rule`, used by the catalog, hover and `codeDescription`.
pub fn docs_url(rule: &str) -> String {
    format!("{DOCS_BASE_URL}#{rule}")
}

fn rule_info(rule: &'static Rule) -> RuleInfo {
    let meta = &rule.meta;
    RuleInfo {
        id: meta.id,
        title: meta.title,
        description: meta.description,
        why: meta.why,
        severity: meta.severity.as_str(),
        docs_url: docs_url(meta.id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_covers_every_registered_rule() {
        assert_eq!(catalog().len(), 57);
        assert_eq!(catalog().len(), RULES.len());
    }

    #[test]
    fn every_field_is_populated_and_links_to_its_anchor() {
        for info in catalog() {
            assert!(!info.id.is_empty(), "empty id");
            assert!(!info.title.trim().is_empty(), "'{}' has no title", info.id);
            assert!(
                !info.description.trim().is_empty(),
                "'{}' has no description",
                info.id
            );
            assert!(!info.why.trim().is_empty(), "'{}' has no why", info.id);
            assert!(
                matches!(info.severity, "error" | "warning" | "info"),
                "'{}' has severity '{}'",
                info.id,
                info.severity
            );
            assert!(
                info.docs_url.ends_with(&format!("#{}", info.id)),
                "'{}' docs_url is '{}'",
                info.id,
                info.docs_url
            );
        }
    }

    #[test]
    fn docs_url_is_stable_for_a_known_rule() {
        assert_eq!(
            docs_url("eqeqeq"),
            "https://github.com/bytezer/nullscan/blob/main/RULES.md#eqeqeq"
        );
    }
}
