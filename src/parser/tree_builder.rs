use crate::model::command::Command;
use crate::model::config_tree::*;
use crate::parser::lexer::Token;
use crate::rules::RulesConfig;

/// Build a ConfigTree from a sequence of tokens using an indent-stack algorithm.
pub fn build_tree(tokens: &[Token], rules: &RulesConfig) -> ConfigTree {
    let mut tree = ConfigTree::new();
    let mut i = 0;

    while i < tokens.len() {
        let (node, next_i) = parse_node(tokens, i, rules);
        tree.nodes.push(node);
        i = next_i;
    }

    tree
}

/// Parse a single node (leaf or section) starting at index `i`.
/// Returns the parsed node and the next index to process.
fn parse_node(tokens: &[Token], i: usize, rules: &RulesConfig) -> (ConfigNode, usize) {
    let token = &tokens[i];

    // Look ahead: if next token has greater indent, this is a section header
    let is_section = if i + 1 < tokens.len() {
        tokens[i + 1].indent > token.indent
    } else {
        false
    };

    if is_section {
        let kind = rules.classify_section(&token.text);
        let mut section = ConfigSection::new(&token.text, kind);
        let section_indent = token.indent;
        let mut j = i + 1;

        // Collect all children (tokens with indent > section_indent)
        while j < tokens.len() && tokens[j].indent > section_indent {
            let (child, next_j) = parse_node(tokens, j, rules);
            section.children.push(child);
            j = next_j;
        }

        (ConfigNode::Section(section), j)
    } else {
        let leaf = ConfigLeaf {
            text: token.text.clone(),
            command: Command::parse(&token.text),
        };
        (ConfigNode::Leaf(leaf), i + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::lexer::tokenize;

    #[test]
    fn test_build_simple_tree() {
        let rules = RulesConfig::builtin();
        let input = "\
hostname Router1
interface GigabitEthernet0/0
 ip address 10.0.0.1 255.255.255.0
 no shutdown
interface GigabitEthernet0/1
 shutdown
";
        let tokens = tokenize(input);
        let tree = build_tree(&tokens, &rules);

        assert_eq!(tree.nodes.len(), 3);
        assert!(tree.nodes[0].as_leaf().is_some());
        assert_eq!(tree.nodes[0].as_leaf().unwrap().text, "hostname Router1");

        let sec = tree.nodes[1].as_section().unwrap();
        assert_eq!(sec.header, "interface GigabitEthernet0/0");
        assert_eq!(sec.children.len(), 2);

        let sec2 = tree.nodes[2].as_section().unwrap();
        assert_eq!(sec2.header, "interface GigabitEthernet0/1");
        assert_eq!(sec2.children.len(), 1);
    }

    #[test]
    fn test_nested_sections() {
        let rules = RulesConfig::builtin();
        let input = "\
router bgp 65000
 address-family ipv4 unicast
  network 10.0.0.0 mask 255.255.255.0
  neighbor 10.0.0.2 activate
 exit-address-family
";
        let tokens = tokenize(input);
        let tree = build_tree(&tokens, &rules);

        assert_eq!(tree.nodes.len(), 1);
        let bgp = tree.nodes[0].as_section().unwrap();
        assert_eq!(bgp.header, "router bgp 65000");
        assert_eq!(bgp.children.len(), 2);

        let af = bgp.children[0].as_section().unwrap();
        assert_eq!(af.header, "address-family ipv4 unicast");
        assert_eq!(af.children.len(), 2);
    }
}
