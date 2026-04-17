use std::collections::HashSet;

use anyhow::Result;
use console::style;

use crate::config::Config;
use crate::edit;
use crate::exec;
use crate::nixfile;
use crate::output;

struct MigrateReport {
    /// Formulae that exist in nixpkgs and aren't managed by nex.
    candidates: Vec<MigrateCandidate>,
    /// Casks installed locally but not managed by nex.
    unmanaged_casks: Vec<String>,
    /// Packages already tracked in the nex config.
    managed: Vec<(String, &'static str)>,
    /// Formulae with no nixpkgs equivalent.
    brew_only: Vec<String>,
}

struct MigrateCandidate {
    name: String,
    brew_version: String,
    nix_version: String,
}

pub fn run(config: &Config) -> Result<()> {
    if !exec::brew_available() {
        output::error("brew not found — nothing to migrate");
        return Ok(());
    }

    output::status("scanning installed brew packages...");

    // Gather what nex already manages
    let mut managed_nix: HashSet<String> = HashSet::new();
    for nix_file in config.all_nix_package_files() {
        for pkg in edit::list_packages(nix_file, &nixfile::NIX_PACKAGES)? {
            managed_nix.insert(pkg);
        }
    }
    let managed_brews: HashSet<String> =
        edit::list_packages(&config.homebrew_file, &nixfile::HOMEBREW_BREWS)?
            .into_iter()
            .collect();
    let managed_casks: HashSet<String> =
        edit::list_packages(&config.homebrew_file, &nixfile::HOMEBREW_CASKS)?
            .into_iter()
            .collect();

    // Get what's actually installed
    let installed_formulae = exec::brew_leaves()?;
    let installed_casks = exec::brew_list_casks()?;

    let mut report = MigrateReport {
        candidates: Vec::new(),
        unmanaged_casks: Vec::new(),
        managed: Vec::new(),
        brew_only: Vec::new(),
    };

    // Classify formulae
    let formula_count = installed_formulae.len();
    for (i, formula) in installed_formulae.into_iter().enumerate() {
        eprint!(
            "\r  checking formulae [{}/{}] {}",
            i + 1,
            formula_count,
            style(&formula).dim()
        );

        if managed_brews.contains(&formula) {
            report.managed.push((formula, "brew formula"));
            continue;
        }
        if managed_nix.contains(&formula) {
            report.managed.push((formula, "nix"));
            continue;
        }

        // Check nixpkgs availability
        let brew_ver = exec::brew_formula_info(&formula)?.unwrap_or_default();
        match exec::nix_eval_version(&formula)? {
            Some(nix_ver) => {
                report.candidates.push(MigrateCandidate {
                    name: formula,
                    brew_version: brew_ver,
                    nix_version: nix_ver,
                });
            }
            None => {
                report.brew_only.push(formula);
            }
        }
    }
    // Clear the progress line
    eprint!("\r{}\r", " ".repeat(60));

    // Classify casks
    for cask in installed_casks {
        if managed_casks.contains(&cask) {
            report.managed.push((cask, "cask"));
        } else {
            report.unmanaged_casks.push(cask);
        }
    }

    print_report(&report);
    Ok(())
}

fn print_report(report: &MigrateReport) {
    // Migration candidates
    if !report.candidates.is_empty() {
        println!();
        println!(
            "{}",
            style("Migrate candidates (brew formula → nix)")
                .green()
                .bold()
        );
        for c in &report.candidates {
            let versions = if c.brew_version == c.nix_version {
                style(c.nix_version.clone()).dim().to_string()
            } else if c.brew_version.is_empty() {
                style(format!("nix {}", c.nix_version)).dim().to_string()
            } else {
                style(format!("{} → {}", c.brew_version, c.nix_version))
                    .dim()
                    .to_string()
            };
            println!("  {:<24} {}", c.name, versions);
        }
        println!(
            "  {}",
            style(format!(
                "{} formulae can migrate to nix",
                report.candidates.len()
            ))
            .dim()
        );
        println!();
        println!("  run: {}", style("nex install <package>").cyan());
        println!("  then: {}", style("brew uninstall <package>").dim());
    }

    // Unmanaged casks
    if !report.unmanaged_casks.is_empty() {
        println!();
        println!(
            "{}",
            style("Unmanaged casks (not in nex config)").yellow().bold()
        );
        for cask in &report.unmanaged_casks {
            println!("  {cask}");
        }
        println!(
            "  {}",
            style(format!(
                "{} casks installed outside nex",
                report.unmanaged_casks.len()
            ))
            .dim()
        );
        println!();
        println!("  run: {}", style("nex install <cask>").cyan());
    }

    // Already managed
    if !report.managed.is_empty() {
        println!();
        println!("{}", style("Already managed by nex").dim().bold());
        for (pkg, source) in &report.managed {
            println!("  {:<24} {}", pkg, style(source).dim());
        }
        println!(
            "  {}",
            style(format!("{} packages", report.managed.len())).dim()
        );
    }

    // Brew-only (no nix equivalent)
    if !report.brew_only.is_empty() {
        println!();
        println!("{}", style("No nix equivalent found").dim().bold());
        for pkg in &report.brew_only {
            println!("  {pkg}");
        }
        println!(
            "  {}",
            style(format!("{} formulae (brew-only)", report.brew_only.len())).dim()
        );
    }

    if report.candidates.is_empty() && report.unmanaged_casks.is_empty() {
        println!();
        println!(
            "  {} all brew packages are managed by nex",
            style("✓").green().bold()
        );
    }

    println!();
}
