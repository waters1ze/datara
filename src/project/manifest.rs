use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PackageMeta {
    pub name: String,
    pub version: String,
    pub entry: Option<String>,
    pub authors: Option<Vec<String>>,
    pub description: Option<String>,
    pub edition: Option<String>,
    pub license: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DependencyConfig {
    Simple(String),
    Detailed {
        version: Option<String>,
        path: Option<String>,
        git: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TargetConfig {
    pub bin_name: Option<String>,
    pub arch: Option<String>,
    pub os: Option<String>,
    pub opt_level: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProfileConfig {
    pub opt_level: Option<String>,
    pub debug_info: Option<bool>,
    pub pgo: Option<bool>,
    pub lto: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DataraManifest {
    pub package: PackageMeta,
    #[serde(default)]
    pub dependencies: HashMap<String, DependencyConfig>,
    pub target: Option<TargetConfig>,
    #[serde(default)]
    pub profiles: HashMap<String, ProfileConfig>,
}

impl DataraManifest {
    pub fn from_file(path: &Path) -> Result<Self, String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read manifest '{}': {}", path.display(), e))?;
        Self::from_str(&content)
    }

    pub fn from_str(content: &str) -> Result<Self, String> {
        toml::from_str(content).map_err(|e| format!("Invalid datara.toml manifest format: {}", e))
    }

    pub fn default_template(name: &str) -> String {
        format!(
            r#"[package]
name = "{}"
version = "0.1.0"
entry = "src/main.dtr"
edition = "2026"
description = "A high-performance Datara application"

[dependencies]
# core = "0.1.0"

[target]
# bin_name = "{}"
# opt_level = "domain"

[profiles.release]
opt_level = "3"
lto = true

[profiles.domain]
opt_level = "domain"
pgo = true
lto = true
"#,
            name, name
        )
    }
}
