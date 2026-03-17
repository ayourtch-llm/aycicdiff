use std::collections::HashMap;

use crate::diff::diff_model::*;
use crate::model::config_tree::*;
use crate::model::section_kind::SectionKind;
use crate::rules::RulesConfig;

/// Diff two ConfigTrees and produce a DiffTree.
pub fn diff_trees(current: &ConfigTree, target: &ConfigTree, rules: &RulesConfig) -> DiffTree {
    let actions = diff_node_lists(&current.nodes, &target.nodes, rules);
    DiffTree { actions }
}

/// Diff two lists of ConfigNodes, producing a list of DiffActions.
fn diff_node_lists(current: &[ConfigNode], target: &[ConfigNode], rules: &RulesConfig) -> Vec<DiffAction> {
    let mut actions = Vec::new();

    // Build identity maps: key -> (index, node)
    let current_map = build_identity_map(current, rules);
    let target_map = build_identity_map(target, rules);

    // Removals: in current but not in target
    for node in current {
        let key = node.identity_key(rules);
        if !target_map.contains_key(&key) {
            actions.push(DiffAction::Remove(node.clone()));
        }
    }

    // Additions and modifications: iterate target to preserve target ordering
    for node in target {
        let key = node.identity_key(rules);
        match current_map.get(&key) {
            None => {
                // New node — add it
                actions.push(DiffAction::Add(node.clone()));
            }
            Some(current_node) => {
                // Both exist — check for modifications
                if let Some(action) = diff_matching_nodes(current_node, node, rules) {
                    actions.push(action);
                }
            }
        }
    }

    actions
}

/// Build a map from identity key to ConfigNode reference.
fn build_identity_map<'a>(nodes: &'a [ConfigNode], rules: &RulesConfig) -> HashMap<String, &'a ConfigNode> {
    let mut map = HashMap::new();
    for node in nodes {
        let key = node.identity_key(rules);
        // Last one wins for duplicate keys (shouldn't happen in valid config)
        map.insert(key, node);
    }
    map
}

/// Compare two matching nodes. Returns None if they're identical.
fn diff_matching_nodes(current: &ConfigNode, target: &ConfigNode, rules: &RulesConfig) -> Option<DiffAction> {
    match (current, target) {
        (ConfigNode::Leaf(cl), ConfigNode::Leaf(tl)) => {
            if cl.text != tl.text {
                // Singleton replacement: same key, different text
                return Some(DiffAction::Remove(current.clone()));
            }
            // Identical leaf
            None
        }
        (ConfigNode::Section(cs), ConfigNode::Section(ts)) => {
            match ts.kind {
                SectionKind::OrderedList => {
                    // Compare children as ordered lists
                    let current_leaves: Vec<&ConfigLeaf> =
                        cs.children.iter().filter_map(|n| n.as_leaf()).collect();
                    let target_leaves: Vec<&ConfigLeaf> =
                        ts.children.iter().filter_map(|n| n.as_leaf()).collect();

                    let current_texts: Vec<&str> =
                        current_leaves.iter().map(|l| l.text.as_str()).collect();
                    let target_texts: Vec<&str> =
                        target_leaves.iter().map(|l| l.text.as_str()).collect();

                    if current_texts != target_texts {
                        Some(DiffAction::ReplaceOrdered {
                            header: ts.header.clone(),
                            remove_children: current_leaves.into_iter().cloned().collect(),
                            add_children: target_leaves.into_iter().cloned().collect(),
                        })
                    } else {
                        None
                    }
                }
                SectionKind::Opaque => {
                    // Compare as blobs
                    if cs.children != ts.children {
                        Some(DiffAction::Remove(current.clone()))
                    } else {
                        None
                    }
                }
                SectionKind::Set => {
                    // Recurse on children
                    let child_actions = diff_node_lists(&cs.children, &ts.children, rules);
                    if child_actions.is_empty() {
                        None
                    } else {
                        Some(DiffAction::ModifySection {
                            header: ts.header.clone(),
                            kind: ts.kind,
                            child_actions,
                        })
                    }
                }
            }
        }
        // Leaf becoming section or vice versa — remove old, add new
        _ => Some(DiffAction::Remove(current.clone())),
    }
}

/// Handle singleton leaf replacement: when identity keys match but text differs,
/// we need both a Remove of the old and an Add of the new.
pub fn fixup_singleton_replacements(
    actions: &mut Vec<DiffAction>,
    current: &[ConfigNode],
    target: &[ConfigNode],
    rules: &RulesConfig,
) {
    let current_map = build_identity_map(current, rules);

    for target_node in target {
        if let ConfigNode::Leaf(tl) = target_node {
            let key = target_node.identity_key(rules);
            if rules.singleton_key(&tl.text).is_some() {
                if let Some(ConfigNode::Leaf(cl)) = current_map.get(&key).copied() {
                    if cl.text != tl.text {
                        actions.push(DiffAction::Add(target_node.clone()));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;

    fn parse(input: &str) -> ConfigTree {
        let rules = RulesConfig::builtin();
        parser::parse_config(input, &rules)
    }

    fn diff(current: &str, target: &str) -> DiffTree {
        let rules = RulesConfig::builtin();
        let c = parse(current);
        let t = parse(target);
        diff_trees(&c, &t, &rules)
    }

    #[test]
    fn test_diff_no_changes() {
        let config = "hostname Router1\ninterface Gi0/0\n ip address 10.0.0.1 255.255.255.0\n";
        let d = diff(config, config);
        assert!(d.is_empty());
    }

    #[test]
    fn test_diff_add_only() {
        let d = diff("hostname Router1\n", "hostname Router1\nip route 0.0.0.0 0.0.0.0 10.0.0.1\n");
        assert_eq!(d.actions.len(), 1);
        assert!(matches!(&d.actions[0], DiffAction::Add(_)));
    }

    #[test]
    fn test_diff_remove_only() {
        let d = diff("hostname Router1\nip route 0.0.0.0 0.0.0.0 10.0.0.1\n", "hostname Router1\n");
        assert_eq!(d.actions.len(), 1);
        assert!(matches!(&d.actions[0], DiffAction::Remove(_)));
    }

    #[test]
    fn test_diff_modify_section() {
        let d = diff(
            "interface GigabitEthernet0/0\n ip address 10.0.0.1 255.255.255.0\n shutdown\n",
            "interface GigabitEthernet0/0\n ip address 10.0.0.1 255.255.255.0\n no shutdown\n",
        );
        assert_eq!(d.actions.len(), 1);
        assert!(matches!(&d.actions[0], DiffAction::ModifySection { .. }));
    }

    #[test]
    fn test_diff_ordered_list() {
        let d = diff(
            "ip access-list extended MY_ACL\n permit ip any 10.0.0.0 0.0.0.255\n",
            "ip access-list extended MY_ACL\n permit ip any 10.0.0.0 0.0.0.255\n deny ip any any\n",
        );
        assert_eq!(d.actions.len(), 1);
        assert!(matches!(&d.actions[0], DiffAction::ReplaceOrdered { .. }));
    }
}
