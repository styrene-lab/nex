use std::process::Command;

use anyhow::{bail, Context, Result};
use console::style;

use crate::exec;

const OMEGON_FLAKE: &str = "github:styrene-lab/omegon";

/// Run `nex develop` — enter a dev shell from a flake with omegon included.
/// Wraps `nix develop` with shorthand for GitHub refs and layers omegon
/// into the shell so AI coding tools are always available.
pub fn run(flake: &str) -> Result<()> {
    // Expand shorthand: "styrene-lab/nex" → "github:styrene-lab/nex"
    let flake_ref = if flake.contains(':') || flake.starts_with('.') || flake.starts_with('/') {
        flake.to_string()
    } else if flake.contains('/') {
        format!("github:{flake}")
    } else {
        // Bare name — assume nixpkgs
        format!("nixpkgs#{flake}")
    };

    let nix = exec::find_nix();

    // Check if omegon is already available
    let has_omegon = Command::new("which")
        .arg("omegon")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if has_omegon {
        // Omegon already in PATH — just enter the project's dev shell
        println!(
            "  {} entering dev shell for {}",
            style("nex develop").bold(),
            style(&flake_ref).cyan()
        );

        let status = Command::new(&nix)
            .args(["develop", &flake_ref, "-c", "bash"])
            .status()
            .context("failed to run nix develop")?;

        if !status.success() {
            bail!("nix develop exited with {}", status.code().unwrap_or(-1));
        }
    } else {
        // Layer omegon into the dev shell via --override-input or shell hook
        // Use nix shell to get omegon, then nix develop for the project
        println!(
            "  {} entering dev shell for {} {}",
            style("nex develop").bold(),
            style(&flake_ref).cyan(),
            style("+ omegon").dim()
        );

        // Build a combined shell: project devShell + omegon binary
        // nix develop runs the project's devShell; we prepend omegon to PATH
        // via a shell wrapper that fetches omegon's store path first
        let omegon_path = Command::new(&nix)
            .args(["build", OMEGON_FLAKE, "--no-link", "--print-out-paths"])
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    let path = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    if !path.is_empty() { Some(path) } else { None }
                } else {
                    None
                }
            });

        match omegon_path {
            Some(path) => {
                // Enter dev shell with omegon prepended to PATH
                let status = Command::new(&nix)
                    .args(["develop", &flake_ref, "-c", "bash", "-c",
                        &format!("export PATH=\"{path}/bin:$PATH\"; exec bash")])
                    .status()
                    .context("failed to run nix develop")?;

                if !status.success() {
                    bail!("nix develop exited with {}", status.code().unwrap_or(-1));
                }
            }
            None => {
                // Omegon build failed — enter without it
                println!(
                    "  {} omegon not available, entering without it",
                    style("!").yellow()
                );

                let status = Command::new(&nix)
                    .args(["develop", &flake_ref, "-c", "bash"])
                    .status()
                    .context("failed to run nix develop")?;

                if !status.success() {
                    bail!("nix develop exited with {}", status.code().unwrap_or(-1));
                }
            }
        }
    }

    Ok(())
}
