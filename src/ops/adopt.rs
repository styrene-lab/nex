use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::Result;
use console::style;

use crate::config::Config;
use crate::edit::{self, EditSession};
use crate::exec;
use crate::nixfile;
use crate::output;

/// Check if stdin is interactive. When non-interactive, return the default.
fn confirm_or_default(prompt: &str, default: bool) -> Result<bool> {
    if !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        return Ok(default);
    }
    Ok(dialoguer::Confirm::new()
        .with_prompt(prompt)
        .default(default)
        .interact()?)
}

/// Capture all installed brew packages into the nex config so the first
/// `nex switch` doesn't zap anything. This is the safe onboarding path for
/// existing Macs.
pub fn run(config: &Config, dry_run: bool) -> Result<()> {
    if !exec::brew_available() {
        output::error("brew not found — nothing to adopt");
        return Ok(());
    }

    println!();
    println!(
        "  {} — capturing installed packages",
        style("nex adopt").bold()
    );
    println!();

    // What nex already manages
    let managed_brews: HashSet<String> =
        edit::list_packages(&config.homebrew_file, &nixfile::HOMEBREW_BREWS)?
            .into_iter()
            .collect();
    let managed_casks: HashSet<String> =
        edit::list_packages(&config.homebrew_file, &nixfile::HOMEBREW_CASKS)?
            .into_iter()
            .collect();

    // What's actually installed
    let installed_formulae = exec::brew_leaves()?;
    let installed_casks = exec::brew_list_casks()?;

    // Figure out what's missing from the config
    let new_formulae: Vec<&String> = installed_formulae
        .iter()
        .filter(|f| !managed_brews.contains(*f))
        .collect();
    let new_casks: Vec<&String> = installed_casks
        .iter()
        .filter(|c| !managed_casks.contains(*c))
        .collect();

    if new_formulae.is_empty() && new_casks.is_empty() {
        println!(
            "  {} all installed brew packages are already in the nex config",
            style("✓").green().bold()
        );
        println!();
        return Ok(());
    }

    // Show what we'll add
    if !new_formulae.is_empty() {
        println!(
            "  {} brew formulae to add:",
            style(new_formulae.len()).bold()
        );
        for f in &new_formulae {
            println!("    {} {}", style("+").green(), f);
        }
        println!();
    }

    if !new_casks.is_empty() {
        println!("  {} brew casks to add:", style(new_casks.len()).bold());
        for c in &new_casks {
            println!("    {} {}", style("+").green(), c);
        }
        println!();
    }

    if dry_run {
        output::dry_run(&format!(
            "would add {} formulae and {} casks to {}",
            new_formulae.len(),
            new_casks.len(),
            config.homebrew_file.display()
        ));
        println!();
        return Ok(());
    }

    // Confirm
    let total = new_formulae.len() + new_casks.len();
    let confirm = confirm_or_default(
        &format!(
            "  Add {total} packages to {}?",
            config.homebrew_file.display()
        ),
        true,
    )?;

    if !confirm {
        println!("  cancelled");
        return Ok(());
    }

    // Back up before bulk edit so we can revert on failure
    let mut session = EditSession::new();
    session.backup(&config.homebrew_file)?;

    // Insert formulae
    let mut added_formulae = 0;
    for formula in &new_formulae {
        if edit::insert(&config.homebrew_file, &nixfile::HOMEBREW_BREWS, formula)? {
            added_formulae += 1;
        }
    }

    // Insert casks
    let mut added_casks = 0;
    for cask in &new_casks {
        if edit::insert(&config.homebrew_file, &nixfile::HOMEBREW_CASKS, cask)? {
            added_casks += 1;
        }
    }

    // Edits succeeded — commit the session (delete backup)
    session.commit_all()?;

    // Commit so nix doesn't complain about dirty tree
    let _ = std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(&config.repo)
        .output();
    let _ = std::process::Command::new("git")
        .args(["commit", "-m", "nex adopt: capture existing brew packages"])
        .current_dir(&config.repo)
        .output();

    println!();
    println!(
        "  {} added {} formulae and {} casks",
        style("✓").green().bold(),
        added_formulae,
        added_casks
    );

    // Check for PATH collisions — binaries that exist outside nix/brew
    // that nix packages would shadow after switch
    let nix_packages = edit::list_packages(&config.nix_packages_file, &nixfile::NIX_PACKAGES)?;
    let collisions = find_path_collisions(&nix_packages);
    if !collisions.is_empty() {
        println!();
        println!("{}", style("PATH collisions detected").yellow().bold());
        println!();
        println!("  The following binaries exist outside nix/brew. After switch,");
        println!("  nix versions will take priority. You can pin each one to");
        println!("  keep your existing version.");
        println!();

        let mut pinned = 0;
        for (binary, existing_path, existing_ver) in &collisions {
            let nix_ver = exec::nix_eval_version(binary)
                .ok()
                .flatten()
                .unwrap_or_else(|| "?".into());

            let ver_info = if let Some(v) = existing_ver {
                format!("{} -> nix {}", v, nix_ver)
            } else {
                format!("-> nix {}", nix_ver)
            };

            println!(
                "    {} {} {}  ({})",
                style("!").yellow(),
                style(binary).bold(),
                style(&ver_info).dim(),
                style(existing_path.display()).dim()
            );

            let pin = confirm_or_default(
                &format!("    Keep existing {binary}? (removes from nix config)"),
                false,
            )?;

            if pin {
                if edit::remove(&config.nix_packages_file, &nixfile::NIX_PACKAGES, binary)? {
                    println!(
                        "    {} pinned — {} removed from nix config",
                        style("✓").green(),
                        binary
                    );
                    pinned += 1;
                }
            }
        }

        if pinned > 0 {
            // Re-commit after pins
            let _ = std::process::Command::new("git")
                .args(["add", "-A"])
                .current_dir(&config.repo)
                .output();
            let _ = std::process::Command::new("git")
                .args(["commit", "-m", "nex adopt: pin existing binaries"])
                .current_dir(&config.repo)
                .output();
        }
    }

    println!();
    println!(
        "  It's now safe to run {}. Your existing packages",
        style("nex switch").bold()
    );
    println!("  won't be removed or shadowed unexpectedly.");
    println!();
    println!(
        "  Later, run {} to see which formulae can move to nix.",
        style("nex migrate").cyan()
    );
    println!();

    Ok(())
}

/// Check if any nix package names correspond to binaries that already exist
/// on PATH at non-nix, non-brew locations (manual installs the user may care about).
/// Returns (package_name, existing_binary_path, version_if_available).
fn find_path_collisions(nix_packages: &[String]) -> Vec<(String, PathBuf, Option<String>)> {
    let path_var = std::env::var("PATH").unwrap_or_default();
    let skip_prefixes = ["/nix/", "/opt/homebrew/"];

    let mut collisions = Vec::new();

    for pkg in nix_packages {
        for dir in path_var.split(':') {
            if skip_prefixes.iter().any(|p| dir.starts_with(p)) {
                continue;
            }
            let bin = PathBuf::from(dir).join(pkg);
            if bin.exists() {
                // Try to get the version of the existing binary
                let version = std::process::Command::new(&bin)
                    .arg("--version")
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .and_then(|o| {
                        let out = String::from_utf8_lossy(&o.stdout).to_string();
                        // Extract first line, trim
                        out.lines().next().map(|l| l.trim().to_string())
                    })
                    .filter(|v| !v.is_empty());

                collisions.push((pkg.clone(), bin, version));
                break;
            }
        }
    }

    collisions
}
