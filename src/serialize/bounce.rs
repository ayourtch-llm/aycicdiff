use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;

use crate::diff::diff_model::DiffAction;
use crate::model::config_tree::{ConfigNode, ConfigSection, ConfigTree};

/// Regex matching physical interface headers (not Loopback, Tunnel, Vlan, etc.).
static PHYSICAL_INTF_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^interface\s+(GigabitEthernet|TenGigabitEthernet|TwentyFiveGigE|FortyGigabitEthernet|HundredGigE|FastEthernet|Ethernet|Serial|TwoGigabitEthernet|FiveGigabitEthernet|AppGigabitEthernet)\S*"
    ).unwrap()
});

/// Regex matching routing protocol sections that may use passive-interface.
static ROUTING_SECTION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^router\s+(ospf|eigrp|ospfv3)\b").unwrap()
});

/// Check whether an interface header refers to a physical interface.
pub fn is_physical_interface(header: &str) -> bool {
    PHYSICAL_INTF_RE.is_match(header)
}

/// Extract the interface name (everything after "interface ") from a header.
fn interface_name(header: &str) -> &str {
    header
        .strip_prefix("interface ")
        .or_else(|| header.strip_prefix("Interface "))
        .unwrap_or(header)
}

/// Transform diff actions for bounce-interfaces mode.
///
/// For every physical interface with changes, replace the incremental diff with:
///   default interface X
///   interface X
///    shutdown
///    <complete target config>
///   exit
///
/// Then append any `no passive-interface` commands from routing protocol sections
/// in the target config that reference bounced interfaces and have
/// `passive-interface default` configured.
pub fn apply_bounce(
    actions: Vec<DiffAction>,
    target: &ConfigTree,
    negation_map: &HashMap<String, String>,
) -> Vec<DiffAction> {
    let mut bounced_names: Vec<String> = Vec::new();
    let mut result: Vec<DiffAction> = Vec::new();

    // Build a map of interface header -> target section for quick lookup.
    let target_intf_map: HashMap<&str, &ConfigSection> = target
        .nodes
        .iter()
        .filter_map(|n| n.as_section())
        .filter(|s| s.header.to_lowercase().starts_with("interface "))
        .map(|s| (s.header.as_str(), s))
        .collect();

    for action in actions {
        let header = match &action {
            DiffAction::ModifySection { header, .. } => Some(header.clone()),
            DiffAction::Add(ConfigNode::Section(s))
                if s.header.to_lowercase().starts_with("interface ") =>
            {
                Some(s.header.clone())
            }
            DiffAction::Remove(ConfigNode::Section(s))
                if s.header.to_lowercase().starts_with("interface ") =>
            {
                Some(s.header.clone())
            }
            _ => None,
        };

        if let Some(ref hdr) = header {
            if is_physical_interface(hdr) {
                let intf_name = interface_name(hdr);
                bounced_names.push(intf_name.to_string());

                // Emit: default interface X
                result.push(DiffAction::Add(ConfigNode::Leaf(
                    crate::model::config_tree::ConfigLeaf::new(&format!(
                        "default interface {}",
                        intf_name
                    )),
                )));

                // Look up the full target config for this interface.
                // If the interface exists in target, emit shutdown + full config.
                // If it was removed (not in target), just leave it defaulted.
                if let Some(target_section) = target_intf_map.get(hdr.as_str()) {
                    let mut children = vec![ConfigNode::Leaf(
                        crate::model::config_tree::ConfigLeaf::new("shutdown"),
                    )];
                    children.extend(target_section.children.clone());
                    let bounced_section = ConfigNode::Section(ConfigSection {
                        header: target_section.header.clone(),
                        command: target_section.command.clone(),
                        kind: target_section.kind,
                        children,
                    });
                    result.push(DiffAction::Add(bounced_section));
                }

                continue;
            }
        }

        // Non-interface or virtual interface action: pass through unchanged.
        result.push(action);
    }

    // Now handle side effects: find routing protocol sections in target that have
    // `passive-interface default` and `no passive-interface <bounced-intf>`.
    if !bounced_names.is_empty() {
        let fixups = collect_passive_interface_fixups(target, &bounced_names, negation_map);
        result.extend(fixups);
    }

    result
}

/// Scan the target config for routing protocol sections that contain
/// `passive-interface default` and `no passive-interface <interface>` for
/// any of the bounced interfaces. Return DiffActions to re-emit those commands.
fn collect_passive_interface_fixups(
    target: &ConfigTree,
    bounced_names: &[String],
    _negation_map: &HashMap<String, String>,
) -> Vec<DiffAction> {
    let mut fixups: Vec<DiffAction> = Vec::new();

    for node in &target.nodes {
        let section = match node.as_section() {
            Some(s) if ROUTING_SECTION_RE.is_match(&s.header) => s,
            _ => continue,
        };

        // Check if this routing section has `passive-interface default`.
        let has_passive_default = section.children.iter().any(|child| {
            child
                .as_leaf()
                .map(|l| l.text.eq_ignore_ascii_case("passive-interface default"))
                .unwrap_or(false)
        });

        if !has_passive_default {
            continue;
        }

        // Collect `no passive-interface <intf>` commands that match bounced interfaces.
        let mut matching_children: Vec<ConfigNode> = Vec::new();
        for child in &section.children {
            if let Some(leaf) = child.as_leaf() {
                if leaf.text.to_lowercase().starts_with("no passive-interface ") {
                    let intf_part = leaf.text["no passive-interface ".len()..].trim();
                    if bounced_names.iter().any(|bn| bn.eq_ignore_ascii_case(intf_part)) {
                        matching_children.push(child.clone());
                    }
                }
            }
        }

        if !matching_children.is_empty() {
            // Emit a section entry to re-apply these commands.
            fixups.push(DiffAction::Add(ConfigNode::Section(ConfigSection {
                header: section.header.clone(),
                command: section.command.clone(),
                kind: section.kind,
                children: matching_children,
            })));
        }
    }

    fixups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::config_tree::*;
    use crate::model::section_kind::SectionKind;
    use crate::parser;
    use crate::rules::RulesConfig;

    #[test]
    fn test_is_physical_interface() {
        assert!(is_physical_interface("interface GigabitEthernet0/0"));
        assert!(is_physical_interface("interface TenGigabitEthernet1/0/1"));
        assert!(is_physical_interface("interface FastEthernet0/1"));
        assert!(is_physical_interface("interface Serial0/0/0"));
        assert!(!is_physical_interface("interface Loopback0"));
        assert!(!is_physical_interface("interface Vlan100"));
        assert!(!is_physical_interface("interface Tunnel0"));
        assert!(!is_physical_interface("interface Port-channel1"));
    }

    #[test]
    fn test_bounce_replaces_modify_with_default_and_full_config() {
        let rules = RulesConfig::builtin();
        let neg = rules.negation_map();

        let target_config = "\
interface GigabitEthernet0/0
 ip address 10.0.0.1 255.255.255.0
 no shutdown
";
        let target = parser::parse_config(target_config, &rules);

        let actions = vec![DiffAction::ModifySection {
            header: "interface GigabitEthernet0/0".to_string(),
            kind: SectionKind::Set,
            child_actions: vec![DiffAction::Add(ConfigNode::Leaf(ConfigLeaf::new(
                "ip address 10.0.0.1 255.255.255.0",
            )))],
        }];

        let result = apply_bounce(actions, &target, &neg);

        // Should have: default interface + full interface section
        assert_eq!(result.len(), 2);

        // First: default interface command
        match &result[0] {
            DiffAction::Add(ConfigNode::Leaf(l)) => {
                assert_eq!(l.text, "default interface GigabitEthernet0/0");
            }
            other => panic!("Expected Add(Leaf), got {:?}", other),
        }

        // Second: full interface section with shutdown prepended
        match &result[1] {
            DiffAction::Add(ConfigNode::Section(s)) => {
                assert_eq!(s.header, "interface GigabitEthernet0/0");
                assert!(s.children.len() >= 3); // shutdown + ip address + no shutdown
                assert_eq!(s.children[0].as_leaf().unwrap().text, "shutdown");
            }
            other => panic!("Expected Add(Section), got {:?}", other),
        }
    }

    #[test]
    fn test_bounce_preserves_virtual_interfaces() {
        let rules = RulesConfig::builtin();
        let neg = rules.negation_map();

        let target_config = "\
interface Loopback0
 ip address 1.1.1.1 255.255.255.255
";
        let target = parser::parse_config(target_config, &rules);

        let actions = vec![DiffAction::ModifySection {
            header: "interface Loopback0".to_string(),
            kind: SectionKind::Set,
            child_actions: vec![DiffAction::Add(ConfigNode::Leaf(ConfigLeaf::new(
                "ip address 1.1.1.1 255.255.255.255",
            )))],
        }];

        let result = apply_bounce(actions, &target, &neg);

        // Should remain as ModifySection, not bounced
        assert_eq!(result.len(), 1);
        assert!(matches!(&result[0], DiffAction::ModifySection { .. }));
    }

    #[test]
    fn test_bounce_emits_passive_interface_fixups() {
        let rules = RulesConfig::builtin();
        let neg = rules.negation_map();

        let target_config = "\
interface GigabitEthernet0/0
 ip address 10.0.0.1 255.255.255.0
 no shutdown
router ospf 1
 passive-interface default
 no passive-interface GigabitEthernet0/0
";
        let target = parser::parse_config(target_config, &rules);

        let actions = vec![DiffAction::ModifySection {
            header: "interface GigabitEthernet0/0".to_string(),
            kind: SectionKind::Set,
            child_actions: vec![DiffAction::Add(ConfigNode::Leaf(ConfigLeaf::new(
                "no shutdown",
            )))],
        }];

        let result = apply_bounce(actions, &target, &neg);

        // Should have: default interface + full interface section + router ospf fixup
        assert_eq!(result.len(), 3);

        // Third: router ospf section re-emitting no passive-interface
        match &result[2] {
            DiffAction::Add(ConfigNode::Section(s)) => {
                assert!(s.header.starts_with("router ospf"));
                assert_eq!(s.children.len(), 1);
                assert_eq!(
                    s.children[0].as_leaf().unwrap().text,
                    "no passive-interface GigabitEthernet0/0"
                );
            }
            other => panic!("Expected Add(Section) for OSPF fixup, got {:?}", other),
        }
    }
}
