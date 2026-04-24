use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};

/// Resolve the path to the `nix` binary.
/// Checks PATH first, then well-known locations for freshly installed nix.
pub fn find_nix() -> String {
    // Test if nix is directly available in PATH
    if let Ok(output) = Command::new("nix").arg("--version").output() {
        if output.status.success() {
            return "nix".to_string();
        }
    }

    let candidates = [
        "/nix/var/nix/profiles/default/bin/nix",
        "/run/current-system/sw/bin/nix",
        "/etc/profiles/per-user/default/bin/nix",
    ];
    for path in &candidates {
        if Path::new(path).exists() {
            return path.to_string();
        }
    }

    // Fall back to bare name — will produce a clear "not found" error
    "nix".to_string()
}

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

/// Stage all changes and commit in a git repo. Warns on failure instead of
/// returning an error, since git may not be configured in all environments.
pub fn git_commit(repo: &Path, message: &str) {
    let add = Command::new("git")
        .args(["add", "-A"])
        .current_dir(repo)
        .output();

    match add {
        Ok(o) if o.status.success() => {}
        Ok(o) => {
            eprintln!(
                "  warning: git add failed (exit {}): {}",
                o.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&o.stderr).trim()
            );
            return;
        }
        Err(e) => {
            eprintln!("  warning: could not run git add: {e}");
            return;
        }
    }

    let commit = Command::new("git")
        .args(["commit", "-m", message])
        .current_dir(repo)
        .output();

    match commit {
        Ok(o) if o.status.success() => {}
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            // "nothing to commit" is not a real failure
            if !stderr.contains("nothing to commit") {
                eprintln!(
                    "  warning: git commit failed — please commit manually: {}",
                    stderr.trim()
                );
            }
        }
        Err(e) => {
            eprintln!("  warning: could not run git commit: {e}");
        }
    }
}

/// Validate that a nix package attribute exists in nixpkgs.
pub fn nix_eval_exists(pkg: &str) -> Result<bool> {
    let output = Command::new(find_nix())
        .args(["eval", &format!("nixpkgs#{pkg}.name"), "--raw"])
        .stderr(std::process::Stdio::null())
        .output()
        .context("failed to run nix eval")?;
    Ok(output.status.success())
}

/// Get the version of a nix package from nixpkgs. Returns None if not found.
pub fn nix_eval_version(pkg: &str) -> Result<Option<String>> {
    let output = Command::new(find_nix())
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
    run(Command::new(find_nix()).args(["search", "nixpkgs", query]))
}

/// Resolve the absolute path to darwin-rebuild so sudo can find it.
fn find_darwin_rebuild() -> Result<String> {
    // Check if darwin-rebuild is directly available in PATH
    if let Ok(output) = Command::new("darwin-rebuild").arg("--help").output() {
        if output.status.success() {
            return Ok("darwin-rebuild".to_string());
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

/// Resolve the absolute path to nixos-rebuild.
fn find_nixos_rebuild() -> Result<String> {
    if let Ok(output) = Command::new("nixos-rebuild").arg("--help").output() {
        if output.status.success() {
            return Ok("nixos-rebuild".to_string());
        }
    }

    let candidates = [
        "/run/current-system/sw/bin/nixos-rebuild",
        "/nix/var/nix/profiles/system/sw/bin/nixos-rebuild",
    ];
    for path in &candidates {
        if std::path::Path::new(path).exists() {
            return Ok(path.to_string());
        }
    }

    bail!(
        "nixos-rebuild not found — is NixOS installed?\n\
         hint: run `nex init` first, or see nex.styrene.io"
    )
}

/// Run darwin-rebuild switch (requires sudo for system activation).
/// After a successful switch, re-registers nix .app bundles with LaunchServices
/// so Spotlight displays correct icons.
pub fn darwin_rebuild_switch(repo: &Path, hostname: &str) -> Result<()> {
    ensure_profile_dirs();
    let dr = find_darwin_rebuild()?;
    run(Command::new("sudo")
        .args([&dr, "switch", "--flake", &format!(".#{hostname}")])
        .current_dir(repo))?;
    refresh_app_icons();
    Ok(())
}

/// Run nixos-rebuild switch (requires sudo for system activation).
pub fn nixos_rebuild_switch(repo: &Path, hostname: &str) -> Result<()> {
    ensure_profile_dirs();
    let nr = find_nixos_rebuild()?;
    run(Command::new("sudo")
        .args([&nr, "switch", "--flake", &format!(".#{hostname}")])
        .current_dir(repo))
}

/// Platform-aware system rebuild switch. Dispatches to darwin-rebuild or nixos-rebuild.
pub fn system_rebuild_switch(
    repo: &Path,
    hostname: &str,
    platform: crate::discover::Platform,
) -> Result<()> {
    match platform {
        crate::discover::Platform::Darwin => darwin_rebuild_switch(repo, hostname),
        crate::discover::Platform::Linux => nixos_rebuild_switch(repo, hostname),
    }
}

/// Ensure home-manager profile directories exist.
/// Determinate Nix on fresh macOS doesn't create per-user profile dirs,
/// which causes home-manager to fail with "Could not find suitable profile directory".
/// Also fixes ~/.local ownership if the install script created it as root.
pub fn ensure_profile_dirs() {
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_default();
    if user.is_empty() {
        eprintln!("  warning: USER not set, skipping profile directory setup");
        return;
    }

    // Fix ~/.local ownership — the install script may have created it via sudo
    if let Some(home) = dirs::home_dir() {
        let dot_local = home.join(".local");
        if dot_local.exists() {
            if let Ok(meta) = std::fs::metadata(&dot_local) {
                use std::os::unix::fs::MetadataExt;
                if meta.uid() == 0 {
                    if let Ok(s) = Command::new("sudo")
                        .args(["chown", "-R", &user, &dot_local.to_string_lossy()])
                        .status()
                    {
                        if !s.success() {
                            eprintln!(
                                "  warning: failed to fix ownership of {}",
                                dot_local.display()
                            );
                        }
                    }
                }
            }
        }
    }

    let dirs = [
        format!("/nix/var/nix/profiles/per-user/{user}"),
        dirs::home_dir()
            .map(|h| {
                h.join(".local/state/nix/profiles")
                    .to_string_lossy()
                    .into_owned()
            })
            .unwrap_or_default(),
    ];

    for dir in &dirs {
        if dir.is_empty() {
            continue;
        }
        let path = Path::new(dir);
        if path.exists() {
            continue;
        }
        if dir.starts_with("/nix/") {
            let ok = Command::new("sudo")
                .args(["mkdir", "-p", dir])
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
                && Command::new("sudo")
                    .args(["chown", &user, dir])
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);
            if !ok {
                eprintln!("  warning: failed to create {dir}");
            }
        } else if std::fs::create_dir_all(path).is_err() {
            eprintln!("  warning: failed to create {dir}");
        }
    }
}

/// Re-register nix .app bundles with LaunchServices so Spotlight shows correct icons.
fn refresh_app_icons() {
    let lsregister = "/System/Library/Frameworks/CoreServices.framework/\
                      Frameworks/LaunchServices.framework/Support/lsregister";

    if !Path::new(lsregister).exists() {
        return;
    }

    let app_dirs: Vec<std::path::PathBuf> = [
        dirs::home_dir().map(|h| h.join("Applications/Home Manager Apps")),
        Some(std::path::PathBuf::from("/Applications/Nix Apps")),
    ]
    .into_iter()
    .flatten()
    .filter(|d| d.exists())
    .collect();

    if app_dirs.is_empty() {
        return;
    }

    for dir in &app_dirs {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("app") {
                let _ = Command::new(lsregister).args(["-f"]).arg(&path).output();
            }
        }
    }
}

/// Run darwin-rebuild build (for diff). No sudo needed — build only.
pub fn darwin_rebuild_build(repo: &Path, hostname: &str) -> Result<()> {
    let dr = find_darwin_rebuild()?;
    run(Command::new(&dr)
        .args(["build", "--flake", &format!(".#{hostname}")])
        .current_dir(repo))
}

/// Run nixos-rebuild build (for diff). No sudo needed — build only.
pub fn nixos_rebuild_build(repo: &Path, hostname: &str) -> Result<()> {
    let nr = find_nixos_rebuild()?;
    run(Command::new(&nr)
        .args(["build", "--flake", &format!(".#{hostname}")])
        .current_dir(repo))
}

/// Platform-aware system rebuild build.
pub fn system_rebuild_build(
    repo: &Path,
    hostname: &str,
    platform: crate::discover::Platform,
) -> Result<()> {
    match platform {
        crate::discover::Platform::Darwin => darwin_rebuild_build(repo, hostname),
        crate::discover::Platform::Linux => nixos_rebuild_build(repo, hostname),
    }
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

/// Run nixos-rebuild --rollback (requires sudo for system activation).
pub fn nixos_rebuild_rollback(repo: &Path, hostname: &str) -> Result<()> {
    let nr = find_nixos_rebuild()?;
    run(Command::new("sudo")
        .args([
            &nr,
            "switch",
            "--rollback",
            "--flake",
            &format!(".#{hostname}"),
        ])
        .current_dir(repo))
}

/// Platform-aware system rebuild rollback.
pub fn system_rebuild_rollback(
    repo: &Path,
    hostname: &str,
    platform: crate::discover::Platform,
) -> Result<()> {
    match platform {
        crate::discover::Platform::Darwin => darwin_rebuild_rollback(repo, hostname),
        crate::discover::Platform::Linux => nixos_rebuild_rollback(repo, hostname),
    }
}

/// Run nix flake update.
pub fn nix_flake_update(repo: &Path) -> Result<()> {
    run(Command::new(find_nix())
        .args(["flake", "update"])
        .current_dir(repo))
}

/// Run nix shell for an ephemeral package.
pub fn nix_shell(pkg: &str) -> Result<()> {
    run(Command::new(find_nix()).args(["shell", &format!("nixpkgs#{pkg}")]))
}

/// Show diff between current system and new build.
pub fn nix_diff_closures(repo: &Path) -> Result<()> {
    run(Command::new(find_nix())
        .args([
            "store",
            "diff-closures",
            "/nix/var/nix/profiles/system",
            "./result",
        ])
        .current_dir(repo))
}

/// Garbage collect nix store and remove old generations.
pub fn nix_gc() -> Result<()> {
    let nix = find_nix();
    run(Command::new(&nix).args(["store", "gc"]))?;
    // Remove old generations — nix-collect-garbage -d, using the same bin dir as nix
    let nix_path = std::path::PathBuf::from(&nix);
    if let Some(bin_dir) = nix_path.parent() {
        let gc_bin = bin_dir.join("nix-collect-garbage");
        if gc_bin.exists() {
            run(Command::new(&gc_bin).args(["-d"]))?;
        }
    }
    Ok(())
}
