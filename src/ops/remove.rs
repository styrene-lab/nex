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

/// Format the display name — show the alias if it differs from input.
fn display_name(pkg: &str, matched: &str) -> String {
    if matched != pkg {
        format!("{pkg} ({matched})")
    } else {
        pkg.to_string()
    }
}

/// Build the list of names to try: the input, its canonical alias, and all reverse aliases.
fn names_to_try(pkg: &str) -> Vec<String> {
    let mut names = vec![pkg.to_string()];
    let canonical = crate::aliases::nixpkgs_attr(pkg);
    if canonical != pkg && !names.contains(&canonical.to_string()) {
        names.push(canonical.to_string());
    }
    for alias in crate::aliases::all_names_for(pkg) {
        let s = alias.to_string();
        if !names.contains(&s) {
            names.push(s);
        }
    }
    names
}

/// Search all sources for the package (and its aliases) and remove it.
fn try_remove_from_all(
    config: &Config,
    session: &mut EditSession,
    pkg: &str,
    dry_run: bool,
    any_removed: &mut bool,
) -> Result<bool> {
    let names = names_to_try(pkg);

    // Check nix package lists first
    for nix_file in config.all_nix_package_files() {
        for name in &names {
            if edit::contains(nix_file, &nixfile::NIX_PACKAGES, name)? {
                if dry_run {
                    output::dry_run(&format!("would remove {name} from {}", nix_file.display()));
                    return Ok(true);
                }
                session.backup(nix_file)?;
                if edit::remove(nix_file, &nixfile::NIX_PACKAGES, name)? {
                    let label = display_name(pkg, name);
                    output::removed(&label);
                    *any_removed = true;
                    return Ok(true);
                }
            }
        }
    }

    // Check homebrew casks
    for name in &names {
        if edit::contains(&config.homebrew_file, &nixfile::HOMEBREW_CASKS, name)? {
            if dry_run {
                output::dry_run(&format!("would remove cask {name}"));
                return Ok(true);
            }
            session.backup(&config.homebrew_file)?;
            if edit::remove(&config.homebrew_file, &nixfile::HOMEBREW_CASKS, name)? {
                let label = display_name(pkg, name);
                output::removed(&label);
                *any_removed = true;
                return Ok(true);
            }
        }
    }

    // Check homebrew brews
    for name in &names {
        if edit::contains(&config.homebrew_file, &nixfile::HOMEBREW_BREWS, name)? {
            if dry_run {
                output::dry_run(&format!("would remove brew {name}"));
                return Ok(true);
            }
            session.backup(&config.homebrew_file)?;
            if edit::remove(&config.homebrew_file, &nixfile::HOMEBREW_BREWS, name)? {
                let label = display_name(pkg, name);
                output::removed(&label);
                *any_removed = true;
                return Ok(true);
            }
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
    let names = names_to_try(pkg);
    for nix_file in config.all_nix_package_files() {
        for name in &names {
            if edit::contains(nix_file, &nixfile::NIX_PACKAGES, name)? {
                if dry_run {
                    output::dry_run(&format!("would remove {name} from {}", nix_file.display()));
                    return Ok(true);
                }
                session.backup(nix_file)?;
                if edit::remove(nix_file, &nixfile::NIX_PACKAGES, name)? {
                    let label = display_name(pkg, name);
                    output::removed(&label);
                    *any_removed = true;
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}

/// Remove from a specific homebrew list (casks or brews).
fn try_remove_list(
    _config: &Config,
    session: &mut EditSession,
    file: &std::path::Path,
    list: &crate::nixfile::NixList,
    label: &str,
    pkg: &str,
    dry_run: bool,
    any_removed: &mut bool,
) -> Result<bool> {
    let names = names_to_try(pkg);
    for name in &names {
        if edit::contains(file, list, name)? {
            if dry_run {
                output::dry_run(&format!("would remove {label} {name}"));
                return Ok(true);
            }
            session.backup(file)?;
            if edit::remove(file, list, name)? {
                let label = display_name(pkg, name);
                output::removed(&label);
                *any_removed = true;
                return Ok(true);
            }
        }
    }
    Ok(false)
}
