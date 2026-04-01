use crate::model::config_tree::{ConfigLeaf, ConfigNode};
use crate::model::section_kind::SectionKind;

/// The result of diffing two config trees.
#[derive(Debug, Clone)]
pub struct DiffTree {
    pub actions: Vec<DiffAction>,
}

/// A single diff action.
#[derive(Debug, Clone)]
pub enum DiffAction {
    /// Add a node that exists in target but not in current.
    Add(ConfigNode),
    /// Remove a node that exists in current but not in target.
    Remove(ConfigNode),
    /// A section exists in both, but its children differ (Set sections).
    ModifySection {
        header: String,
        kind: SectionKind,
        child_actions: Vec<DiffAction>,
    },
    /// An ordered list section differs — remove old entries, add new ones.
    ReplaceOrdered {
        header: String,
        remove_children: Vec<ConfigLeaf>,
        add_children: Vec<ConfigLeaf>,
    },
}

impl DiffAction {
    /// If this is an `Add(Leaf)`, return the leaf text. Useful in tests.
    pub fn as_add_leaf_text(&self) -> Option<&str> {
        match self {
            DiffAction::Add(ConfigNode::Leaf(l)) => Some(&l.text),
            _ => None,
        }
    }
}

impl DiffTree {
    pub fn new() -> Self {
        DiffTree {
            actions: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }
}

impl Default for DiffTree {
    fn default() -> Self {
        Self::new()
    }
}
