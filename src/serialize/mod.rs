pub mod bounce;
pub mod dependency;
pub mod emitter;
pub mod negation;

use crate::diff::diff_model::DiffTree;
use crate::model::config_tree::ConfigTree;
use crate::rules::RulesConfig;
use bounce::apply_bounce;
use dependency::topological_sort;
use emitter::emit_delta;

/// Serialize a DiffTree into a config delta string, with dependency-aware ordering.
pub fn serialize_delta(diff: &DiffTree, rules: &RulesConfig) -> String {
    let sorted_actions = topological_sort(diff.actions.clone());
    let sorted_diff = DiffTree {
        actions: sorted_actions,
    };
    let negation_map = rules.negation_map();
    emit_delta(&sorted_diff, &negation_map)
}

/// Serialize with bounce-interfaces: physical interfaces with changes get
/// `default interface X` + shutdown + full target config instead of incremental diff.
pub fn serialize_delta_bounce(
    diff: &DiffTree,
    target: &ConfigTree,
    rules: &RulesConfig,
) -> String {
    let negation_map = rules.negation_map();
    let bounced_actions = apply_bounce(diff.actions.clone(), target, &negation_map);
    let sorted_actions = topological_sort(bounced_actions);
    let sorted_diff = DiffTree {
        actions: sorted_actions,
    };
    emit_delta(&sorted_diff, &negation_map)
}
