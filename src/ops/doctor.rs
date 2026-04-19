use anyhow::{Context, Result};
use console::style;

use crate::config::Config;
use crate::output;

/// Check and fix common issues with the nex-managed config repo.
pub fn run(config: &Config) -> Result<()> {
    println!();
    println!("  {} — checking configuration", style("nex doctor").bold());
    println!();

    let mut fixed = 0;

    // Check mac-app-util integration
    if check_mac_app_util(config, &mut fixed)? {
        // Changes were made — need a switch
    }

    // Check unfree packages allowed
    check_allow_unfree(config, &mut fixed)?;

    // Check ~/.local/bin is on PATH via home.sessionPath
    check_session_path(config, &mut fixed)?;

    if fixed > 0 {
        // Commit the changes so nix doesn't complain about dirty tree
        let repo = &config.repo;
        let _ = std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(repo)
            .output();
        let _ = std::process::Command::new("git")
            .args(["commit", "-m", "nex doctor: apply fixes"])
            .current_dir(repo)
            .output();

        println!();
        println!(
            "  {} {fixed} issue(s) fixed. Run {} to activate.",
            style("✓").green().bold(),
            style("nex switch").bold()
        );
    } else {
        println!("  {} no issues found", style("✓").green().bold());
    }

    println!();
    Ok(())
}

/// Check if mac-app-util is configured for Spotlight-indexable app aliases.
/// If missing, patch flake.nix and mkHost.nix.
fn check_mac_app_util(config: &Config, fixed: &mut usize) -> Result<bool> {
    let flake_path = config.repo.join("flake.nix");
    let mkhost_path = config.repo.join("nix/lib/mkHost.nix");

    let flake = std::fs::read_to_string(&flake_path)
        .with_context(|| format!("reading {}", flake_path.display()))?;

    if flake.contains("mac-app-util") {
        ok("mac-app-util", "Spotlight app aliases enabled");
        return Ok(false);
    }

    warn(
        "mac-app-util",
        "not configured — nix apps won't appear in Spotlight",
    );

    // Patch flake.nix: add input
    let patched_flake = flake
        .replace(
            "home-manager = {\n      url = \"github:nix-community/home-manager\";\n      inputs.nixpkgs.follows = \"nixpkgs\";\n    };\n  };",
            "home-manager = {\n      url = \"github:nix-community/home-manager\";\n      inputs.nixpkgs.follows = \"nixpkgs\";\n    };\n\n    mac-app-util.url = \"github:hraban/mac-app-util\";\n  };"
        )
        // Also handle the variant with different spacing
        .replace(
            "home-manager = {\n      url = \"github:nix-community/home-manager\";\n      inputs.nixpkgs.follows = \"nixpkgs\";\n    };\n  };\n\n  outputs = { self, nixpkgs, nix-darwin, home-manager }:",
            "home-manager = {\n      url = \"github:nix-community/home-manager\";\n      inputs.nixpkgs.follows = \"nixpkgs\";\n    };\n\n    mac-app-util.url = \"github:hraban/mac-app-util\";\n  };\n\n  outputs = { self, nixpkgs, nix-darwin, home-manager, mac-app-util }:"
        );

    // If the outputs line wasn't caught by the combined replace, handle it separately
    let patched_flake = if patched_flake.contains("mac-app-util")
        && !patched_flake
            .contains("outputs = { self, nixpkgs, nix-darwin, home-manager, mac-app-util }")
    {
        patched_flake.replace(
            "outputs = { self, nixpkgs, nix-darwin, home-manager }:",
            "outputs = { self, nixpkgs, nix-darwin, home-manager, mac-app-util }:",
        )
    } else {
        patched_flake
    };

    // Also update mkHost.nix reference
    let patched_flake = patched_flake.replace(
        "inherit nixpkgs nix-darwin home-manager;",
        "inherit nixpkgs nix-darwin home-manager mac-app-util;",
    );

    if !patched_flake.contains("mac-app-util") {
        output::warn("could not auto-patch flake.nix — manual edit required");
        return Ok(false);
    }

    // Validate the patched flake has all required elements before writing
    let has_input = patched_flake.contains("mac-app-util.url");
    let has_output = patched_flake.contains("mac-app-util }");
    let has_inherit = patched_flake.contains("mac-app-util;");
    if !has_input || !has_output || !has_inherit {
        output::warn(
            "could not fully patch flake.nix — partial changes would break the flake.\n\
             Add mac-app-util manually: https://github.com/hraban/mac-app-util",
        );
        return Ok(false);
    }

    std::fs::write(&flake_path, &patched_flake)
        .with_context(|| format!("writing {}", flake_path.display()))?;
    info("patched", &flake_path.display().to_string());

    // Patch mkHost.nix
    if mkhost_path.exists() {
        let mkhost = std::fs::read_to_string(&mkhost_path)
            .with_context(|| format!("reading {}", mkhost_path.display()))?;

        if !mkhost.contains("mac-app-util") {
            let patched = mkhost
                .replace(
                    "{ nixpkgs, nix-darwin, home-manager }:",
                    "{ nixpkgs, nix-darwin, home-manager, mac-app-util }:",
                )
                .replace(
                    "    hostModule\n    home-manager.darwinModules.home-manager",
                    "    hostModule\n    mac-app-util.darwinModules.default\n    home-manager.darwinModules.home-manager",
                )
                .replace(
                    "        extraSpecialArgs = { inherit hostname username; };\n      };",
                    "        extraSpecialArgs = { inherit hostname username; };\n        sharedModules = [\n          mac-app-util.homeManagerModules.default\n        ];\n      };",
                );

            std::fs::write(&mkhost_path, patched)
                .with_context(|| format!("writing {}", mkhost_path.display()))?;
            info("patched", &mkhost_path.display().to_string());
        }
    }

    *fixed += 1;
    Ok(true)
}

/// Check if nixpkgs.config.allowUnfree is set. Many common packages
/// (vscode, slack, spotify, terraform, vault) are unfree.
fn check_allow_unfree(config: &Config, fixed: &mut usize) -> Result<bool> {
    // Check all nix files in the repo for any unfree config
    let base_path = config.repo.join("nix/modules/darwin/base.nix");
    if !base_path.exists() {
        return Ok(false);
    }

    let content = std::fs::read_to_string(&base_path)
        .with_context(|| format!("reading {}", base_path.display()))?;

    if content.contains("allowUnfree") {
        ok("unfree packages", "nixpkgs.config.allowUnfree is set");
        return Ok(false);
    }

    warn(
        "unfree packages",
        "not allowed — vscode, slack, spotify, etc. will fail to install",
    );

    // Insert after the nix.enable or nix.settings line
    let patched = if content.contains("nix.enable = false;") {
        content.replace(
            "nix.enable = false;",
            "nix.enable = false;\n\n  nixpkgs.config.allowUnfree = true;",
        )
    } else if content.contains("nix.settings.experimental-features") {
        content.replace(
            "nix.settings.experimental-features",
            "nixpkgs.config.allowUnfree = true;\n\n  nix.settings.experimental-features",
        )
    } else {
        // Can't find a good insertion point
        output::warn(
            "could not auto-patch base.nix — add `nixpkgs.config.allowUnfree = true;` manually",
        );
        return Ok(false);
    };

    std::fs::write(&base_path, patched)
        .with_context(|| format!("writing {}", base_path.display()))?;
    info("patched", &base_path.display().to_string());

    *fixed += 1;
    Ok(true)
}

/// Check that ~/.local/bin is in home.sessionPath so nex is always on PATH.
fn check_session_path(config: &Config, fixed: &mut usize) -> Result<bool> {
    let base_path = &config.nix_packages_file;
    if !base_path.exists() {
        return Ok(false);
    }

    let content = std::fs::read_to_string(base_path)
        .with_context(|| format!("reading {}", base_path.display()))?;

    let in_nix_config = content.contains("sessionPath");
    let on_path = is_local_bin_on_path();

    if in_nix_config && on_path {
        ok("sessionPath", "~/.local/bin is on PATH");
        return Ok(false);
    }

    if in_nix_config && !on_path {
        warn(
            "sessionPath",
            "configured in nix but ~/.local/bin is not on PATH — run `nex switch` then open a new shell",
        );
        return Ok(false);
    }

    // Not in nix config at all
    warn(
        "sessionPath",
        "~/.local/bin not in PATH — nex may not be found after install",
    );

    // Insert after the home block
    let patched = if content.contains("stateVersion =") {
        content.replace(
            "stateVersion =",
            "sessionPath = [ \"$HOME/.local/bin\" ];\n    stateVersion =",
        )
    } else {
        output::warn(
            "could not auto-patch — add `home.sessionPath = [ \"$HOME/.local/bin\" ];` manually",
        );
        return Ok(false);
    };

    std::fs::write(base_path, patched)
        .with_context(|| format!("writing {}", base_path.display()))?;
    info("patched", &base_path.display().to_string());

    *fixed += 1;
    Ok(true)
}

/// Check if ~/.local/bin (or its expanded form) is on the current $PATH.
fn is_local_bin_on_path() -> bool {
    let path_var = std::env::var("PATH").unwrap_or_default();
    let home = std::env::var("HOME").unwrap_or_default();
    let expanded = format!("{home}/.local/bin");

    path_var
        .split(':')
        .any(|entry| entry == "~/.local/bin" || entry == "$HOME/.local/bin" || entry == expanded)
}

fn ok(label: &str, detail: &str) {
    eprintln!(
        "  {} {}: {}",
        style("✓").green().bold(),
        label,
        style(detail).dim()
    );
}

fn warn(label: &str, detail: &str) {
    eprintln!(
        "  {} {}: {}",
        style("!").yellow().bold(),
        label,
        style(detail).dim()
    );
}

fn info(label: &str, detail: &str) {
    eprintln!("  {} {}: {}", style("→").cyan(), label, style(detail).dim());
}
