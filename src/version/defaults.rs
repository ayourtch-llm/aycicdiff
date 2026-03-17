use crate::model::version_info::{Platform, VersionInfo};

/// Commands that are present by default and should be suppressed from the diff
/// (i.e., if they appear in running config but not in target, don't remove them
/// because they're implicit defaults).
static COMMON_DEFAULTS: &[&str] = &[
    "service timestamps debug datetime msec",
    "service timestamps log datetime msec",
    "service password-encryption",
    "ip cef",
    "no ip domain-lookup",
];

static IOS_XE_DEFAULTS: &[&str] = &[
    "ip http server",
    "ip http secure-server",
    "ip http authentication local",
];

/// Get the set of default commands to suppress based on version info.
pub fn default_commands(version: &VersionInfo) -> Vec<&'static str> {
    let mut defaults: Vec<&str> = COMMON_DEFAULTS.to_vec();

    match version.platform {
        Platform::IosXe => {
            defaults.extend_from_slice(IOS_XE_DEFAULTS);
        }
        Platform::IosClassic | Platform::Unknown => {}
    }

    defaults
}

/// Check if a command text matches a known default that should be suppressed.
pub fn is_default_command(text: &str, version: &VersionInfo) -> bool {
    let trimmed = text.trim();
    let defaults = default_commands(version);
    defaults.iter().any(|&d| trimmed == d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_common_defaults() {
        let version = VersionInfo::default();
        assert!(is_default_command("service timestamps debug datetime msec", &version));
        assert!(!is_default_command("hostname Router1", &version));
    }

    #[test]
    fn test_ios_xe_defaults() {
        let version = VersionInfo {
            platform: Platform::IosXe,
            ..Default::default()
        };
        assert!(is_default_command("ip http server", &version));
    }
}
