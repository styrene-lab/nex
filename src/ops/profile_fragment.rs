use std::path::Path;

use anyhow::{bail, Result};

use crate::profile_fragment::{find_fragment_files, infer_fragment_path_id, ProfileFragmentDocument};

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

pub fn run_inspect(path: &Path) -> Result<()> {
    if path.is_dir() {
        bail!("profile-fragment inspect expects a TOML file, not a directory");
    }
    let document = ProfileFragmentDocument::from_path(path)?;
    let fragment = &document.fragment;

    println!("\nProfile Fragment");
    print_kv("ID", &fragment.id);
    print_kv("Name", &fragment.name);
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
        print_kv("Requires confirmation", &safety.requires_confirmation.to_string());
    }

    Ok(())
}

fn validate_file(path: &Path) -> Result<bool> {
    let content = std::fs::read_to_string(path)?;
    if !content.contains("[fragment]") {
        return Ok(false);
    }
    let document = ProfileFragmentDocument::from_str(&content)?;
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
