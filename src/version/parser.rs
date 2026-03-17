use regex::Regex;
use std::sync::LazyLock;

use crate::model::version_info::{Platform, VersionInfo};

static VERSION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)Version\s+(\d+)\.(\d+)").unwrap()
});

static IOS_XE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)IOS-XE|IOSXE|Cisco IOS XE").unwrap()
});

static TRAIN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)Version\s+([\d.]+\w*)").unwrap()
});

static MODEL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?im)^cisco\s+(\S+)\s.*processor\b").unwrap()
});

static IMAGE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)System image file is "([^"]+)""#).unwrap()
});

/// Parse a "show version" output into a VersionInfo struct.
pub fn parse_show_version(input: &str) -> VersionInfo {
    let mut info = VersionInfo::default();

    // Detect platform
    if IOS_XE_RE.is_match(input) {
        info.platform = Platform::IosXe;
    } else if input.contains("Cisco IOS Software") || input.contains("IOS (tm)") {
        info.platform = Platform::IosClassic;
    }

    // Extract version numbers
    if let Some(caps) = VERSION_RE.captures(input) {
        info.major_version = caps[1].parse().unwrap_or(0);
        info.minor_version = caps[2].parse().unwrap_or(0);
    }

    // Extract train string
    if let Some(caps) = TRAIN_RE.captures(input) {
        info.train = caps[1].to_string();
    }

    // Extract model
    if let Some(caps) = MODEL_RE.captures(input) {
        info.model = caps[1].to_string();
    }

    // Extract image
    if let Some(caps) = IMAGE_RE.captures(input) {
        info.image = caps[1].to_string();
    }

    info
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ios_xe() {
        let input = r#"
Cisco IOS XE Software, Version 17.06.03a
Cisco IOS Software [Bengaluru], Catalyst L3 Switch Software (CAT9K_IOSXE), Version 17.6.3a, RELEASE SOFTWARE
Technical Support: http://www.cisco.com/techsupport
cisco C9300-48P (X86) processor with 1419044K/6147K bytes of memory.
System image file is "flash:packages.conf"
"#;
        let info = parse_show_version(input);
        assert_eq!(info.platform, Platform::IosXe);
        assert_eq!(info.major_version, 17);
        assert_eq!(info.minor_version, 06);
        assert_eq!(info.train, "17.06.03a");
        assert!(info.model.contains("C9300"));
    }

    #[test]
    fn test_parse_ios_classic() {
        let input = r#"
Cisco IOS Software, ISR Software (X86_64_LINUX_IOSD-UNIVERSALK9-M), Version 15.5(3)S5, RELEASE SOFTWARE
cisco ISR4321/K9 (1RU) processor with 1795979K/6147K bytes of memory.
System image file is "bootflash:isr4300-universalk9.SPA.155-3.S5-ext.bin"
"#;
        let info = parse_show_version(input);
        assert_eq!(info.platform, Platform::IosClassic);
        assert_eq!(info.major_version, 15);
        assert_eq!(info.minor_version, 5);
    }
}
