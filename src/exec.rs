use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};

/// Run a command, inheriting stdout/stderr. Returns Ok if exit code 0.
fn run(cmd: &mut Command) -> Result<()> {
    let status = cmd
        .status()
        .with_context(|| format!("failed to run: {:?}", cmd))?;
    if !status.success() {
        bail!(
            "command failed with exit code {}",
            status.code().unwrap_or(-1)
        );
    }
    Ok(())
}

/// Validate that a nix package attribute exists in nixpkgs.
pub fn nix_eval_exists(pkg: &str) -> Result<bool> {
    let output = Command::new("nix")
        .args(["eval", &format!("nixpkgs#{pkg}.name"), "--raw"])
        .stderr(std::process::Stdio::null())
        .output()
        .context("failed to run nix eval")?;
    Ok(output.status.success())
}

/// Get the version of a nix package from nixpkgs. Returns None if not found.
pub fn nix_eval_version(pkg: &str) -> Result<Option<String>> {
    let output = Command::new("nix")
        .args(["eval", &format!("nixpkgs#{pkg}.version"), "--raw"])
        .stderr(std::process::Stdio::null())
        .output()
        .context("failed to run nix eval")?;
    if output.status.success() {
        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if version.is_empty() {
            Ok(None)
        } else {
            Ok(Some(version))
        }
    } else {
        Ok(None)
    }
}

/// Check if a brew cask exists and return its version.
pub fn brew_cask_info(pkg: &str) -> Result<Option<String>> {
    let output = Command::new("brew")
        .args(["info", "--json=v2", "--cask", pkg])
        .stderr(std::process::Stdio::null())
        .output();

    let output = match output {
        Ok(o) => o,
        Err(_) => return Ok(None), // brew not in PATH
    };

    if !output.status.success() {
        return Ok(None);
    }

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).unwrap_or(serde_json::Value::Null);

    let version = json
        .get("casks")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("version"))
        .and_then(|v| v.as_str())
        .map(String::from);

    Ok(version)
}

/// Check if a brew formula exists and return its version.
pub fn brew_formula_info(pkg: &str) -> Result<Option<String>> {
    let output = Command::new("brew")
        .args(["info", "--json=v2", "--formula", pkg])
        .stderr(std::process::Stdio::null())
        .output();

    let output = match output {
        Ok(o) => o,
        Err(_) => return Ok(None),
    };

    if !output.status.success() {
        return Ok(None);
    }

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).unwrap_or(serde_json::Value::Null);

    let version = json
        .get("formulae")
        .and_then(|f| f.get(0))
        .and_then(|f| f.get("versions"))
        .and_then(|v| v.get("stable"))
        .and_then(|v| v.as_str())
        .map(String::from);

    Ok(version)
}

/// Search nixpkgs for packages matching a query.
pub fn nix_search(query: &str) -> Result<()> {
    run(Command::new("nix").args(["search", "nixpkgs", query]))
}

/// Run darwin-rebuild switch.
pub fn darwin_rebuild_switch(repo: &Path, hostname: &str) -> Result<()> {
    run(Command::new("darwin-rebuild")
        .args(["switch", "--flake", &format!(".#{hostname}")])
        .current_dir(repo))
}

/// Run darwin-rebuild build (for diff).
pub fn darwin_rebuild_build(repo: &Path, hostname: &str) -> Result<()> {
    run(Command::new("darwin-rebuild")
        .args(["build", "--flake", &format!(".#{hostname}")])
        .current_dir(repo))
}

/// Run darwin-rebuild --rollback.
pub fn darwin_rebuild_rollback(repo: &Path, hostname: &str) -> Result<()> {
    run(Command::new("darwin-rebuild")
        .args(["switch", "--rollback", "--flake", &format!(".#{hostname}")])
        .current_dir(repo))
}

/// Run nix flake update.
pub fn nix_flake_update(repo: &Path) -> Result<()> {
    run(Command::new("nix")
        .args(["flake", "update"])
        .current_dir(repo))
}

/// Run nix shell for an ephemeral package.
pub fn nix_shell(pkg: &str) -> Result<()> {
    run(Command::new("nix").args(["shell", &format!("nixpkgs#{pkg}")]))
}

/// Show diff between current system and new build.
pub fn nix_diff_closures(repo: &Path) -> Result<()> {
    run(Command::new("nix")
        .args([
            "store",
            "diff-closures",
            "/nix/var/nix/profiles/system",
            "./result",
        ])
        .current_dir(repo))
}

/// Garbage collect.
pub fn nix_gc() -> Result<()> {
    run(Command::new("nix").args(["store", "gc"]))?;
    run(Command::new("nix-collect-garbage").args(["-d"]))
}
