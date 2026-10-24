//! Node configuration for domain-scoped static files.

use std::collections::BTreeMap;

use camino::Utf8PathBuf;
use serde::Deserialize;

/// Configuration loaded from a node's TOML file.
#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    /// Filesystem serving policy.
    pub web: WebConfig,
    /// Domain aliases mapped to directory names under the static root.
    #[serde(default)]
    pub domains: BTreeMap<String, String>,
}

impl Config {
    /// Parse a node configuration document.
    ///
    /// # Errors
    /// Returns [`ConfigError`] when TOML syntax or a known field is invalid.
    pub fn from_toml(document: &str) -> Result<Self, ConfigError> {
        toml::from_str(document).map_err(ConfigError)
    }
}

/// The root directory contains a subdirectory for each domain.
#[derive(Clone, Debug, Deserialize)]
pub struct WebConfig {
    /// Directory containing domain roots.
    pub static_dir: Utf8PathBuf,
}

/// A configuration document could not be read as node settings.
#[derive(Debug, thiserror::Error)]
#[error("parse node configuration: {0}")]
pub struct ConfigError(#[source] toml::de::Error);
