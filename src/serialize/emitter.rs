use std::collections::HashMap;

use crate::diff::diff_model::DiffAction;
use crate::diff::diff_model::DiffTree;
use crate::model::config_tree::ConfigNode;
use crate::serialize::negation::negate_command;

/// Render a DiffTree into the final config delta text suitable for `copy file run`.
pub fn emit_delta(diff: &DiffTree, negation_map: &HashMap<String, String>) -> String {
    let mut output = String::new();
    for action in &diff.actions {
        emit_action(&mut output, action, negation_map);
    }
    output
}

fn emit_action(output: &mut String, action: &DiffAction, neg: &HashMap<String, String>) {
    match action {
        DiffAction::Add(node) => {
            emit_add(output, node, 0);
        }
        DiffAction::Remove(node) => {
            emit_remove(output, node, neg);
        }
        DiffAction::ModifySection {
            header,
            child_actions,
            ..
        } => {
            output.push_str(header);
            output.push('\n');
            // Emit removals first, then additions (within the section)
            for action in child_actions {
                if matches!(action, DiffAction::Remove(_)) {
                    emit_action_indented(output, action, 1, neg);
                }
            }
            for action in child_actions {
                if !matches!(action, DiffAction::Remove(_)) {
                    emit_action_indented(output, action, 1, neg);
                }
            }
            output.push_str("exit\n");
        }
        DiffAction::ReplaceOrdered {
            header,
            remove_children: _,
            add_children,
        } => {
            // Remove entire old list, then re-add with new contents
            let negated_header = negate_command(header, neg);
            output.push_str(&negated_header);
            output.push('\n');

            if !add_children.is_empty() {
                output.push_str(header);
                output.push('\n');
                for child in add_children {
                    output.push_str(&format!(" {}\n", child.text));
                }
                output.push_str("exit\n");
            }
        }
    }
}

fn emit_action_indented(output: &mut String, action: &DiffAction, indent: usize, neg: &HashMap<String, String>) {
    let prefix = " ".repeat(indent);
    match action {
        DiffAction::Add(node) => {
            emit_add(output, node, indent);
        }
        DiffAction::Remove(node) => {
            let negated = negate_command(node.text(), neg);
            output.push_str(&format!("{}{}\n", prefix, negated));
        }
        DiffAction::ModifySection {
            header,
            child_actions,
            ..
        } => {
            output.push_str(&format!("{}{}\n", prefix, header));
            for action in child_actions {
                if matches!(action, DiffAction::Remove(_)) {
                    emit_action_indented(output, action, indent + 1, neg);
                }
            }
            for action in child_actions {
                if !matches!(action, DiffAction::Remove(_)) {
                    emit_action_indented(output, action, indent + 1, neg);
                }
            }
            output.push_str(&format!("{}exit\n", prefix));
        }
        DiffAction::ReplaceOrdered {
            header,
            add_children,
            ..
        } => {
            let negated = negate_command(header, neg);
            output.push_str(&format!("{}{}\n", prefix, negated));
            if !add_children.is_empty() {
                output.push_str(&format!("{}{}\n", prefix, header));
                for child in add_children {
                    output.push_str(&format!("{} {}\n", prefix, child.text));
                }
                output.push_str(&format!("{}exit\n", prefix));
            }
        }
    }
}

/// Emit a node as an addition (new config lines).
fn emit_add(output: &mut String, node: &ConfigNode, indent: usize) {
    let prefix = " ".repeat(indent);
    match node {
        ConfigNode::Leaf(leaf) => {
            output.push_str(&format!("{}{}\n", prefix, leaf.text));
        }
        ConfigNode::Section(section) => {
            output.push_str(&format!("{}{}\n", prefix, section.header));
            for child in &section.children {
                emit_add(output, child, indent + 1);
            }
            output.push_str(&format!("{}exit\n", prefix));
        }
    }
}

/// Emit a node as a removal (negated command).
fn emit_remove(output: &mut String, node: &ConfigNode, neg: &HashMap<String, String>) {
    match node {
        ConfigNode::Leaf(leaf) => {
            let negated = negate_command(&leaf.text, neg);
            output.push_str(&negated);
            output.push('\n');
        }
        ConfigNode::Section(section) => {
            let negated = negate_command(&section.header, neg);
            output.push_str(&negated);
            output.push('\n');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::diff_model::*;
    use crate::model::config_tree::*;
    use crate::model::section_kind::SectionKind;
    use crate::rules::RulesConfig;

    fn neg() -> HashMap<String, String> {
        RulesConfig::builtin().negation_map()
    }

    #[test]
    fn test_emit_add_leaf() {
        let diff = DiffTree {
            actions: vec![DiffAction::Add(ConfigNode::Leaf(ConfigLeaf::new(
                "hostname NewRouter",
            )))],
        };
        assert_eq!(emit_delta(&diff, &neg()), "hostname NewRouter\n");
    }

    #[test]
    fn test_emit_remove_leaf() {
        let diff = DiffTree {
            actions: vec![DiffAction::Remove(ConfigNode::Leaf(ConfigLeaf::new(
                "ip route 10.0.0.0 255.0.0.0 10.0.0.1",
            )))],
        };
        assert_eq!(emit_delta(&diff, &neg()), "no ip route 10.0.0.0 255.0.0.0 10.0.0.1\n");
    }

    #[test]
    fn test_emit_modify_section() {
        let diff = DiffTree {
            actions: vec![DiffAction::ModifySection {
                header: "interface GigabitEthernet0/0".to_string(),
                kind: SectionKind::Set,
                child_actions: vec![
                    DiffAction::Remove(ConfigNode::Leaf(ConfigLeaf::new("shutdown"))),
                    DiffAction::Add(ConfigNode::Leaf(ConfigLeaf::new("no shutdown"))),
                ],
            }],
        };
        let output = emit_delta(&diff, &neg());
        assert_eq!(
            output,
            "interface GigabitEthernet0/0\n no shutdown\n no shutdown\nexit\n"
        );
    }

    #[test]
    fn test_emit_replace_ordered() {
        let diff = DiffTree {
            actions: vec![DiffAction::ReplaceOrdered {
                header: "ip access-list extended MY_ACL".to_string(),
                remove_children: vec![ConfigLeaf::new("permit ip any 10.0.0.0 0.0.0.255")],
                add_children: vec![
                    ConfigLeaf::new("permit ip any 10.0.0.0 0.0.0.255"),
                    ConfigLeaf::new("deny ip any any"),
                ],
            }],
        };
        let output = emit_delta(&diff, &neg());
        assert!(output.contains("no ip access-list extended MY_ACL"));
        assert!(output.contains("ip access-list extended MY_ACL"));
        assert!(output.contains(" deny ip any any"));
    }
}
