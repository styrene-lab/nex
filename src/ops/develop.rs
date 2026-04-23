use std::process::Command;

use anyhow::{bail, Context, Result};
use console::style;

use crate::exec;

/// Expand shorthand flake refs: "user/repo" → "github:user/repo"
pub fn expand_flake_ref(flake: &str) -> String {
    if flake.contains(':') || flake.starts_with('.') || flake.starts_with('/') {
        flake.to_string()
    } else if flake.contains('/') {
        format!("github:{flake}")
    } else {
        // Bare name — assume nixpkgs
        format!("nixpkgs#{flake}")
    }
}

/// Run `nex develop` — enter a dev shell from a flake. Pure nix develop wrapper.
pub fn run(flake: &str) -> Result<()> {
    let flake_ref = expand_flake_ref(flake);
    let nix = exec::find_nix();

    println!(
        "  {} {}",
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

    Ok(())
}
