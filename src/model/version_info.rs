/// Platform type detected from show version output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Platform {
    IosClassic,
    IosXe,
    Unknown,
}

/// Parsed "show version" information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionInfo {
    pub platform: Platform,
    pub major_version: u32,
    pub minor_version: u32,
    pub train: String,
    pub image: String,
    pub model: String,
}

impl Default for VersionInfo {
    fn default() -> Self {
        VersionInfo {
            platform: Platform::Unknown,
            major_version: 0,
            minor_version: 0,
            train: String::new(),
            image: String::new(),
            model: String::new(),
        }
    }
}
