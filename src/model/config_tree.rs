use crate::model::command::Command;
use crate::model::section_kind::SectionKind;

/// A parsed IOS configuration represented as a tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigTree {
    pub nodes: Vec<ConfigNode>,
}

/// A node in the config tree — either a leaf command or a section with children.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigNode {
    Leaf(ConfigLeaf),
    Section(ConfigSection),
}

/// A single-line config command (no children).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigLeaf {
    pub text: String,
    pub command: Command,
}

/// A section with a header line and indented children.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigSection {
    pub header: String,
    pub command: Command,
    pub kind: SectionKind,
    pub children: Vec<ConfigNode>,
}

impl ConfigTree {
    pub fn new() -> Self {
        ConfigTree { nodes: Vec::new() }
    }

    /// Serialize the config tree back to config text.
    pub fn to_config_text(&self) -> String {
        let mut output = String::new();
        for node in &self.nodes {
            node.write_to(&mut output, 0);
        }
        output
    }
}

impl Default for ConfigTree {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigNode {
    fn write_to(&self, output: &mut String, indent: usize) {
        let prefix = " ".repeat(indent);
        match self {
            ConfigNode::Leaf(leaf) => {
                output.push_str(&format!("{}{}\n", prefix, leaf.text));
            }
            ConfigNode::Section(section) => {
                output.push_str(&format!("{}{}\n", prefix, section.header));
                for child in &section.children {
                    child.write_to(output, indent + 1);
                }
            }
        }
    }

    /// Get the identity key for this node (used for matching nodes between trees).
    pub fn identity_key(&self, rules: &crate::rules::RulesConfig) -> String {
        match self {
            ConfigNode::Leaf(leaf) => {
                if let Some(key) = rules.singleton_key(&leaf.text) {
                    key.to_string()
                } else {
                    leaf.text.clone()
                }
            }
            ConfigNode::Section(section) => section.header.clone(),
        }
    }

    pub fn as_leaf(&self) -> Option<&ConfigLeaf> {
        match self {
            ConfigNode::Leaf(l) => Some(l),
            _ => None,
        }
    }

    pub fn as_section(&self) -> Option<&ConfigSection> {
        match self {
            ConfigNode::Section(s) => Some(s),
            _ => None,
        }
    }

    pub fn text(&self) -> &str {
        match self {
            ConfigNode::Leaf(l) => &l.text,
            ConfigNode::Section(s) => &s.header,
        }
    }
}

impl ConfigLeaf {
    pub fn new(text: &str) -> Self {
        ConfigLeaf {
            text: text.to_string(),
            command: Command::parse(text),
        }
    }
}

impl ConfigSection {
    pub fn new(header: &str, kind: SectionKind) -> Self {
        ConfigSection {
            header: header.to_string(),
            command: Command::parse(header),
            kind,
            children: Vec::new(),
        }
    }
}
