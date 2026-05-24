use std::fmt;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

pub const PROFILE_FRAGMENT_SCHEMA_V1: &str = "io.styrene.nex.profile-fragment.v1";

#[derive(Debug, Clone, Deserialize)]
pub struct ProfileFragmentDocument {
    pub fragment: ProfileFragment,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProfileFragment {
    pub schema: String,
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: ProfileFragmentCategory,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub conflicts: Vec<String>,
    #[serde(default)]
    pub platforms: Vec<ProfileFragmentPlatform>,
    pub visibility: Option<ProfileFragmentVisibility>,
    pub safety: Option<ProfileFragmentSafety>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ProfileFragmentCategory {
    Core,
    Platform,
    Desktop,
    Gpu,
    Audio,
    Shell,
    Role,
}

impl fmt::Display for ProfileFragmentCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Core => "core",
            Self::Platform => "platform",
            Self::Desktop => "desktop",
            Self::Gpu => "gpu",
            Self::Audio => "audio",
            Self::Shell => "shell",
            Self::Role => "role",
        })
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ProfileFragmentPlatform {
    Any,
    Linux,
    Macos,
}

impl fmt::Display for ProfileFragmentPlatform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Any => "any",
            Self::Linux => "linux",
            Self::Macos => "macos",
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProfileFragmentVisibility {
    Public,
    Internal,
}

impl fmt::Display for ProfileFragmentVisibility {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Public => "public",
            Self::Internal => "internal",
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProfileFragmentSafety {
    pub mutates_system_services: bool,
    pub mutates_hardware_drivers: bool,
    pub requires_confirmation: bool,
}

impl ProfileFragmentDocument {
    pub fn from_path(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("reading profile fragment {}", path.display()))?;
        Self::from_str(&content).with_context(|| format!("parsing profile fragment {}", path.display()))
    }

    pub fn from_str(content: &str) -> Result<Self> {
        let document: Self = toml::from_str(content).context("invalid profile fragment TOML")?;
        document.validate()?;
        Ok(document)
    }

    pub fn validate(&self) -> Result<()> {
        let fragment = &self.fragment;
        require_non_empty("fragment.schema", &fragment.schema)?;
        require_non_empty("fragment.id", &fragment.id)?;
        require_non_empty("fragment.name", &fragment.name)?;

        if fragment.schema != PROFILE_FRAGMENT_SCHEMA_V1 {
            bail!(
                "unsupported profile fragment schema '{}'; expected '{}'",
                fragment.schema,
                PROFILE_FRAGMENT_SCHEMA_V1
            );
        }

        validate_fragment_id(&fragment.id)?;
        let category_prefix = format!("{}/", fragment.category);
        if !fragment.id.starts_with(&category_prefix) {
            bail!(
                "fragment id '{}' must start with its category prefix '{}'",
                fragment.id,
                category_prefix
            );
        }

        for required in &fragment.requires {
            validate_fragment_id(required)?;
            if required == &fragment.id {
                bail!("fragment '{}' cannot require itself", fragment.id);
            }
        }

        for conflict in &fragment.conflicts {
            validate_fragment_id(conflict)?;
            if conflict == &fragment.id {
                bail!("fragment '{}' cannot conflict with itself", fragment.id);
            }
        }

        if fragment.platforms.is_empty() {
            bail!("fragment.platforms must contain at least one platform");
        }
        if fragment.platforms.contains(&ProfileFragmentPlatform::Any) && fragment.platforms.len() > 1 {
            bail!("fragment.platforms cannot combine 'any' with specific platforms");
        }

        if matches!(
            fragment.category,
            ProfileFragmentCategory::Platform
                | ProfileFragmentCategory::Desktop
                | ProfileFragmentCategory::Gpu
                | ProfileFragmentCategory::Audio
        ) && fragment.safety.is_none()
        {
            bail!("fragment.safety is required for system-mutating fragment categories");
        }

        if let Some(safety) = &fragment.safety {
            if safety.mutates_hardware_drivers && !safety.requires_confirmation {
                bail!("hardware-driver fragments must require confirmation");
            }
        }

        Ok(())
    }
}

pub fn validate_fragment_id(id: &str) -> Result<()> {
    require_non_empty("fragment id", id)?;
    let parts: Vec<&str> = id.split('/').collect();
    if parts.len() != 2 || parts.iter().any(|part| part.is_empty()) {
        bail!("fragment id '{}' must use '<category>/<slug>' format", id);
    }
    for part in parts {
        if !part
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            bail!(
                "fragment id '{}' must use lowercase slugs with [a-z0-9-] characters",
                id
            );
        }
    }
    Ok(())
}

pub fn infer_fragment_path_id(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    let parent = path.parent()?.file_name()?.to_str()?;
    Some(format!("{parent}/{stem}"))
}

pub fn find_fragment_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    visit_fragment_files(root, &mut files)?;
    files.sort();
    Ok(files)
}

fn visit_fragment_files(path: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(path).with_context(|| format!("reading {}", path.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            visit_fragment_files(&path, files)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
            files.push(path);
        }
    }
    Ok(())
}

fn require_non_empty(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        bail!("{field} is required");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_fragment() -> &'static str {
        r#"
[fragment]
schema = "io.styrene.nex.profile-fragment.v1"
id = "gpu/amd"
name = "amd"
description = "AMD GPU"
category = "gpu"
requires = ["platform/linux"]
conflicts = ["gpu/nvidia", "gpu/intel"]
platforms = ["linux"]
visibility = "public"

[fragment.safety]
mutates_system_services = false
mutates_hardware_drivers = true
requires_confirmation = true
"#
    }

    #[test]
    fn valid_profile_fragment_passes() {
        ProfileFragmentDocument::from_str(valid_fragment()).expect("valid fragment");
    }

    #[test]
    fn rejects_category_mismatch() {
        let manifest = valid_fragment().replace("id = \"gpu/amd\"", "id = \"audio/amd\"");
        let error = ProfileFragmentDocument::from_str(&manifest).expect_err("category mismatch");
        assert!(format!("{error:#}").contains("must start with its category prefix"));
    }

    #[test]
    fn rejects_hardware_mutation_without_confirmation() {
        let manifest = valid_fragment().replace("requires_confirmation = true", "requires_confirmation = false");
        let error = ProfileFragmentDocument::from_str(&manifest).expect_err("confirmation required");
        assert!(format!("{error:#}").contains("hardware-driver fragments must require confirmation"));
    }
}
