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

    std::fs::write(&flake_path, patched_flake)
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
