use std::fmt;
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::config::RegistryConfig;

pub const DEFAULT_REGISTRY_NAME: &str = "styrene-armory";
pub const DEFAULT_REGISTRY_URL: &str = "https://armory.styrene.io/api/index.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageRef {
    pub kind: String,
    pub id: String,
}

impl PackageRef {
    pub fn parse(value: &str) -> Result<Self> {
        let Some((kind, id)) = value.split_once('/') else {
            bail!("package ref must be <kind>/<id>, got '{value}'");
        };
        if kind.is_empty() || id.is_empty() || id.contains('/') {
            bail!("package ref must be <kind>/<id>, got '{value}'");
        }
        Ok(Self {
            kind: kind.to_string(),
            id: id.to_string(),
        })
    }
}

impl fmt::Display for PackageRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.kind, self.id)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArmoryIndex {
    #[serde(default)]
    pub packages: Vec<ArmoryPackage>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArmoryPackage {
    pub package_ref: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub install_command: Option<String>,
    #[serde(default)]
    pub fallback_install_command: Option<String>,
    #[serde(default)]
    pub dependencies: Vec<ArmoryDependency>,
    #[serde(default)]
    pub optional_dependencies: Vec<ArmoryDependency>,
    #[serde(default)]
    pub activation: Option<ArmoryActivation>,
    #[serde(default)]
    pub oci_ref: Option<String>,
    #[serde(default)]
    pub digest: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArmoryDependency {
    #[serde(default)]
    pub package_ref: Option<String>,
    #[serde(default)]
    pub r#ref: Option<String>,
    #[serde(default)]
    pub optional: Option<bool>,
}

impl ArmoryDependency {
    pub fn display_ref(&self) -> Option<&str> {
        self.package_ref.as_deref().or(self.r#ref.as_deref())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArmoryActivation {
    #[serde(default)]
    pub runtime: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
}

pub fn default_registry() -> RegistryConfig {
    RegistryConfig {
        name: DEFAULT_REGISTRY_NAME.to_string(),
        url: DEFAULT_REGISTRY_URL.to_string(),
        trust: Some("signed".to_string()),
    }
}

pub fn fetch_index(registry: &RegistryConfig) -> Result<ArmoryIndex> {
    let output = Command::new("curl")
        .args(["-fsSL", &registry.url])
        .output()
        .with_context(|| format!("fetching Armory registry {}", registry.url))?;
    if !output.status.success() {
        bail!(
            "failed to fetch Armory registry {}: {}",
            registry.url,
            crate::exec::captured_text(&output.stderr).trim()
        );
    }
    parse_index(&output.stdout)
}

pub fn parse_index(bytes: &[u8]) -> Result<ArmoryIndex> {
    serde_json::from_slice(bytes).context("parsing Armory index JSON")
}

pub fn search(index: &ArmoryIndex, query: &str) -> Vec<ArmoryPackage> {
    let query = query.to_ascii_lowercase();
    index
        .packages
        .iter()
        .filter(|package| {
            package.package_ref.to_ascii_lowercase().contains(&query)
                || package
                    .name
                    .as_deref()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .contains(&query)
                || package
                    .description
                    .as_deref()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .contains(&query)
        })
        .cloned()
        .collect()
}

pub fn find<'a>(index: &'a ArmoryIndex, package_ref: &PackageRef) -> Option<&'a ArmoryPackage> {
    let needle = package_ref.to_string();
    index
        .packages
        .iter()
        .find(|package| package.package_ref == needle)
}

pub fn print_search_results(registry: &RegistryConfig, results: &[ArmoryPackage]) {
    if results.is_empty() {
        return;
    }
    println!("\nArmory registry: {}", registry.name);
    for package in results {
        let version = package.version.as_deref().unwrap_or("unknown");
        let description = package.description.as_deref().unwrap_or("");
        println!(
            "  {:<40} {:<12} {}",
            package.package_ref, version, description
        );
    }
}

pub fn print_info(registry: &RegistryConfig, package: &ArmoryPackage) {
    println!("{}", package.package_ref);
    println!("  registry: {}", registry.name);
    if let Some(name) = &package.name {
        println!("  name: {name}");
    }
    if let Some(version) = &package.version {
        println!("  version: {version}");
    }
    if let Some(description) = &package.description {
        println!("  description: {description}");
    }
    if let Some(command) = &package.install_command {
        println!("  install: {command}");
    }
    if let Some(command) = &package.fallback_install_command {
        println!("  fallback: {command}");
    }
    if let Some(oci_ref) = &package.oci_ref {
        println!("  oci: {oci_ref}");
    }
    if let Some(digest) = &package.digest {
        println!("  digest: {digest}");
    }
    if !package.dependencies.is_empty() {
        println!("  dependencies:");
        for dep in &package.dependencies {
            if let Some(dep_ref) = dep.display_ref() {
                let suffix = if dep.optional.unwrap_or(false) {
                    " (optional)"
                } else {
                    ""
                };
                println!("    - {dep_ref}{suffix}");
            }
        }
    }
    if !package.optional_dependencies.is_empty() {
        println!("  optional dependencies:");
        for dep in &package.optional_dependencies {
            if let Some(dep_ref) = dep.display_ref() {
                let suffix = if dep.optional.unwrap_or(true) {
                    " (optional)"
                } else {
                    ""
                };
                println!("    - {dep_ref}{suffix}");
            }
        }
    }
    if let Some(activation) = &package.activation {
        println!("  activation:");
        if let Some(runtime) = &activation.runtime {
            println!("    runtime: {runtime}");
        }
        if let Some(mode) = &activation.mode {
            println!("    mode: {mode}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{find, parse_index, search, PackageRef};

    const INDEX: &[u8] = br#"{
      "packages": [{
        "packageRef": "profile/rust-shop",
        "name": "Rust Shop",
        "version": "1.0.0",
        "description": "Rust development profile",
        "installCommand": "nex install profile/rust-shop",
        "activation": { "runtime": "omegon", "mode": "profile" },
        "dependencies": [{ "packageRef": "skill/rust" }]
      }]
    }"#;

    #[test]
    fn parses_package_ref() {
        let package_ref = PackageRef::parse("profile/rust-shop").expect("package ref");
        assert_eq!(package_ref.kind, "profile");
        assert_eq!(package_ref.id, "rust-shop");
    }

    #[test]
    fn parses_index_and_finds_package() {
        let index = parse_index(INDEX).expect("index");
        let package_ref = PackageRef::parse("profile/rust-shop").expect("package ref");
        let package = find(&index, &package_ref).expect("package");
        assert_eq!(package.version.as_deref(), Some("1.0.0"));
        assert_eq!(package.dependencies[0].display_ref(), Some("skill/rust"));
    }

    #[test]
    fn searches_metadata() {
        let index = parse_index(INDEX).expect("index");
        let results = search(&index, "rust");
        assert_eq!(results.len(), 1);
    }
}
