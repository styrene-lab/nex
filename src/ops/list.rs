use anyhow::Result;
use console::style;

use crate::config::Config;
use crate::edit;
use crate::nixfile;

pub fn run(config: &Config) -> Result<()> {
    tracing::debug!("listing packages");
    // Nix packages from base.nix
    println!("{}", style("Nix packages").green().bold());
    let pkgs = edit::list_packages(&config.nix_packages_file, &nixfile::NIX_PACKAGES)?;
    for pkg in &pkgs {
        println!("  {pkg}");
    }
    println!("  {} total", style(pkgs.len()).dim());

    // Additional module files
    for (name, path) in &config.module_files {
        println!();
        println!("{}", style(format!("Nix packages ({name})")).green().bold());
        let pkgs = edit::list_packages(path, &nixfile::NIX_PACKAGES)?;
        for pkg in &pkgs {
            println!("  {pkg}");
        }
        println!("  {} total", style(pkgs.len()).dim());
    }

    // Homebrew brews
    println!();
    println!("{}", style("Homebrew brews").yellow().bold());
    let brews = edit::list_packages(&config.homebrew_file, &nixfile::HOMEBREW_BREWS)?;
    for b in &brews {
        println!("  {b}");
    }
    println!("  {} total", style(brews.len()).dim());

    // Homebrew casks
    println!();
    println!("{}", style("Homebrew casks").yellow().bold());
    let casks = edit::list_packages(&config.homebrew_file, &nixfile::HOMEBREW_CASKS)?;
    for c in &casks {
        println!("  {c}");
    }
    println!("  {} total", style(casks.len()).dim());

    Ok(())
}
