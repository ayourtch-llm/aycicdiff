/// Represents a parsed IOS command with keyword, arguments, and negation state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    /// The primary keyword (e.g., "hostname", "ip", "interface")
    pub keyword: String,
    /// Remaining arguments after the keyword
    pub args: Vec<String>,
    /// Whether the command is negated (starts with "no")
    pub negated: bool,
}

impl Command {
    /// Parse a command string into its components.
    pub fn parse(text: &str) -> Self {
        let trimmed = text.trim();
        let parts: Vec<&str> = trimmed.split_whitespace().collect();

        if parts.is_empty() {
            return Command {
                keyword: String::new(),
                args: Vec::new(),
                negated: false,
            };
        }

        let (negated, keyword_idx) = if parts[0].eq_ignore_ascii_case("no") && parts.len() > 1 {
            (true, 1)
        } else {
            (false, 0)
        };

        let keyword = parts[keyword_idx].to_string();
        let args = parts[keyword_idx + 1..]
            .iter()
            .map(|s| s.to_string())
            .collect();

        Command {
            keyword,
            args,
            negated,
        }
    }

    /// Return the full text representation of this command.
    pub fn to_text(&self) -> String {
        let mut parts = Vec::new();
        if self.negated {
            parts.push("no".to_string());
        }
        parts.push(self.keyword.clone());
        parts.extend(self.args.iter().cloned());
        parts.join(" ")
    }
}

/// Set of singleton keywords where identity is the keyword alone,
/// not the full command text. Two commands with the same singleton
/// keyword are considered the same command (the second replaces the first).
pub static SINGLETON_KEYWORDS: &[&str] = &[
    "hostname",
    "enable secret",
    "enable password",
    "service timestamps debug",
    "service timestamps log",
    "ip domain name",
    "ip domain-name",
    "ip name-server",
    "logging buffered",
    "clock timezone",
    "boot-start-marker",
    "boot-end-marker",
    "ip default-gateway",
    "ip http server",
    "ip http secure-server",
    "ip http authentication",
    "ip ssh version",
    "spanning-tree mode",
];

/// Check if a command text matches a singleton keyword.
/// Returns the matching singleton key if found.
pub fn singleton_key(text: &str) -> Option<&'static str> {
    let trimmed = text.trim();
    // Strip leading "no " for matching
    let check = if let Some(rest) = trimmed.strip_prefix("no ") {
        rest.trim()
    } else {
        trimmed
    };

    for &kw in SINGLETON_KEYWORDS {
        if check.starts_with(kw) {
            // Make sure it's a word boundary (space or end of string after keyword)
            let rest = &check[kw.len()..];
            if rest.is_empty() || rest.starts_with(' ') {
                return Some(kw);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let cmd = Command::parse("hostname Router1");
        assert_eq!(cmd.keyword, "hostname");
        assert_eq!(cmd.args, vec!["Router1"]);
        assert!(!cmd.negated);
    }

    #[test]
    fn test_parse_negated() {
        let cmd = Command::parse("no ip domain-lookup");
        assert_eq!(cmd.keyword, "ip");
        assert_eq!(cmd.args, vec!["domain-lookup"]);
        assert!(cmd.negated);
    }

    #[test]
    fn test_singleton_key() {
        assert_eq!(singleton_key("hostname Router1"), Some("hostname"));
        assert_eq!(singleton_key("no hostname"), Some("hostname"));
        assert_eq!(singleton_key("ip domain-name example.com"), Some("ip domain-name"));
        assert_eq!(singleton_key("interface GigabitEthernet0/0"), None);
    }
}
