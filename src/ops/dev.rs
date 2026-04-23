use std::process::Command;

use anyhow::{bail, Context, Result};
use console::style;

use crate::exec;
use crate::ops::develop;

const OMEGON_FLAKE: &str = "github:styrene-lab/omegon";

/// Run `nex dev` — open a project with omegon. Fails if omegon can't be resolved.
///
/// This is the opinionated "start working on a project" command:
/// 1. Resolve omegon (must succeed)
/// 2. Enter the project's dev shell
/// 3. Start omegon in the background
/// 4. Drop into the shell with omegon available
pub fn run(project: &str) -> Result<()> {
    let flake_ref = develop::expand_flake_ref(project);
    let nix = exec::find_nix();

    println!(
        "  {} {} {}",
        style("nex dev").bold(),
        style(&flake_ref).cyan(),
        style("+ omegon").dim()
    );

    // Step 1: resolve omegon — hard requirement
    let omegon_path = resolve_omegon(&nix)?;

    let omegon_bin = format!("{omegon_path}/bin/omegon");

    println!(
        "  {} omegon resolved",
        style("✓").green().bold(),
    );

    // Step 2: check auth — if not authenticated, prompt login
    let auth_ok = Command::new(&omegon_bin)
        .args(["auth", "status"])
        .output()
        .map(|o| {
            let stdout = String::from_utf8_lossy(&o.stdout);
            stdout.contains("authenticated") && !stdout.contains("Authenticated:   0")
        })
        .unwrap_or(false);

    if !auth_ok {
        println!(
            "  {} omegon not authenticated — launching login",
            style("!").yellow()
        );
        let login = Command::new(&omegon_bin)
            .args(["auth", "login"])
            .status()
            .context("failed to run omegon auth login")?;

        if !login.success() {
            bail!("omegon auth login failed — run `omegon auth login` manually");
        }
    } else {
        println!(
            "  {} omegon authenticated",
            style("✓").green().bold(),
        );
    }

    // Step 3: enter dev shell with omegon, launch interactive TUI
    println!(
        "  {} entering dev shell + omegon interactive",
        style(">>>").bold()
    );

    let status = Command::new(&nix)
        .args([
            "develop", &flake_ref, "-c", "bash", "-c",
            &format!(
                "export PATH=\"{omegon_path}/bin:$PATH\"; \
                 exec omegon interactive"
            ),
        ])
        .status()
        .context("failed to run nix develop")?;

    if !status.success() {
        bail!("dev session exited with {}", status.code().unwrap_or(-1));
    }

    Ok(())
}

/// Resolve omegon — check PATH first, then build from flake. Fails hard if neither works.
fn resolve_omegon(nix: &str) -> Result<String> {
    // Already installed?
    if let Ok(output) = Command::new("which").arg("omegon").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                // Return the bin dir (strip /bin/omegon)
                if let Some(bin_dir) = std::path::Path::new(&path).parent() {
                    if let Some(pkg_dir) = bin_dir.parent() {
                        return Ok(pkg_dir.display().to_string());
                    }
                }
                return Ok(path);
            }
        }
    }

    // Build from flake
    println!(
        "  {} resolving omegon...",
        style(">>>").bold()
    );

    let output = Command::new(nix)
        .args(["build", OMEGON_FLAKE, "--no-link", "--print-out-paths"])
        .output()
        .context("failed to build omegon")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "omegon is required for `nex dev` but could not be resolved.\n\
             Install it: nix profile install {OMEGON_FLAKE}\n\
             Error: {}", stderr.lines().last().unwrap_or("unknown")
        );
    }

    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        bail!(
            "omegon is required for `nex dev` but build produced no output.\n\
             Install it: nix profile install {OMEGON_FLAKE}"
        );
    }

    Ok(path)
}
