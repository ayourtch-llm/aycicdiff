use std::collections::HashMap;

use crate::rules::RulesConfig;

/// Generate the negated form of a command (for removal), using the rules config.
pub fn negate_command(text: &str, negation_map: &HashMap<String, String>) -> String {
    let trimmed = text.trim();

    // Check override registry first
    if let Some(negated) = negation_map.get(trimmed) {
        return negated.clone();
    }

    // Default: if it starts with "no ", strip the "no "; otherwise prepend "no "
    if let Some(rest) = trimmed.strip_prefix("no ") {
        rest.to_string()
    } else {
        format!("no {}", trimmed)
    }
}

/// Convenience: negate using a RulesConfig directly.
pub fn negate_command_with_rules(text: &str, rules: &RulesConfig) -> String {
    let map = rules.negation_map();
    negate_command(text, &map)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_map() -> HashMap<String, String> {
        RulesConfig::builtin().negation_map()
    }

    #[test]
    fn test_negate_simple() {
        assert_eq!(
            negate_command("ip route 10.0.0.0 255.0.0.0 10.0.0.1", &default_map()),
            "no ip route 10.0.0.0 255.0.0.0 10.0.0.1"
        );
    }

    #[test]
    fn test_negate_already_negated() {
        assert_eq!(negate_command("no ip http server", &default_map()), "ip http server");
    }

    #[test]
    fn test_negate_shutdown() {
        let map = default_map();
        assert_eq!(negate_command("shutdown", &map), "no shutdown");
        assert_eq!(negate_command("no shutdown", &map), "shutdown");
    }

    #[test]
    fn test_negate_domain_lookup() {
        let map = default_map();
        assert_eq!(negate_command("no ip domain-lookup", &map), "ip domain-lookup");
        assert_eq!(negate_command("ip domain-lookup", &map), "no ip domain-lookup");
    }
}
