use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub(crate) struct AppConfig {
    pub(crate) trigger: String,
}

impl AppConfig {
    pub(crate) fn parse(input: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_trigger_from_toml() {
        let config = AppConfig::parse("trigger = \"ctrl+space\"").unwrap();

        assert_eq!(config.trigger, "ctrl+space");
    }
}
