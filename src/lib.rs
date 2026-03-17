pub mod diff;
pub mod model;
pub mod parser;
pub mod rules;
pub mod serialize;
pub mod version;

use model::config_tree::ConfigTree;
use model::version_info::VersionInfo;
use rules::RulesConfig;

/// High-level API: generate a config delta from running config to target config.
///
/// The returned string can be applied via `copy file run` to transform the
/// running config into the target config.
pub fn generate_delta(
    running_config: &str,
    target_config: &str,
    show_version: Option<&str>,
) -> String {
    generate_delta_with_rules(running_config, target_config, show_version, &RulesConfig::builtin())
}

/// High-level API with custom rules configuration.
pub fn generate_delta_with_rules(
    running_config: &str,
    target_config: &str,
    show_version: Option<&str>,
    rules: &RulesConfig,
) -> String {
    let version_info = show_version
        .map(version::parser::parse_show_version)
        .unwrap_or_default();

    let mut current = parser::parse_config(running_config, rules);
    let mut target = parser::parse_config(target_config, rules);

    // Filter out version-default commands from both trees so they don't
    // appear as spurious adds or removes in the diff.
    filter_defaults(&mut current, &version_info, rules);
    filter_defaults(&mut target, &version_info, rules);

    let diff = diff::diff_configs(&current, &target, rules);
    serialize::serialize_delta(&diff, rules)
}

/// Remove commands from the tree that are known defaults for this version.
/// This prevents generating unnecessary "no" commands for implicit defaults.
fn filter_defaults(tree: &mut ConfigTree, version: &VersionInfo, rules: &RulesConfig) {
    tree.nodes.retain(|node| {
        if let Some(leaf) = node.as_leaf() {
            !rules.is_default_command(&leaf.text, &version.platform)
        } else {
            true
        }
    });
}
