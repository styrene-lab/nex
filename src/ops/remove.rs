use anyhow::Result;

use crate::config::Config;
use crate::edit::{self, EditSession};
use crate::exec;
use crate::nixfile;
use crate::output;

pub enum RemoveMode {
    /// Search everywhere for the package
    Auto,
    /// Only remove from nix packages
    Nix,
    /// Only remove from casks
    Cask,
    /// Only remove from brews
    Brew,
}

pub fn run(config: &Config, mode: RemoveMode, packages: &[String], dry_run: bool) -> Result<()> {
    if packages.is_empty() {
        anyhow::bail!("no packages specified");
    }

    let mut session = EditSession::new();
    let mut any_removed = false;

    for pkg in packages {
        let removed = match mode {
            RemoveMode::Auto => {
                try_remove_from_all(config, &mut session, pkg, dry_run, &mut any_removed)?
            }
            RemoveMode::Nix => {
                try_remove_nix(config, &mut session, pkg, dry_run, &mut any_removed)?
            }
            RemoveMode::Cask => try_remove_list(
                config,
                &mut session,
                &config.homebrew_file,
                &nixfile::HOMEBREW_CASKS,
                "cask",
                pkg,
                dry_run,
                &mut any_removed,
            )?,
            RemoveMode::Brew => try_remove_list(
                config,
                &mut session,
                &config.homebrew_file,
                &nixfile::HOMEBREW_BREWS,
                "brew",
                pkg,
                dry_run,
                &mut any_removed,
            )?,
        };

        if !removed {
            output::not_found(pkg, "not found in any package list");
        }
    }

    if dry_run || !any_removed {
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

/// Search all sources for the package and remove it from wherever it's found.
fn try_remove_from_all(
    config: &Config,
    session: &mut EditSession,
    pkg: &str,
    dry_run: bool,
    any_removed: &mut bool,
) -> Result<bool> {
    // Check nix package lists first
    for nix_file in config.all_nix_package_files() {
        if edit::contains(nix_file, &nixfile::NIX_PACKAGES, pkg)? {
            if dry_run {
                output::dry_run(&format!("would remove {pkg} from {}", nix_file.display()));
                return Ok(true);
            }
            session.backup(nix_file)?;
            if edit::remove(nix_file, &nixfile::NIX_PACKAGES, pkg)? {
                output::removed(pkg);
                *any_removed = true;
                return Ok(true);
            }
        }
    }

    // Check homebrew casks
    if edit::contains(&config.homebrew_file, &nixfile::HOMEBREW_CASKS, pkg)? {
        if dry_run {
            output::dry_run(&format!("would remove cask {pkg}"));
            return Ok(true);
        }
        session.backup(&config.homebrew_file)?;
        if edit::remove(&config.homebrew_file, &nixfile::HOMEBREW_CASKS, pkg)? {
            output::removed(pkg);
            *any_removed = true;
            return Ok(true);
        }
    }

    // Check homebrew brews
    if edit::contains(&config.homebrew_file, &nixfile::HOMEBREW_BREWS, pkg)? {
        if dry_run {
            output::dry_run(&format!("would remove brew {pkg}"));
            return Ok(true);
        }
        session.backup(&config.homebrew_file)?;
        if edit::remove(&config.homebrew_file, &nixfile::HOMEBREW_BREWS, pkg)? {
            output::removed(pkg);
            *any_removed = true;
            return Ok(true);
        }
    }

    Ok(false)
}

/// Remove from nix package lists only.
fn try_remove_nix(
    config: &Config,
    session: &mut EditSession,
    pkg: &str,
    dry_run: bool,
    any_removed: &mut bool,
) -> Result<bool> {
    for nix_file in config.all_nix_package_files() {
        if edit::contains(nix_file, &nixfile::NIX_PACKAGES, pkg)? {
            if dry_run {
                output::dry_run(&format!("would remove {pkg} from {}", nix_file.display()));
                return Ok(true);
            }
            session.backup(nix_file)?;
            if edit::remove(nix_file, &nixfile::NIX_PACKAGES, pkg)? {
                output::removed(pkg);
                *any_removed = true;
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Remove from a specific homebrew list (casks or brews).
fn try_remove_list(
    config: &Config,
    session: &mut EditSession,
    file: &std::path::Path,
    list: &crate::nixfile::NixList,
    label: &str,
    pkg: &str,
    dry_run: bool,
    any_removed: &mut bool,
) -> Result<bool> {
    if edit::contains(file, list, pkg)? {
        if dry_run {
            output::dry_run(&format!("would remove {label} {pkg}"));
            return Ok(true);
        }
        session.backup(file)?;
        if edit::remove(file, list, pkg)? {
            output::removed(pkg);
            *any_removed = true;
            return Ok(true);
        }
    }
    Ok(false)
}
