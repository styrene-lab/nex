use anyhow::Result;

use crate::config::Config;
use crate::edit::{self, EditSession};
use crate::exec;
use crate::nixfile;
use crate::output;
use crate::resolve::{self, Resolution, Source};

pub enum InstallMode {
    /// User explicitly chose nix
    Nix,
    /// User explicitly chose cask
    Cask,
    /// User explicitly chose brew formula
    Brew,
    /// No flag — resolve automatically
    Auto,
}

pub fn run(config: &Config, mode: InstallMode, packages: &[String], dry_run: bool) -> Result<()> {
    if packages.is_empty() {
        anyhow::bail!("no packages specified");
    }

    let mut session = EditSession::new();
    let mut any_added = false;

    for pkg in packages {
        // Check if already declared anywhere
        if is_already_declared(config, pkg)? {
            output::already(pkg);
            continue;
        }

        // Determine the install source
        let source = match &mode {
            InstallMode::Nix => Source::Nix,
            InstallMode::Cask => Source::BrewCask,
            InstallMode::Brew => Source::BrewFormula,
            InstallMode::Auto => {
                if dry_run {
                    // In dry-run, still resolve to show what would happen
                    resolve_source(pkg, dry_run)?
                } else {
                    resolve_source(pkg, dry_run)?
                }
            }
        };

        if dry_run {
            let target = target_description(&source, config);
            output::dry_run(&format!("would add {pkg} to {target} (via {source})"));
            continue;
        }

        // For explicit modes, validate the package exists in that source
        if matches!(mode, InstallMode::Nix) && !exec::nix_eval_exists(pkg)? {
            output::not_found(pkg, "not in nixpkgs — try: nex install --cask");
            continue;
        }

        // Perform the edit
        match install_as(config, &mut session, pkg, &source)? {
            true => {
                output::added_with_source(pkg, &source.to_string());
                any_added = true;
            }
            false => output::already(pkg),
        }
    }

    if dry_run || !any_added {
        return Ok(());
    }

    output::status("switching...");
    match exec::darwin_rebuild_switch(&config.repo, &config.hostname) {
        Ok(()) => {
            session.commit_all()?;
            output::status("done");
            Ok(())
        }
        Err(e) => {
            output::error("switch failed, reverting changes");
            session.revert_all()?;
            Err(e)
        }
    }
}

/// Check if a package is already declared in any config file.
fn is_already_declared(config: &Config, pkg: &str) -> Result<bool> {
    // Check nix package lists
    for nix_file in config.all_nix_package_files() {
        if edit::contains(nix_file, &nixfile::NIX_PACKAGES, pkg)? {
            return Ok(true);
        }
    }
    // Check homebrew lists
    if edit::contains(&config.homebrew_file, &nixfile::HOMEBREW_CASKS, pkg)? {
        return Ok(true);
    }
    if edit::contains(&config.homebrew_file, &nixfile::HOMEBREW_BREWS, pkg)? {
        return Ok(true);
    }
    Ok(false)
}

/// Resolve which source to use for a package (auto mode).
fn resolve_source(pkg: &str, dry_run: bool) -> Result<Source> {
    let resolution = resolve::resolve(pkg)?;

    match resolution {
        Resolution::Single(candidate) => Ok(candidate.source),
        Resolution::Conflict {
            candidates,
            recommended,
            reason,
        } => {
            if dry_run {
                // In dry-run, just use the recommendation
                return Ok(recommended);
            }
            // Interactive prompt
            match resolve::prompt_resolution(pkg, &candidates, &recommended, &reason)? {
                Some(source) => Ok(source),
                None => anyhow::bail!("cancelled"),
            }
        }
        Resolution::NotFound => {
            output::not_found(pkg, "not found in nixpkgs or homebrew");
            anyhow::bail!("package {pkg} not found");
        }
    }
}

/// Insert a package into the appropriate config file.
fn install_as(
    config: &Config,
    session: &mut EditSession,
    pkg: &str,
    source: &Source,
) -> Result<bool> {
    match source {
        Source::Nix => {
            session.backup(&config.nix_packages_file)?;
            edit::insert(&config.nix_packages_file, &nixfile::NIX_PACKAGES, pkg)
        }
        Source::BrewCask => {
            session.backup(&config.homebrew_file)?;
            edit::insert(&config.homebrew_file, &nixfile::HOMEBREW_CASKS, pkg)
        }
        Source::BrewFormula => {
            session.backup(&config.homebrew_file)?;
            edit::insert(&config.homebrew_file, &nixfile::HOMEBREW_BREWS, pkg)
        }
    }
}

/// Human-readable description of the target file for a source.
fn target_description(source: &Source, config: &Config) -> String {
    match source {
        Source::Nix => config.nix_packages_file.display().to_string(),
        Source::BrewCask | Source::BrewFormula => config.homebrew_file.display().to_string(),
    }
}
