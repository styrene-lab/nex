use std::fmt;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

pub const MACHINE_PROFILE_FILE: &str = "machine-profile.pkl";
pub const MACHINE_PROFILE_TOML_COMPAT_FILE: &str = "machine-profile.toml";
pub const MACHINE_PROFILE_SCHEMA_V1: &str = "io.styrene.nex.machine-profile.v1";

#[derive(Debug, Clone, Deserialize)]
pub struct MachineProfileDocument {
    pub machine_profile: MachineProfile,
    #[serde(default)]
    pub dependencies: Vec<MachineProfileDependency>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MachineProfile {
    pub schema: String,
    pub id: String,
    pub slug: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub license: Option<String>,
    pub min_nex: String,
    pub defaults: MachineProfileDefaults,
    pub safety: MachineProfileSafety,
    pub secrets: Option<MachineProfileSecrets>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MachineProfileDefaults {
    pub mode: MachineProfileMode,
    pub target: MachineProfileTarget,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MachineProfileMode {
    PlanOnly,
    ImageBuild,
    VmBuild,
    Provision,
}

impl fmt::Display for MachineProfileMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::PlanOnly => "plan-only",
            Self::ImageBuild => "image-build",
            Self::VmBuild => "vm-build",
            Self::Provision => "provision",
        };
        f.write_str(value)
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum MachineProfileTarget {
    NixDevshell,
    OciImage,
    Vm,
    PhysicalMachine,
}

impl fmt::Display for MachineProfileTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::NixDevshell => "nix-devshell",
            Self::OciImage => "oci-image",
            Self::Vm => "vm",
            Self::PhysicalMachine => "physical-machine",
        };
        f.write_str(value)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct MachineProfileSafety {
    pub default_destructive: bool,
    pub requires_confirmation: bool,
    pub requires_target_attestation: bool,
    pub allowed_targets: Vec<MachineProfileTarget>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MachineProfileSecrets {
    #[serde(default)]
    pub required: Vec<String>,
    #[serde(default)]
    pub optional: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MachineProfileDependency {
    pub kind: MachineProfileDependencyKind,
    pub id: String,
    pub version: String,
    pub required: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MachineProfileDependencyKind {
    ForgeTemplate,
}

impl fmt::Display for MachineProfileDependencyKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ForgeTemplate => f.write_str("forge-template"),
        }
    }
}

impl MachineProfileDocument {
    pub fn from_path(path: &Path) -> Result<Self> {
        let manifest_path = resolve_manifest_path(path)?;
        let loaded = crate::document::load_document::<Self>(&manifest_path, "machine profile")?;
        loaded
            .value
            .validate()
            .with_context(|| format!("validating {}", manifest_path.display()))?;
        Ok(loaded.value)
    }

    pub fn from_str(content: &str) -> Result<Self> {
        let document: Self = toml::from_str(content).context("invalid compatibility machine profile TOML")?;
        document.validate()?;
        Ok(document)
    }

    pub fn validate(&self) -> Result<()> {
        let profile = &self.machine_profile;
        require_non_empty("machine_profile.schema", &profile.schema)?;
        require_non_empty("machine_profile.id", &profile.id)?;
        require_non_empty("machine_profile.slug", &profile.slug)?;
        require_non_empty("machine_profile.name", &profile.name)?;
        require_non_empty("machine_profile.version", &profile.version)?;
        require_non_empty("machine_profile.min_nex", &profile.min_nex)?;

        if profile.schema != MACHINE_PROFILE_SCHEMA_V1 {
            bail!(
                "unsupported machine profile schema '{}'; expected '{}'",
                profile.schema,
                MACHINE_PROFILE_SCHEMA_V1
            );
        }

        if !profile
            .safety
            .allowed_targets
            .contains(&profile.defaults.target)
        {
            bail!(
                "default target '{}' is not listed in machine_profile.safety.allowed_targets",
                profile.defaults.target
            );
        }

        if profile.safety.default_destructive && !profile.safety.requires_confirmation {
            bail!("destructive machine profiles must require confirmation");
        }

        if profile.defaults.target == MachineProfileTarget::PhysicalMachine
            && !profile.safety.requires_target_attestation
        {
            bail!("physical-machine targets require target attestation");
        }

        if let Some(secrets) = &profile.secrets {
            for secret in secrets.required.iter().chain(secrets.optional.iter()) {
                validate_secret_name(secret)?;
            }
        }

        for dependency in &self.dependencies {
            require_non_empty("dependencies.id", &dependency.id)?;
            require_non_empty("dependencies.version", &dependency.version)?;
            let _ = dependency.kind.to_string();
            let _ = dependency.required;
        }

        Ok(())
    }
}

pub fn resolve_manifest_path(path: &Path) -> Result<PathBuf> {
    if path.is_dir() {
        let canonical = path.join(MACHINE_PROFILE_FILE);
        if canonical.exists() {
            return Ok(canonical);
        }
        let compat = path.join(MACHINE_PROFILE_TOML_COMPAT_FILE);
        if compat.exists() {
            return Ok(compat);
        }
        Ok(canonical)
    } else {
        Ok(path.to_path_buf())
    }
}

fn require_non_empty(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        bail!("{field} is required");
    }
    Ok(())
}

fn validate_secret_name(secret: &str) -> Result<()> {
    require_non_empty("secret name", secret)?;
    if secret.contains('=') || secret.contains(':') || secret.contains('/') || secret.contains('\\') {
        bail!("secret '{}' must be a name, not a value or path", secret);
    }
    if !secret
        .chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
    {
        bail!(
            "secret '{}' must use uppercase environment-name syntax [A-Z0-9_]",
            secret
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_manifest() -> &'static str {
        r#"
[machine_profile]
schema = "io.styrene.nex.machine-profile.v1"
id = "io.styrene.nex.machine-profile.example"
slug = "example"
name = "Example Machine Profile"
version = "1.0.0"
description = "Reusable machine materialization policy."
license = "MIT"
min_nex = "0.18.0"

[machine_profile.defaults]
mode = "plan-only"
target = "oci-image"

[machine_profile.safety]
default_destructive = false
requires_confirmation = true
requires_target_attestation = true
allowed_targets = ["nix-devshell", "oci-image", "vm", "physical-machine"]

[machine_profile.secrets]
required = ["GITHUB_TOKEN"]
optional = ["AWS_PROFILE"]

[[dependencies]]
kind = "forge-template"
id = "nixos-workstation"
version = ">=1.0.0"
required = true
"#
    }

    #[test]
    fn valid_machine_profile_passes() {
        MachineProfileDocument::from_str(valid_manifest()).expect("valid manifest");
    }

    #[test]
    fn rejects_wrong_schema() {
        let manifest = valid_manifest().replace(MACHINE_PROFILE_SCHEMA_V1, "wrong");
        let error = MachineProfileDocument::from_str(&manifest).expect_err("schema rejected");
        assert!(error.to_string().contains("unsupported machine profile schema"));
    }

    #[test]
    fn rejects_secret_values() {
        let manifest = valid_manifest().replace("GITHUB_TOKEN", "GITHUB_TOKEN=secret");
        let error = MachineProfileDocument::from_str(&manifest).expect_err("secret value rejected");
        assert!(error.to_string().contains("must be a name"));
    }

    #[test]
    fn rejects_physical_machine_without_attestation() {
        let manifest = valid_manifest()
            .replace("target = \"oci-image\"", "target = \"physical-machine\"")
            .replace("requires_target_attestation = true", "requires_target_attestation = false");
        let error = MachineProfileDocument::from_str(&manifest).expect_err("attestation required");
        let message = format!("{error:#}");
        assert!(message.contains("physical-machine targets require target attestation"));
    }
}
