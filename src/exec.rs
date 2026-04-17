use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};

/// Run a command, inheriting stdout/stderr. Returns Ok if exit code 0.
fn run(cmd: &mut Command) -> Result<()> {
    let program = cmd.get_program().to_string_lossy().to_string();
    match cmd.status() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => bail!(
            "{program} failed with exit code {}",
            status.code().unwrap_or(-1)
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            bail!(
                "{program} not found — is it installed and in your PATH?\n\
                   hint: if you haven't run darwin-rebuild yet, see: nex.styrene.io"
            )
        }
        Err(e) => Err(e).with_context(|| format!("failed to run {program}")),
    }
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

/// Check whether the `brew` binary is available.
pub fn brew_available() -> bool {
    Command::new("brew")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
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

/// List installed brew formulae (top-level only, not deps).
pub fn brew_leaves() -> Result<Vec<String>> {
    let output = Command::new("brew")
        .arg("leaves")
        .stderr(std::process::Stdio::null())
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return Ok(Vec::new()),
    };

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect())
}

/// List installed brew casks.
pub fn brew_list_casks() -> Result<Vec<String>> {
    let output = Command::new("brew")
        .args(["list", "--cask"])
        .stderr(std::process::Stdio::null())
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return Ok(Vec::new()),
    };

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect())
}

/// Search nixpkgs for packages matching a query.
pub fn nix_search(query: &str) -> Result<()> {
    run(Command::new("nix").args(["search", "nixpkgs", query]))
}

/// Resolve the absolute path to darwin-rebuild so sudo can find it.
fn find_darwin_rebuild() -> Result<String> {
    // Check user's PATH first
    if let Ok(output) = Command::new("which").arg("darwin-rebuild").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Ok(path);
            }
        }
    }

    // Known locations after nix-darwin activation
    let candidates = [
        "/run/current-system/sw/bin/darwin-rebuild",
        "/nix/var/nix/profiles/system/sw/bin/darwin-rebuild",
    ];
    for path in &candidates {
        if std::path::Path::new(path).exists() {
            return Ok(path.to_string());
        }
    }

    bail!(
        "darwin-rebuild not found — is nix-darwin activated?\n\
         hint: run `nex init` first, or see nex.styrene.io"
    )
}

/// Run darwin-rebuild switch (requires sudo for system activation).
pub fn darwin_rebuild_switch(repo: &Path, hostname: &str) -> Result<()> {
    let dr = find_darwin_rebuild()?;
    run(Command::new("sudo")
        .args([&dr, "switch", "--flake", &format!(".#{hostname}")])
        .current_dir(repo))
}

/// Run darwin-rebuild build (for diff). No sudo needed — build only.
pub fn darwin_rebuild_build(repo: &Path, hostname: &str) -> Result<()> {
    let dr = find_darwin_rebuild()?;
    run(Command::new(&dr)
        .args(["build", "--flake", &format!(".#{hostname}")])
        .current_dir(repo))
}

/// Run darwin-rebuild --rollback (requires sudo for system activation).
pub fn darwin_rebuild_rollback(repo: &Path, hostname: &str) -> Result<()> {
    let dr = find_darwin_rebuild()?;
    run(Command::new("sudo")
        .args([
            &dr,
            "switch",
            "--rollback",
            "--flake",
            &format!(".#{hostname}"),
        ])
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
