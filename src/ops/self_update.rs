use std::fs;
use std::process::Command;

use anyhow::{bail, Context, Result};
use console::style;

use crate::output;

const REPO: &str = "https://github.com/styrene-lab/nex";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn run() -> Result<()> {
    let target = detect_target()?;
    let current = CURRENT_VERSION;

    output::status("checking for updates...");

    let latest = fetch_latest_version()?;

    if latest == current {
        println!(
            "  {} nex {} is already the latest",
            style("✓").green().bold(),
            current
        );
        return Ok(());
    }

    println!(
        "  nex {} -> {}",
        style(current).yellow(),
        style(&latest).green()
    );

    let url = format!("{REPO}/releases/download/v{latest}/nex-{target}.tar.gz");

    output::status(&format!("downloading nex {latest}..."));

    let tmpdir = tempfile::tempdir().context("failed to create temp dir")?;
    let tarball = tmpdir.path().join("nex.tar.gz");

    let dl = Command::new("curl")
        .args(["-fsSL", &url, "-o"])
        .arg(&tarball)
        .status()
        .context("failed to run curl")?;
    if !dl.success() {
        bail!("download failed — release may not exist for {target}");
    }

    let extract = Command::new("tar")
        .args(["-xzf"])
        .arg(&tarball)
        .args(["-C"])
        .arg(tmpdir.path())
        .status()
        .context("failed to extract archive")?;
    if !extract.success() {
        bail!("archive extraction failed");
    }

    let new_binary = tmpdir.path().join("nex");
    if !new_binary.exists() {
        bail!("binary not found in release archive");
    }

    // Find where the current binary lives and replace it
    let current_exe = std::env::current_exe().context("could not determine current binary path")?;
    let real_path = fs::canonicalize(&current_exe).unwrap_or_else(|_| current_exe.clone());
    let needs_sudo = !is_writable(&real_path);

    output::status(&format!("replacing {}...", real_path.display()));

    if needs_sudo {
        // Use sudo to replace in-place (e.g. /usr/local/bin owned by root)
        let status = Command::new("sudo")
            .args(["cp", "-f"])
            .arg(&new_binary)
            .arg(&real_path)
            .status()
            .context("failed to run sudo")?;
        if !status.success() {
            bail!("sudo cp failed — could not replace {}", real_path.display());
        }
        let _ = Command::new("sudo")
            .args(["chmod", "+x"])
            .arg(&real_path)
            .status();
    } else {
        let backup = real_path.with_extension("old");
        if let Err(e) = fs::rename(&real_path, &backup) {
            bail!("could not replace {} — {e}", real_path.display());
        }

        if let Err(e) = fs::copy(&new_binary, &real_path) {
            let _ = fs::rename(&backup, &real_path);
            bail!("failed to install new binary: {e}");
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&real_path, fs::Permissions::from_mode(0o755));
        }

        let _ = fs::remove_file(&backup);
    }

    // Verify the new binary works
    let verify = Command::new(&real_path).arg("--version").output();
    if !verify.map(|o| o.status.success()).unwrap_or(false) {
        output::warn("new binary may not be working — verify with `nex --version`");
    }

    println!(
        "\n  {} nex updated {} -> {}",
        style("✓").green().bold(),
        current,
        style(&latest).green()
    );

    Ok(())
}

/// Check if a path (or its parent directory) is writable by the current user.
fn is_writable(path: &std::path::Path) -> bool {
    if path.exists() {
        // Can we write to the file itself?
        std::fs::OpenOptions::new().write(true).open(path).is_ok()
    } else if let Some(parent) = path.parent() {
        // Can we create files in the parent?
        let test = parent.join(".nex-write-test");
        if std::fs::write(&test, b"").is_ok() {
            let _ = std::fs::remove_file(&test);
            true
        } else {
            false
        }
    } else {
        false
    }
}

fn detect_target() -> Result<String> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    let target = match (arch, os) {
        ("aarch64", "macos") => "aarch64-apple-darwin",
        ("x86_64", "macos") => "x86_64-apple-darwin",
        ("aarch64", "linux") => "aarch64-unknown-linux-gnu",
        ("x86_64", "linux") => "x86_64-unknown-linux-gnu",
        _ => bail!("unsupported platform: {arch}-{os}"),
    };

    Ok(target.to_string())
}

fn fetch_latest_version() -> Result<String> {
    // GitHub API: get latest release tag
    let output = Command::new("curl")
        .args([
            "-fsSL",
            "-H",
            "Accept: application/vnd.github+json",
            &format!("https://api.github.com/repos/styrene-lab/nex/releases/latest"),
        ])
        .output()
        .context("failed to query GitHub releases")?;

    if !output.status.success() {
        bail!("could not fetch latest release from GitHub");
    }

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("invalid JSON from GitHub API")?;

    let tag = json
        .get("tag_name")
        .and_then(|v| v.as_str())
        .context("no tag_name in release")?;

    // Strip leading 'v'
    let version = tag.strip_prefix('v').unwrap_or(tag);
    Ok(version.to_string())
}
