use std::collections::HashSet;

use anyhow::Result;
use console::style;

use crate::config::Config;
use crate::edit;
use crate::exec;
use crate::nixfile;
use crate::output;

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
    let confirm = dialoguer::Confirm::new()
        .with_prompt(format!(
            "  Add {total} packages to {}?",
            config.homebrew_file.display()
        ))
        .default(true)
        .interact()?;

    if !confirm {
        println!("  cancelled");
        return Ok(());
    }

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
    println!();
    println!(
        "  It's now safe to run {}. Your existing brew packages",
        style("nex switch").bold()
    );
    println!("  won't be removed.");
    println!();
    println!(
        "  Later, run {} to see which formulae can move to nix.",
        style("nex migrate").cyan()
    );
    println!();

    Ok(())
}
