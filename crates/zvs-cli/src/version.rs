pub const VERSION: &str = env!("ZVS_CLI_VERSION");
pub const CHANNEL: &str = env!("ZVS_CLI_CHANNEL");

pub fn label() -> String {
    format!("{CHANNEL} v{VERSION}")
}

pub fn version_line() -> String {
    format!("aistudio {}", label())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_label_carries_the_channel_and_the_version() {
        assert_eq!(label(), format!("{CHANNEL} v{VERSION}"));
        assert!(version_line().starts_with("aistudio "));
    }
}
