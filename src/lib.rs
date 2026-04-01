pub mod diff;
pub mod model;
pub mod parser;
pub mod rules;
pub mod serialize;
pub mod version;

use model::config_tree::ConfigTree;
use model::version_info::VersionInfo;
use rules::RulesConfig;

/// Options controlling delta generation behavior.
#[derive(Debug, Clone, Default)]
pub struct DeltaOptions {
    /// When true, physical interfaces with changes are fully rebuilt:
    /// `default interface X` + shutdown + full target config,
    /// instead of incremental changes.
    pub rebuild_changed_interfaces: bool,
    /// When true, physical interfaces with changes are temporarily bounced:
    /// incremental diff is wrapped in shutdown / no shutdown if the target
    /// state is not shutdown.
    pub bounce_changed_interfaces: bool,
}

/// High-level API: generate a config delta from running config to target config.
///
/// The returned string can be applied via `copy file run` to transform the
/// running config into the target config.
pub fn generate_delta(
    running_config: &str,
    target_config: &str,
    show_version: Option<&str>,
) -> String {
    generate_delta_with_rules(
        running_config,
        target_config,
        show_version,
        &RulesConfig::builtin(),
        &DeltaOptions::default(),
    )
}

/// High-level API with custom rules configuration.
pub fn generate_delta_with_rules(
    running_config: &str,
    target_config: &str,
    show_version: Option<&str>,
    rules: &RulesConfig,
    options: &DeltaOptions,
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

    if options.rebuild_changed_interfaces {
        serialize::serialize_delta_rebuild(&diff, &target, rules)
    } else if options.bounce_changed_interfaces {
        serialize::serialize_delta_bounce_changed(&diff, &target, rules)
    } else {
        serialize::serialize_delta(&diff, rules)
    }
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
