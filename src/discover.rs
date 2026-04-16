use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

/// Auto-detect the nix-darwin repo by walking up from CWD, then checking well-known paths.
pub fn find_repo() -> Result<PathBuf> {
    // 1. Walk up from CWD looking for flake.nix with darwinConfigurations
    if let Ok(cwd) = std::env::current_dir() {
        let mut dir = cwd.as_path();
        loop {
            let flake = dir.join("flake.nix");
            if flake.exists() && is_darwin_flake(&flake) {
                return Ok(dir.to_path_buf());
            }
            match dir.parent() {
                Some(parent) => dir = parent,
                None => break,
            }
        }
    }

    // 2. Check well-known paths
    if let Some(home) = dirs::home_dir() {
        let candidates = [
            home.join("workspace/black-meridian/styrene-lab/macos-nix"),
            home.join("macos-nix"),
            home.join(".config/nix-darwin"),
        ];
        for path in &candidates {
            let flake = path.join("flake.nix");
            if flake.exists() && is_darwin_flake(&flake) {
                return Ok(path.clone());
            }
        }
    }

    anyhow::bail!("could not find nix-darwin repo")
}

/// Check if a flake.nix contains darwinConfigurations.
fn is_darwin_flake(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .map(|content| content.contains("darwinConfigurations"))
        .unwrap_or(false)
}

/// Auto-detect the macOS hostname via scutil.
pub fn hostname() -> Result<String> {
    let output = Command::new("scutil")
        .args(["--get", "LocalHostName"])
        .output()
        .context("failed to run scutil --get LocalHostName")?;

    if !output.status.success() {
        anyhow::bail!("scutil --get LocalHostName failed");
    }

    let name = String::from_utf8(output.stdout)
        .context("hostname is not valid UTF-8")?
        .trim()
        .to_string();

    Ok(name)
}
