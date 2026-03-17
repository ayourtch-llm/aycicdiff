use std::collections::{HashMap, HashSet, VecDeque};

use regex::Regex;
use std::sync::LazyLock;

use crate::diff::diff_model::DiffAction;
use crate::model::config_tree::ConfigNode;

/// A resource identifier (type:name) that can be provided or required.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Resource {
    pub kind: String,
    pub name: String,
}

impl Resource {
    pub fn new(kind: &str, name: &str) -> Self {
        Resource {
            kind: kind.to_string(),
            name: name.to_string(),
        }
    }
}

struct DependencyPattern {
    provides: Vec<(Regex, String)>, // (pattern, resource_kind)
    requires: Vec<(Regex, String, usize)>, // (pattern, resource_kind, capture_group_for_name)
}

static DEP_PATTERNS: LazyLock<DependencyPattern> = LazyLock::new(|| {
    DependencyPattern {
        provides: vec![
            (Regex::new(r"(?i)^route-map\s+(\S+)").unwrap(), "route-map".to_string()),
            (Regex::new(r"(?i)^ip prefix-list\s+(\S+)").unwrap(), "prefix-list".to_string()),
            (Regex::new(r"(?i)^ip access-list\s+\S+\s+(\S+)").unwrap(), "acl".to_string()),
            (Regex::new(r"(?i)^vrf definition\s+(\S+)").unwrap(), "vrf".to_string()),
            (Regex::new(r"(?i)^ip vrf\s+(\S+)").unwrap(), "vrf".to_string()),
            (Regex::new(r"(?i)^policy-map\s+(\S+)").unwrap(), "policy-map".to_string()),
            (Regex::new(r"(?i)^class-map\s+\S+\s+(\S+)").unwrap(), "class-map".to_string()),
        ],
        requires: vec![
            (Regex::new(r"(?i)neighbor\s+\S+\s+route-map\s+(\S+)").unwrap(), "route-map".to_string(), 1),
            (Regex::new(r"(?i)match\s+ip\s+address\s+prefix-list\s+(\S+)").unwrap(), "prefix-list".to_string(), 1),
            (Regex::new(r"(?i)match\s+ip\s+address\s+(\S+)").unwrap(), "acl".to_string(), 1),
            (Regex::new(r"(?i)ip\s+vrf\s+forwarding\s+(\S+)").unwrap(), "vrf".to_string(), 1),
            (Regex::new(r"(?i)vrf\s+forwarding\s+(\S+)").unwrap(), "vrf".to_string(), 1),
            (Regex::new(r"(?i)service-policy\s+\S+\s+(\S+)").unwrap(), "policy-map".to_string(), 1),
        ],
    }
});

/// Extract resources provided by a config node (from its header/text).
fn extract_provides(text: &str) -> Vec<Resource> {
    let mut resources = Vec::new();
    for (pattern, kind) in &DEP_PATTERNS.provides {
        if let Some(caps) = pattern.captures(text) {
            if let Some(name) = caps.get(1) {
                resources.push(Resource::new(kind, name.as_str()));
            }
        }
    }
    resources
}

/// Extract resources required by a node (checking its text and children).
fn extract_requires_from_node(node: &ConfigNode) -> Vec<Resource> {
    let mut resources = Vec::new();
    let text = node.text();

    for (pattern, kind, group) in &DEP_PATTERNS.requires {
        if let Some(caps) = pattern.captures(text) {
            if let Some(name) = caps.get(*group) {
                resources.push(Resource::new(kind, name.as_str()));
            }
        }
    }

    // Recurse into section children
    if let Some(section) = node.as_section() {
        for child in &section.children {
            resources.extend(extract_requires_from_node(child));
        }
    }

    resources
}

/// Extract resources required by a DiffAction.
fn extract_requires(action: &DiffAction) -> Vec<Resource> {
    match action {
        DiffAction::Add(node) => extract_requires_from_node(node),
        DiffAction::Remove(node) => {
            // For removals, we "provide" the removal (freeing the resource)
            // but we need to check if removing this breaks any dependency
            extract_provides(node.text())
        }
        DiffAction::ModifySection {
            child_actions, ..
        } => {
            let mut resources = Vec::new();
            for ca in child_actions {
                resources.extend(extract_requires(ca));
            }
            resources
        }
        DiffAction::ReplaceOrdered { .. } => Vec::new(),
    }
}

/// Extract resources provided by a DiffAction.
fn action_provides(action: &DiffAction) -> Vec<Resource> {
    match action {
        DiffAction::Add(node) => extract_provides(node.text()),
        DiffAction::Remove(_) => Vec::new(),
        DiffAction::ModifySection { .. } => Vec::new(),
        DiffAction::ReplaceOrdered { header, .. } => extract_provides(header),
    }
}

/// Sort diff actions respecting dependencies using Kahn's algorithm.
/// For Add actions: if A provides X and B requires X → A before B.
/// For Remove actions: remove references before definitions.
pub fn topological_sort(actions: Vec<DiffAction>) -> Vec<DiffAction> {
    let n = actions.len();
    if n <= 1 {
        return actions;
    }

    // Build provides/requires maps
    let mut provides_map: HashMap<Resource, Vec<usize>> = HashMap::new();
    let mut requires_map: Vec<Vec<Resource>> = Vec::with_capacity(n);

    for (i, action) in actions.iter().enumerate() {
        let provides = action_provides(action);
        for r in &provides {
            provides_map.entry(r.clone()).or_default().push(i);
        }
        requires_map.push(extract_requires(action));
    }

    // Build adjacency list and in-degree count
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut in_degree: Vec<usize> = vec![0; n];

    for (i, requires) in requires_map.iter().enumerate() {
        for req in requires {
            if let Some(providers) = provides_map.get(req) {
                for &j in providers {
                    if i != j {
                        // For Add actions: provider must come before requirer
                        if matches!(&actions[i], DiffAction::Add(_))
                            && matches!(&actions[j], DiffAction::Add(_))
                        {
                            // j provides, i requires → j before i
                            adj[j].push(i);
                            in_degree[i] += 1;
                        }
                        // For Remove actions: requirer (reference) removed before provider (definition)
                        if matches!(&actions[i], DiffAction::Remove(_))
                            && matches!(&actions[j], DiffAction::Remove(_))
                        {
                            // i removes a reference to what j provides → i before j
                            adj[i].push(j);
                            in_degree[j] += 1;
                        }
                    }
                }
            }
        }
    }

    // Kahn's algorithm
    let mut queue: VecDeque<usize> = VecDeque::new();
    for i in 0..n {
        if in_degree[i] == 0 {
            queue.push_back(i);
        }
    }

    let mut sorted_indices = Vec::with_capacity(n);
    while let Some(node) = queue.pop_front() {
        sorted_indices.push(node);
        for &neighbor in &adj[node] {
            in_degree[neighbor] -= 1;
            if in_degree[neighbor] == 0 {
                queue.push_back(neighbor);
            }
        }
    }

    // If there's a cycle, just append remaining in original order
    if sorted_indices.len() < n {
        let in_sorted: HashSet<usize> = sorted_indices.iter().copied().collect();
        for i in 0..n {
            if !in_sorted.contains(&i) {
                sorted_indices.push(i);
            }
        }
    }

    // Reconstruct sorted actions
    let mut actions_arr: Vec<Option<DiffAction>> = actions.into_iter().map(Some).collect();
    sorted_indices
        .into_iter()
        .map(|i| actions_arr[i].take().unwrap())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::config_tree::*;
    use crate::model::section_kind::SectionKind;

    #[test]
    fn test_extract_provides() {
        let resources = extract_provides("route-map MY_MAP permit 10");
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].kind, "route-map");
        assert_eq!(resources[0].name, "MY_MAP");
    }

    #[test]
    fn test_topological_sort_route_map_before_reference() {
        // Action 0: Add neighbor ... route-map MY_MAP (requires route-map:MY_MAP)
        let bgp_section = ConfigNode::Section(ConfigSection {
            header: "router bgp 65000".to_string(),
            command: crate::model::command::Command::parse("router bgp 65000"),
            kind: SectionKind::Set,
            children: vec![ConfigNode::Leaf(ConfigLeaf::new(
                "neighbor 10.0.0.1 route-map MY_MAP in",
            ))],
        });

        // Action 1: Add route-map MY_MAP (provides route-map:MY_MAP)
        let route_map = ConfigNode::Section(ConfigSection {
            header: "route-map MY_MAP permit 10".to_string(),
            command: crate::model::command::Command::parse("route-map MY_MAP permit 10"),
            kind: SectionKind::Set,
            children: vec![ConfigNode::Leaf(ConfigLeaf::new("set local-preference 200"))],
        });

        let actions = vec![
            DiffAction::Add(bgp_section),
            DiffAction::Add(route_map),
        ];

        let sorted = topological_sort(actions);

        // route-map should come before the bgp section that references it
        let route_map_idx = sorted
            .iter()
            .position(|a| matches!(a, DiffAction::Add(ConfigNode::Section(s)) if s.header.starts_with("route-map")))
            .unwrap();
        let bgp_idx = sorted
            .iter()
            .position(|a| matches!(a, DiffAction::Add(ConfigNode::Section(s)) if s.header.starts_with("router bgp")))
            .unwrap();

        assert!(route_map_idx < bgp_idx, "route-map should be created before it's referenced in BGP");
    }
}
