pub mod lexer;
pub mod tree_builder;

use crate::model::config_tree::ConfigTree;
use crate::rules::RulesConfig;

/// Parse an IOS configuration string into a ConfigTree.
pub fn parse_config(input: &str, rules: &RulesConfig) -> ConfigTree {
    let tokens = lexer::tokenize(input);
    tree_builder::build_tree(&tokens, rules)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_config_roundtrip() {
        let rules = RulesConfig::builtin();
        let input = "\
hostname Router1
interface GigabitEthernet0/0
 ip address 10.0.0.1 255.255.255.0
 no shutdown
interface GigabitEthernet0/1
 shutdown
";
        let tree = parse_config(input, &rules);
        assert_eq!(tree.nodes.len(), 3);
    }

    #[test]
    fn test_parse_with_preamble() {
        let rules = RulesConfig::builtin();
        let input = "\
Building configuration...

Current configuration : 1234 bytes
!
hostname Router1
!
interface GigabitEthernet0/0
 ip address 10.0.0.1 255.255.255.0
!
end
";
        let tree = parse_config(input, &rules);
        assert_eq!(tree.nodes.len(), 2);
        assert_eq!(tree.nodes[0].text(), "hostname Router1");
        assert_eq!(tree.nodes[1].text(), "interface GigabitEthernet0/0");
    }
}
