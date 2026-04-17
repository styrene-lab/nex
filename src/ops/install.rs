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
            InstallMode::Auto => resolve_source(pkg, dry_run, config.prefer_nix_on_equal)?,
        };

        if dry_run {
            let target = target_description(&source, config);
            output::dry_run(&format!("would add {pkg} to {target} (via {source})"));
            continue;
        }

        // For explicit modes, validate the package exists in that source
        if matches!(mode, InstallMode::Nix) {
            let attr = crate::aliases::nixpkgs_attr(pkg);
            if !exec::nix_eval_exists(attr)? && (attr == pkg || !exec::nix_eval_exists(pkg)?) {
                output::not_found(pkg, "not in nixpkgs — try: nex install --cask");
                continue;
            }
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
/// Also checks known aliases (e.g. "rg" matches "ripgrep").
fn is_already_declared(config: &Config, pkg: &str) -> Result<bool> {
    let names_to_check = crate::aliases::all_names_for(pkg);

    // Check nix package lists against all known aliases
    for nix_file in config.all_nix_package_files() {
        if edit::contains(nix_file, &nixfile::NIX_PACKAGES, pkg)? {
            return Ok(true);
        }
        for alias in &names_to_check {
            if *alias != pkg && edit::contains(nix_file, &nixfile::NIX_PACKAGES, alias)? {
                return Ok(true);
            }
        }
    }
    // Check homebrew lists
    if edit::contains(&config.homebrew_file, &nixfile::HOMEBREW_CASKS, pkg)? {
        return Ok(true);
    }
    if edit::contains(&config.homebrew_file, &nixfile::HOMEBREW_BREWS, pkg)? {
        return Ok(true);
    }
    for alias in &names_to_check {
        if *alias != pkg {
            if edit::contains(&config.homebrew_file, &nixfile::HOMEBREW_CASKS, alias)? {
                return Ok(true);
            }
            if edit::contains(&config.homebrew_file, &nixfile::HOMEBREW_BREWS, alias)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Resolve which source to use for a package (auto mode).
fn resolve_source(pkg: &str, dry_run: bool, prefer_nix_on_equal: bool) -> Result<Source> {
    let result = resolve::resolve(pkg)?;

    // Warn if brew wasn't available — the user might be getting nix-only
    // results for packages that should be casks.
    if !result.brew_checked {
        if let Resolution::Single(ref c) = result.resolution {
            if c.source == Source::Nix {
                output::warn(
                    "brew not available — cask/formula check skipped; \
                     use --cask or --brew to install via homebrew",
                );
            }
        }
    }

    match result.resolution {
        Resolution::Single(candidate) => Ok(candidate.source),
        Resolution::Conflict {
            candidates,
            recommended,
            reason,
            versions_equal,
        } => {
            // If user previously chose "always nix" and versions match, skip prompt
            if versions_equal && prefer_nix_on_equal {
                return Ok(Source::Nix);
            }

            if dry_run {
                return Ok(recommended);
            }

            // Interactive prompt
            match resolve::prompt_resolution(
                pkg,
                &candidates,
                &recommended,
                &reason,
                versions_equal,
            )? {
                Some(result) => {
                    if result.remember_nix {
                        if let Err(e) = crate::config::set_preference("prefer_nix_on_equal", "true")
                        {
                            output::warn(&format!("could not save preference: {e}"));
                        } else {
                            eprintln!(
                                "  {} saved — won't ask again for equal versions",
                                console::style("✓").green()
                            );
                        }
                    }
                    Ok(result.source)
                }
                None => anyhow::bail!("cancelled"),
            }
        }
        Resolution::NotFound => {
            let hint = if result.brew_checked {
                "not found in nixpkgs or homebrew"
            } else {
                "not found in nixpkgs (brew unavailable — install homebrew?)"
            };
            output::not_found(pkg, hint);
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
            // Use the canonical nixpkgs attr (e.g. "zed" -> "zed-editor")
            let attr = crate::aliases::nixpkgs_attr(pkg);
            session.backup(&config.nix_packages_file)?;
            edit::insert(&config.nix_packages_file, &nixfile::NIX_PACKAGES, attr)
        }
        Source::BrewCask => {
            // Use the canonical cask name (e.g. "vscode" -> "visual-studio-code")
            let cask = crate::aliases::brew_cask_name(pkg).unwrap_or(pkg);
            session.backup(&config.homebrew_file)?;
            edit::insert(&config.homebrew_file, &nixfile::HOMEBREW_CASKS, cask)
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
