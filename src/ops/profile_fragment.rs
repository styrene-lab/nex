use std::path::Path;

use anyhow::{bail, Result};
use serde::Serialize;

use crate::profile_fragment::{
    find_fragment_files, infer_fragment_path_id, ProfileFragmentDocument,
};

pub fn run_validate(path: &Path) -> Result<()> {
    if path.is_dir() {
        let files = find_fragment_files(path)?;
        let mut count = 0usize;
        for file in files {
            if validate_file(&file)? {
                count += 1;
            }
        }
        eprintln!("{} profile fragments valid", count);
    } else {
        ProfileFragmentDocument::from_path(path)?;
        eprintln!("{} is valid", path.display());
    }
    Ok(())
}

pub fn run_inspect(path: &Path, json: bool) -> Result<()> {
    if path.is_dir() {
        bail!("profile-fragment inspect expects a Pkl file or compatibility TOML file, not a directory");
    }
    let document = ProfileFragmentDocument::from_path(path)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&ProfileFragmentInspect::from(&document))?
        );
        return Ok(());
    }

    let fragment = &document.fragment;

    println!("\nProfile Fragment");
    print_kv("ID", &fragment.id);
    print_kv("Name", &fragment.name);
    print_kv("Version", &fragment.version);
    print_kv("Category", &fragment.category.to_string());
    print_kv(
        "Platforms",
        &fragment
            .platforms
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", "),
    );
    if let Some(description) = &fragment.description {
        print_kv("Description", description);
    }
    if let Some(visibility) = &fragment.visibility {
        print_kv("Visibility", &visibility.to_string());
    }
    if !fragment.requires.is_empty() {
        print_kv("Requires", &fragment.requires.join(", "));
    }
    if !fragment.conflicts.is_empty() {
        print_kv("Conflicts", &fragment.conflicts.join(", "));
    }
    if let Some(safety) = &fragment.safety {
        println!("\nSafety");
        print_kv(
            "Mutates system services",
            &safety.mutates_system_services.to_string(),
        );
        print_kv(
            "Mutates hardware drivers",
            &safety.mutates_hardware_drivers.to_string(),
        );
        print_kv(
            "Requires confirmation",
            &safety.requires_confirmation.to_string(),
        );
    }

    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProfileFragmentInspect {
    kind: &'static str,
    schema: String,
    id: String,
    name: String,
    version: String,
    description: Option<String>,
    category: String,
    requires: Vec<String>,
    conflicts: Vec<String>,
    platforms: Vec<String>,
    visibility: Option<String>,
    safety: Option<ProfileFragmentSafetyInspect>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProfileFragmentSafetyInspect {
    mutates_system_services: bool,
    mutates_hardware_drivers: bool,
    requires_confirmation: bool,
}

impl From<&ProfileFragmentDocument> for ProfileFragmentInspect {
    fn from(document: &ProfileFragmentDocument) -> Self {
        let fragment = &document.fragment;
        Self {
            kind: "profile-fragment",
            schema: fragment.schema.clone(),
            id: fragment.id.clone(),
            name: fragment.name.clone(),
            version: fragment.version.clone(),
            description: fragment.description.clone(),
            category: fragment.category.to_string(),
            requires: fragment.requires.clone(),
            conflicts: fragment.conflicts.clone(),
            platforms: fragment.platforms.iter().map(ToString::to_string).collect(),
            visibility: fragment.visibility.as_ref().map(ToString::to_string),
            safety: fragment
                .safety
                .as_ref()
                .map(|safety| ProfileFragmentSafetyInspect {
                    mutates_system_services: safety.mutates_system_services,
                    mutates_hardware_drivers: safety.mutates_hardware_drivers,
                    requires_confirmation: safety.requires_confirmation,
                }),
        }
    }
}

fn validate_file(path: &Path) -> Result<bool> {
    let is_toml_fragment = if path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
        std::fs::read_to_string(path)?.contains("[fragment]")
    } else {
        true
    };
    if !is_toml_fragment {
        return Ok(false);
    }
    let document = ProfileFragmentDocument::from_path(path)?;
    if let Some(path_id) = infer_fragment_path_id(path) {
        if document.fragment.id != path_id {
            bail!(
                "fragment id '{}' does not match path-derived id '{}' for {}",
                document.fragment.id,
                path_id,
                path.display()
            );
        }
    }
    Ok(true)
}

fn print_kv(key: &str, value: &str) {
    println!("  {key}: {value}");
}
