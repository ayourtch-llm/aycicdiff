use crate::model::version_info::{Platform, VersionInfo};

/// Version-specific syntax quirks for command generation.
pub struct Quirks {
    /// Whether to use "vrf forwarding" (IOS-XE) vs "ip vrf forwarding" (IOS classic)
    pub use_vrf_forwarding: bool,
    /// Whether to use "ip domain name" (IOS-XE) vs "ip domain-name" (IOS classic)
    pub use_ip_domain_name: bool,
}

impl Quirks {
    pub fn for_version(version: &VersionInfo) -> Self {
        match version.platform {
            Platform::IosXe => Quirks {
                use_vrf_forwarding: true,
                use_ip_domain_name: true,
            },
            Platform::IosClassic => Quirks {
                use_vrf_forwarding: false,
                use_ip_domain_name: false,
            },
            Platform::Unknown => Quirks {
                use_vrf_forwarding: false,
                use_ip_domain_name: false,
            },
        }
    }
}

/// Normalize a command for a specific platform version.
pub fn normalize_command(text: &str, version: &VersionInfo) -> String {
    let quirks = Quirks::for_version(version);
    let mut result = text.to_string();

    // Normalize VRF command syntax
    if quirks.use_vrf_forwarding {
        if result.contains("ip vrf forwarding") {
            result = result.replace("ip vrf forwarding", "vrf forwarding");
        }
    } else if result.contains("vrf forwarding") && !result.contains("ip vrf forwarding") {
        result = result.replace("vrf forwarding", "ip vrf forwarding");
    }

    // Normalize domain name syntax
    if quirks.use_ip_domain_name {
        if result.contains("ip domain-name") {
            result = result.replace("ip domain-name", "ip domain name");
        }
    } else if result.contains("ip domain name") {
        result = result.replace("ip domain name", "ip domain-name");
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_vrf_ios_xe() {
        let version = VersionInfo {
            platform: Platform::IosXe,
            ..Default::default()
        };
        assert_eq!(
            normalize_command("ip vrf forwarding MGMT", &version),
            "vrf forwarding MGMT"
        );
    }

    #[test]
    fn test_normalize_domain_ios_classic() {
        let version = VersionInfo {
            platform: Platform::IosClassic,
            ..Default::default()
        };
        assert_eq!(
            normalize_command("ip domain name example.com", &version),
            "ip domain-name example.com"
        );
    }
}
