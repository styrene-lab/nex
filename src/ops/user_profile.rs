use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::{aliases, exec, output};

#[derive(Debug, Default, Deserialize, Serialize)]
struct LiveIntent {
    schema: String,
    hostname: String,
    packages: BTreeSet<String>,
}

pub fn install(packages: &[String], dry_run: bool) -> Result<()> {
    require_packages(packages)?;
    let mut intent = load()?;
    for package in packages {
        intent
            .packages
            .insert(aliases::nixpkgs_attr(package).to_string());
    }
    if dry_run {
        output::dry_run(&format!(
            "would record {} in the live machine profile",
            packages.join(", ")
        ));
        return Ok(());
    }
    reconcile(&intent)?;
    save(&intent)
}

pub fn remove(packages: &[String], dry_run: bool) -> Result<()> {
    require_packages(packages)?;
    let mut intent = load()?;
    for package in packages {
        intent.packages.remove(aliases::nixpkgs_attr(package));
    }
    if dry_run {
        output::dry_run(&format!(
            "would remove {} from the live machine profile",
            packages.join(", ")
        ));
        return Ok(());
    }
    reconcile(&intent)?;
    save(&intent)
}

pub fn polymerize(dry_run: bool) -> Result<()> {
    let intent = load()?;
    if dry_run {
        output::dry_run(&format!(
            "would polymerize {} live packages",
            intent.packages.len()
        ));
        return Ok(());
    }
    reconcile(&intent)
}

fn reconcile(intent: &LiveIntent) -> Result<()> {
    let profile = profile_path()?;
    if let Some(parent) = profile.parent() {
        fs::create_dir_all(parent)?;
    }
    let installables: Vec<String> = intent
        .packages
        .iter()
        .map(|p| format!("nixpkgs#{p}"))
        .collect();
    output::status(&format!(
        "polymerizing {} live packages...",
        installables.len()
    ));
    exec::nix_profile_reconcile(&profile, &installables)
}

fn load() -> Result<LiveIntent> {
    let path = intent_path()?;
    if path.exists() {
        return toml::from_str(&fs::read_to_string(&path)?).context("invalid live machine profile");
    }
    Ok(LiveIntent {
        schema: "io.styrene.nex.live-profile.v1".into(),
        hostname: hostname()?,
        packages: BTreeSet::new(),
    })
}

fn save(intent: &LiveIntent) -> Result<()> {
    let path = intent_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    crate::edit::atomic_write_bytes(&path, toml::to_string_pretty(intent)?.as_bytes())
}

fn machine_dir() -> Result<PathBuf> {
    let root = std::env::var_os("NEX_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::state_dir()
                .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".local/state"))
                .join("nex")
        });
    Ok(root.join("machines").join(hostname()?))
}
fn intent_path() -> Result<PathBuf> {
    Ok(machine_dir()?.join("live-profile.toml"))
}
fn profile_path() -> Result<PathBuf> {
    Ok(machine_dir()?.join("profile"))
}
fn hostname() -> Result<String> {
    crate::discover::hostname().context("could not determine hostname")
}
fn require_packages(packages: &[String]) -> Result<()> {
    if packages.is_empty() {
        bail!("no packages specified")
    } else {
        Ok(())
    }
}
