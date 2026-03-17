use regex::Regex;
use std::sync::LazyLock;

/// Classification of a config section determining how diffs are computed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionKind {
    /// Unordered, idempotent body (interfaces, router config bodies, line config).
    /// Children are diffed as a set.
    Set,
    /// Order matters — replaced wholesale when changed (ACLs, prefix-lists).
    OrderedList,
    /// Treated as opaque blob — replaced if different (banners, crypto certs).
    Opaque,
}

struct ClassificationRule {
    pattern: Regex,
    kind: SectionKind,
}

static CLASSIFICATION_RULES: LazyLock<Vec<ClassificationRule>> = LazyLock::new(|| {
    vec![
        ClassificationRule {
            pattern: Regex::new(r"(?i)^ip access-list\b").unwrap(),
            kind: SectionKind::OrderedList,
        },
        ClassificationRule {
            pattern: Regex::new(r"(?i)^ip prefix-list\b").unwrap(),
            kind: SectionKind::OrderedList,
        },
        ClassificationRule {
            pattern: Regex::new(r"(?i)^banner\b").unwrap(),
            kind: SectionKind::Opaque,
        },
        ClassificationRule {
            pattern: Regex::new(r"(?i)^crypto pki certificate\b").unwrap(),
            kind: SectionKind::Opaque,
        },
        // Everything else is Set (interfaces, router blocks, line, etc.)
    ]
});

/// Classify a section header into a SectionKind.
pub fn classify_section(header: &str) -> SectionKind {
    let trimmed = header.trim();
    for rule in CLASSIFICATION_RULES.iter() {
        if rule.pattern.is_match(trimmed) {
            return rule.kind;
        }
    }
    SectionKind::Set
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify() {
        assert_eq!(classify_section("interface GigabitEthernet0/0"), SectionKind::Set);
        assert_eq!(classify_section("ip access-list extended MY_ACL"), SectionKind::OrderedList);
        assert_eq!(classify_section("ip prefix-list PFX_LIST"), SectionKind::OrderedList);
        assert_eq!(classify_section("banner motd ^C"), SectionKind::Opaque);
        assert_eq!(classify_section("router ospf 1"), SectionKind::Set);
        assert_eq!(classify_section("line vty 0 4"), SectionKind::Set);
    }
}
