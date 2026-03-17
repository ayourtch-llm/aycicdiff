pub mod dependency;
pub mod emitter;
pub mod negation;

use crate::diff::diff_model::DiffTree;
use crate::rules::RulesConfig;
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
