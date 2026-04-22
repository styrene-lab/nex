use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::exec;

/// Run `nex develop` — enter a dev shell from a flake.
/// Wraps `nix develop` with shorthand for GitHub refs.
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

    let status = Command::new(&nix)
        .args(["develop", &flake_ref, "-c", "bash"])
        .status()
        .context("failed to run nix develop")?;

    if !status.success() {
        bail!("nix develop exited with {}", status.code().unwrap_or(-1));
    }

    Ok(())
}
