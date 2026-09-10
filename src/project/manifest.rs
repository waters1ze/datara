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

use crate::rust_bridge::config::RustCrateConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DependencyConfig {
    Simple(String),
    Detailed {
        version: Option<String>,
        path: Option<String>,
        git: Option<String>,
    },
    RustGroup(HashMap<String, RustCrateConfig>),
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
    #[serde(default, rename = "rust_dependencies")]
    pub explicit_rust_deps: HashMap<String, RustCrateConfig>,
    pub target: Option<TargetConfig>,
    #[serde(default)]
    pub profiles: HashMap<String, ProfileConfig>,
}

pub fn validate_semver(version: &str) -> Result<(), String> {
    if version.is_empty() {
        return Err("Package version cannot be empty".to_string());
    }

    // Split build metadata
    let (core_and_pre, _build) = match version.split_once('+') {
        Some((c, b)) => {
            if b.is_empty()
                || !b.split('.').all(|id| {
                    !id.is_empty() && id.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
                })
            {
                return Err(format!(
                    "Invalid semver build metadata '+{}' in '{}'",
                    b, version
                ));
            }
            (c, Some(b))
        }
        None => (version, None),
    };

    // Split prerelease
    let (core, _prerelease) = match core_and_pre.split_once('-') {
        Some((c, p)) => {
            if p.is_empty() {
                return Err(format!("Invalid empty semver prerelease in '{}'", version));
            }
            for id in p.split('.') {
                if id.is_empty() || !id.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-') {
                    return Err(format!(
                        "Invalid semver prerelease identifier '{}' in '{}'",
                        id, version
                    ));
                }
                if id.chars().all(|ch| ch.is_ascii_digit()) && id.len() > 1 && id.starts_with('0') {
                    return Err(format!(
                        "Leading zero in numeric semver prerelease '{}' in '{}'",
                        id, version
                    ));
                }
            }
            (c, Some(p))
        }
        None => (core_and_pre, None),
    };

    let parts: Vec<&str> = core.split('.').collect();
    if parts.len() != 3 {
        return Err(format!(
            "Package version '{}' is not valid semver: expected MAJOR.MINOR.PATCH (e.g. '0.1.0')",
            version
        ));
    }

    for (part_name, part) in [
        ("major", parts[0]),
        ("minor", parts[1]),
        ("patch", parts[2]),
    ] {
        if part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()) {
            return Err(format!(
                "Invalid semver {} component '{}' in '{}' (must be a non-negative integer)",
                part_name, part, version
            ));
        }
        if part.len() > 1 && part.starts_with('0') {
            return Err(format!(
                "Invalid semver {} component '{}' in '{}': leading zeroes are forbidden",
                part_name, part, version
            ));
        }
    }

    Ok(())
}

pub fn validate_edition(edition: &str) -> Result<(), String> {
    match edition {
        "2024" | "2025" | "2026" => Ok(()),
        other => Err(format!(
            "Unsupported edition '{}'. Supported editions: '2024', '2025', '2026'",
            other
        )),
    }
}

impl DataraManifest {
    pub fn from_file(path: &Path) -> Result<Self, String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read manifest '{}': {}", path.display(), e))?;
        Self::parse(&content)
    }

    pub fn parse(content: &str) -> Result<Self, String> {
        let mut manifest: Self = toml::from_str(content)
            .map_err(|e| format!("Invalid datara.toml manifest format: {}", e))?;

        // Enforce semver validation on package version
        validate_semver(&manifest.package.version)?;

        // Enforce edition validation
        if let Some(ref ed) = manifest.package.edition {
            validate_edition(ed)?;
        } else {
            manifest.package.edition = Some("2026".to_string());
        }

        if let Ok(value) = toml::from_str::<toml::Value>(content) {
            if let Some(deps) = value.get("dependencies").and_then(|d| d.as_table()) {
                if let Some(rust_val) = deps.get("rust") {
                    if let Ok(rust_group) = rust_val
                        .clone()
                        .try_into::<HashMap<String, RustCrateConfig>>()
                    {
                        manifest.explicit_rust_deps.extend(rust_group);
                    }
                }
            }
            if let Some(rust_val) = value.get("rust_dependencies") {
                if let Ok(rust_group) = rust_val
                    .clone()
                    .try_into::<HashMap<String, RustCrateConfig>>()
                {
                    manifest.explicit_rust_deps.extend(rust_group);
                }
            }
        }

        Ok(manifest)
    }

    /// Extract all Rust crate dependencies from [dependencies.rust] or [rust_dependencies].
    pub fn rust_dependencies(&self) -> HashMap<String, RustCrateConfig> {
        let mut rust_deps = self.explicit_rust_deps.clone();
        if let Some(DependencyConfig::RustGroup(group)) = self.dependencies.get("rust") {
            for (k, v) in group {
                rust_deps.insert(k.clone(), v.clone());
            }
        }
        rust_deps
    }

    pub fn default_template(name: &str) -> String {
        format!(
            r#"[package]
name = "{}"
version = "1.0.0"
entry = "src/main.dtr"
edition = "2026"
description = "A high-performance Datara application"

[dependencies]
# core = "1.0.0"

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
