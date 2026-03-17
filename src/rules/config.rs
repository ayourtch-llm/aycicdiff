use std::collections::HashMap;
use std::path::Path;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::model::section_kind::SectionKind;

/// The top-level rules configuration, loaded from TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RulesConfig {
    /// Section classification rules: header pattern → Set/OrderedList/Opaque.
    #[serde(default)]
    pub sections: SectionRules,

    /// Singleton keyword definitions (identity = keyword prefix, not full text).
    #[serde(default)]
    pub singletons: SingletonRules,

    /// Negation override pairs.
    #[serde(default)]
    pub negation: NegationRules,

    /// Commands that are version defaults and should be ignored in diffs.
    #[serde(default)]
    pub defaults: DefaultRules,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SectionRules {
    /// Patterns classified as OrderedList (regex, matched against section header).
    #[serde(default)]
    pub ordered: Vec<String>,
    /// Patterns classified as Opaque.
    #[serde(default)]
    pub opaque: Vec<String>,
    /// Patterns classified as Set (normally the default, but allows overriding
    /// a built-in classification).
    #[serde(default)]
    pub set: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SingletonRules {
    /// Keyword prefixes where identity = keyword, not full command text.
    #[serde(default)]
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct NegationRules {
    /// Pairs of [command, negated_form]. Both directions are registered.
    #[serde(default)]
    pub pairs: Vec<[String; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct DefaultRules {
    /// Commands to suppress (treat as implicit defaults) regardless of platform.
    #[serde(default)]
    pub common: Vec<String>,
    /// Additional defaults for IOS-XE only.
    #[serde(default)]
    pub ios_xe: Vec<String>,
    /// Additional defaults for IOS classic only.
    #[serde(default)]
    pub ios_classic: Vec<String>,
}

impl Default for RulesConfig {
    fn default() -> Self {
        Self::builtin()
    }
}

impl RulesConfig {
    /// Return the built-in default rules (what was previously hardcoded).
    pub fn builtin() -> Self {
        RulesConfig {
            sections: SectionRules {
                ordered: vec![
                    // Access control lists — order of entries matters
                    r"(?i)^ip access-list\b".into(),
                    r"(?i)^ipv6 access-list\b".into(),
                    r"(?i)^mac access-list\s+extended\b".into(),
                    // Numbered ACLs (when they appear as sections)
                    r"(?i)^access-list\b".into(),
                    // Prefix lists — sequence-number ordered
                    r"(?i)^ip prefix-list\b".into(),
                    r"(?i)^ipv6 prefix-list\b".into(),
                    // AS-path and community lists — match order matters
                    r"(?i)^ip as-path access-list\b".into(),
                    r"(?i)^ip community-list\b".into(),
                    r"(?i)^ip extcommunity-list\b".into(),
                    // Kron policy lists — ordered sequence of CLI commands
                    r"(?i)^kron policy-list\b".into(),
                ],
                opaque: vec![
                    // Banners — delimiter-bounded text blobs
                    r"(?i)^banner\b".into(),
                    // Crypto certificates — hex block
                    r"(?i)^crypto pki certificate\b".into(),
                ],
                set: vec![],
            },
            singletons: SingletonRules {
                keywords: vec![
                    "hostname".into(),
                    "enable secret".into(),
                    "enable password".into(),
                    "service timestamps debug".into(),
                    "service timestamps log".into(),
                    "ip domain name".into(),
                    "ip domain-name".into(),
                    "ip name-server".into(),
                    "logging buffered".into(),
                    "clock timezone".into(),
                    "boot-start-marker".into(),
                    "boot-end-marker".into(),
                    "ip default-gateway".into(),
                    "ip http server".into(),
                    "ip http secure-server".into(),
                    "ip http authentication".into(),
                    "ip ssh version".into(),
                    "spanning-tree mode".into(),
                ],
            },
            negation: NegationRules {
                pairs: vec![
                    ["shutdown".into(), "no shutdown".into()],
                    ["ip domain-lookup".into(), "no ip domain-lookup".into()],
                    ["ip routing".into(), "no ip routing".into()],
                    ["ip cef".into(), "no ip cef".into()],
                    ["cdp enable".into(), "no cdp enable".into()],
                    ["lldp transmit".into(), "no lldp transmit".into()],
                    ["lldp receive".into(), "no lldp receive".into()],
                ],
            },
            defaults: DefaultRules {
                common: vec![
                    "service timestamps debug datetime msec".into(),
                    "service timestamps log datetime msec".into(),
                    "service password-encryption".into(),
                    "ip cef".into(),
                    "no ip domain-lookup".into(),
                ],
                ios_xe: vec![
                    "ip http server".into(),
                    "ip http secure-server".into(),
                    "ip http authentication local".into(),
                ],
                ios_classic: vec![],
            },
        }
    }

    /// Serialize the effective rules as TOML (for --dump-rules).
    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_else(|e| format!("# Error serializing rules: {}", e))
    }

    /// Load from a TOML file, merging with built-in defaults.
    /// User rules are appended to (not replacing) the built-in rules.
    pub fn load_from_file(path: &Path) -> Result<Self, RulesError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| RulesError::Io(path.display().to_string(), e))?;
        let user: RulesConfig =
            toml::from_str(&content).map_err(|e| RulesError::Parse(path.display().to_string(), e))?;

        let mut merged = Self::builtin();
        merged.merge(user);
        Ok(merged)
    }

    /// Merge another RulesConfig into this one (appending, not replacing).
    pub fn merge(&mut self, other: RulesConfig) {
        self.sections.ordered.extend(other.sections.ordered);
        self.sections.opaque.extend(other.sections.opaque);
        // "set" overrides can override a built-in ordered/opaque classification,
        // so we store them and check them with priority in classify_section.
        self.sections.set.extend(other.sections.set);

        self.singletons.keywords.extend(other.singletons.keywords);

        self.negation.pairs.extend(other.negation.pairs);

        self.defaults.common.extend(other.defaults.common);
        self.defaults.ios_xe.extend(other.defaults.ios_xe);
        self.defaults.ios_classic.extend(other.defaults.ios_classic);
    }

    // --- Compiled accessors used by the rest of the codebase ---

    /// Classify a section header into a SectionKind.
    pub fn classify_section(&self, header: &str) -> SectionKind {
        let trimmed = header.trim();

        // "set" overrides have highest priority (lets user un-order something)
        for pat in &self.sections.set {
            if let Ok(re) = Regex::new(pat) {
                if re.is_match(trimmed) {
                    return SectionKind::Set;
                }
            }
        }

        for pat in &self.sections.ordered {
            if let Ok(re) = Regex::new(pat) {
                if re.is_match(trimmed) {
                    return SectionKind::OrderedList;
                }
            }
        }

        for pat in &self.sections.opaque {
            if let Ok(re) = Regex::new(pat) {
                if re.is_match(trimmed) {
                    return SectionKind::Opaque;
                }
            }
        }

        SectionKind::Set
    }

    /// Check if a command text matches a singleton keyword.
    /// Returns the matching keyword prefix if found.
    pub fn singleton_key<'a>(&'a self, text: &str) -> Option<&'a str> {
        let trimmed = text.trim();
        let check = trimmed.strip_prefix("no ").map(|s| s.trim()).unwrap_or(trimmed);

        for kw in &self.singletons.keywords {
            if check.starts_with(kw.as_str()) {
                let rest = &check[kw.len()..];
                if rest.is_empty() || rest.starts_with(' ') {
                    return Some(kw.as_str());
                }
            }
        }
        None
    }

    /// Build the negation lookup map.
    pub fn negation_map(&self) -> HashMap<String, String> {
        let mut m = HashMap::new();
        for pair in &self.negation.pairs {
            m.insert(pair[0].clone(), pair[1].clone());
            m.insert(pair[1].clone(), pair[0].clone());
        }
        m
    }

    /// Get default commands to suppress for a given platform.
    pub fn default_commands(&self, platform: &crate::model::version_info::Platform) -> Vec<&str> {
        use crate::model::version_info::Platform;
        let mut defaults: Vec<&str> = self.defaults.common.iter().map(|s| s.as_str()).collect();
        match platform {
            Platform::IosXe => {
                defaults.extend(self.defaults.ios_xe.iter().map(|s| s.as_str()));
            }
            Platform::IosClassic => {
                defaults.extend(self.defaults.ios_classic.iter().map(|s| s.as_str()));
            }
            Platform::Unknown => {}
        }
        defaults
    }

    /// Check if a command is a version default.
    pub fn is_default_command(&self, text: &str, platform: &crate::model::version_info::Platform) -> bool {
        let trimmed = text.trim();
        self.default_commands(platform).iter().any(|&d| trimmed == d)
    }
}

#[derive(Debug)]
pub enum RulesError {
    Io(String, std::io::Error),
    Parse(String, toml::de::Error),
}

impl std::fmt::Display for RulesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RulesError::Io(path, e) => write!(f, "reading rules file '{}': {}", path, e),
            RulesError::Parse(path, e) => write!(f, "parsing rules file '{}': {}", path, e),
        }
    }
}

impl std::error::Error for RulesError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_classify() {
        let rules = RulesConfig::builtin();
        assert_eq!(rules.classify_section("ip access-list extended FOO"), SectionKind::OrderedList);
        assert_eq!(rules.classify_section("ipv6 access-list MY_V6_ACL"), SectionKind::OrderedList);
        assert_eq!(rules.classify_section("ip prefix-list PFX"), SectionKind::OrderedList);
        assert_eq!(rules.classify_section("banner motd ^C"), SectionKind::Opaque);
        assert_eq!(rules.classify_section("interface Gi0/0"), SectionKind::Set);
    }

    #[test]
    fn test_builtin_singletons() {
        let rules = RulesConfig::builtin();
        assert_eq!(rules.singleton_key("hostname Router1"), Some("hostname"));
        assert_eq!(rules.singleton_key("no hostname"), Some("hostname"));
        assert_eq!(rules.singleton_key("interface Gi0/0"), None);
    }

    #[test]
    fn test_parse_toml() {
        let toml_str = r#"
[sections]
ordered = ['(?i)^kron policy-list\b']
opaque = ['(?i)^macro name\b']

[singletons]
keywords = ["ip default-network"]

[negation]
pairs = [["switchport", "no switchport"]]

[defaults]
common = ["ip classless"]
"#;
        let user: RulesConfig = toml::from_str(toml_str).unwrap();
        let mut rules = RulesConfig::builtin();
        rules.merge(user);

        assert_eq!(rules.classify_section("kron policy-list FOO"), SectionKind::OrderedList);
        assert_eq!(rules.classify_section("macro name MY_MACRO"), SectionKind::Opaque);
        assert!(rules.singleton_key("ip default-network 10.0.0.0").is_some());
    }

    #[test]
    fn test_set_override() {
        let toml_str = r#"
[sections]
set = ['(?i)^ip access-list standard\b']
"#;
        let user: RulesConfig = toml::from_str(toml_str).unwrap();
        let mut rules = RulesConfig::builtin();
        rules.merge(user);

        // "set" overrides take priority — this ACL is now treated as Set
        assert_eq!(rules.classify_section("ip access-list standard FOO"), SectionKind::Set);
        // But extended ACLs are still OrderedList
        assert_eq!(rules.classify_section("ip access-list extended BAR"), SectionKind::OrderedList);
    }
}
