pub mod diff_model;
pub mod tree_diff;

use crate::model::config_tree::ConfigTree;
use crate::rules::RulesConfig;
use diff_model::DiffTree;
use tree_diff::{diff_trees, fixup_singleton_replacements};

/// Compare two configs and produce a DiffTree describing the changes.
pub fn diff_configs(current: &ConfigTree, target: &ConfigTree, rules: &RulesConfig) -> DiffTree {
    let mut diff = diff_trees(current, target, rules);
    fixup_singleton_replacements(&mut diff.actions, &current.nodes, &target.nodes, rules);
    diff
}
