use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

/// The operating system nex is running on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Darwin,
    Linux,
}

/// Detected Linux desktop environment (if any).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum DesktopEnvironment {
    Gnome,
    Kde,
    Cosmic,
    Other,
    None,
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Platform::Darwin => write!(f, "macOS"),
            Platform::Linux => write!(f, "Linux"),
        }
    }
}

impl std::fmt::Display for DesktopEnvironment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DesktopEnvironment::Gnome => write!(f, "GNOME"),
            DesktopEnvironment::Kde => write!(f, "KDE Plasma"),
            DesktopEnvironment::Cosmic => write!(f, "COSMIC"),
            DesktopEnvironment::Other => write!(f, "other"),
            DesktopEnvironment::None => write!(f, "none"),
        }
    }
}

/// Detect the current platform at runtime (not compile-time).
pub fn detect_platform() -> Platform {
    if runtime_os() == "darwin" {
        Platform::Darwin
    } else {
        Platform::Linux
    }
}

/// Detect the Linux desktop environment from standard env vars.
#[allow(dead_code)]
pub fn detect_desktop_environment() -> DesktopEnvironment {
    if detect_platform() == Platform::Darwin {
        return DesktopEnvironment::None;
    }

    // XDG_CURRENT_DESKTOP is the standard signal (set by display managers / session)
    if let Ok(desktop) = std::env::var("XDG_CURRENT_DESKTOP") {
        let lower = desktop.to_lowercase();
        if lower.contains("gnome") {
            return DesktopEnvironment::Gnome;
        }
        if lower.contains("kde") || lower.contains("plasma") {
            return DesktopEnvironment::Kde;
        }
        if lower.contains("cosmic") {
            return DesktopEnvironment::Cosmic;
        }
        return DesktopEnvironment::Other;
    }

    // Fallback: DESKTOP_SESSION
    if let Ok(session) = std::env::var("DESKTOP_SESSION") {
        let lower = session.to_lowercase();
        if lower.contains("gnome") {
            return DesktopEnvironment::Gnome;
        }
        if lower.contains("plasma") || lower.contains("kde") {
            return DesktopEnvironment::Kde;
        }
        if lower.contains("cosmic") {
            return DesktopEnvironment::Cosmic;
        }
        return DesktopEnvironment::Other;
    }

    // Fallback: check for running processes
    if Command::new("pgrep")
        .arg("gnome-shell")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return DesktopEnvironment::Gnome;
    }
    if Command::new("pgrep")
        .arg("plasmashell")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return DesktopEnvironment::Kde;
    }
    if Command::new("pgrep")
        .arg("cosmic-comp")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return DesktopEnvironment::Cosmic;
    }

    DesktopEnvironment::None
}

/// Check if the system is running NixOS (has /etc/NIXOS marker).
#[allow(dead_code)]
pub fn is_nixos() -> bool {
    Path::new("/etc/NIXOS").exists()
}

/// Auto-detect the nix config repo by walking up from CWD, then checking well-known paths.
/// Finds both nix-darwin (macOS) and NixOS repos.
pub fn find_repo() -> Result<PathBuf> {
    // 1. Walk up from CWD looking for flake.nix with nix system configurations
    if let Ok(cwd) = std::env::current_dir() {
        let mut dir = cwd.as_path();
        loop {
            let flake = dir.join("flake.nix");
            if flake.exists() && is_nex_flake(&flake) {
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
            home.join("nix-config"),
            home.join(".config/nix-darwin"),
            home.join(".config/nixos"),
            // System-level NixOS config (written by polymerize)
            PathBuf::from("/etc/nixos"),
        ];
        for path in &candidates {
            let flake = path.join("flake.nix");
            if flake.exists() && is_nex_flake(&flake) {
                return Ok(path.clone());
            }
        }
    }

    anyhow::bail!("could not find nix config repo (nix-darwin or NixOS)")
}

/// Check if a flake.nix contains darwinConfigurations or nixosConfigurations.
fn is_nex_flake(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .map(|content| {
            content.contains("darwinConfigurations") || content.contains("nixosConfigurations")
        })
        .unwrap_or(false)
}

/// Auto-detect the hostname. Uses scutil on macOS, /etc/hostname or hostname(1) on Linux.
pub fn hostname() -> Result<String> {
    match detect_platform() {
        Platform::Darwin => {
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
        Platform::Linux => {
            // Try /etc/hostname first (most reliable on NixOS)
            if let Ok(name) = std::fs::read_to_string("/etc/hostname") {
                let trimmed = name.trim().to_string();
                if !trimmed.is_empty() {
                    return Ok(trimmed);
                }
            }

            // Fallback to hostname command
            let output = Command::new("hostname")
                .output()
                .context("failed to run hostname")?;

            if !output.status.success() {
                anyhow::bail!("hostname command failed");
            }

            let name = String::from_utf8(output.stdout)
                .context("hostname is not valid UTF-8")?
                .trim()
                .to_string();

            Ok(name)
        }
    }
}

/// Detect the nix system string for this machine.
/// Uses runtime detection so cross-compiled binaries report the correct target.
pub fn detect_system() -> &'static str {
    let os = runtime_os();
    let arch = runtime_arch();
    match (arch, os) {
        ("x86_64", "darwin") => "x86_64-darwin",
        ("aarch64", "darwin") => "aarch64-darwin",
        ("x86_64", "linux") => "x86_64-linux",
        ("aarch64", "linux") => "aarch64-linux",
        _ => {
            // Fallback to compile-time detection
            if cfg!(target_os = "macos") {
                if cfg!(target_arch = "x86_64") {
                    "x86_64-darwin"
                } else {
                    "aarch64-darwin"
                }
            } else {
                if cfg!(target_arch = "x86_64") {
                    "x86_64-linux"
                } else {
                    "aarch64-linux"
                }
            }
        }
    }
}

/// Runtime OS detection via uname.
fn runtime_os() -> &'static str {
    // Check /proc/version first (Linux-only, fast, no subprocess)
    if Path::new("/proc/version").exists() {
        return "linux";
    }
    // macOS has no /proc
    if Path::new("/System/Library").exists() {
        return "darwin";
    }
    // Fallback to compile-time
    if cfg!(target_os = "macos") {
        "darwin"
    } else {
        "linux"
    }
}

/// Runtime architecture detection via uname -m.
fn runtime_arch() -> &'static str {
    if let Ok(output) = Command::new("uname").arg("-m").output() {
        if output.status.success() {
            let arch = String::from_utf8_lossy(&output.stdout);
            let arch = arch.trim();
            return match arch {
                "x86_64" => "x86_64",
                "aarch64" | "arm64" => "aarch64",
                _ => {
                    if cfg!(target_arch = "x86_64") {
                        "x86_64"
                    } else {
                        "aarch64"
                    }
                }
            };
        }
    }
    if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else {
        "aarch64"
    }
}

/// Return the conventional repo directory name for this platform.
pub fn default_repo_name() -> &'static str {
    match detect_platform() {
        Platform::Darwin => "macos-nix",
        Platform::Linux => "nix-config",
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_system_returns_valid_string() {
        let sys = detect_system();
        assert!(
            [
                "x86_64-darwin",
                "aarch64-darwin",
                "x86_64-linux",
                "aarch64-linux"
            ]
            .contains(&sys),
            "detect_system returned unexpected: {sys}"
        );
    }

    #[test]
    fn test_detect_platform_consistent_with_system() {
        let platform = detect_platform();
        let system = detect_system();
        match platform {
            Platform::Darwin => assert!(system.ends_with("-darwin")),
            Platform::Linux => assert!(system.ends_with("-linux")),
        }
    }

    #[test]
    fn test_runtime_arch_returns_known() {
        let arch = runtime_arch();
        assert!(
            ["x86_64", "aarch64"].contains(&arch),
            "runtime_arch returned unexpected: {arch}"
        );
    }

    #[test]
    fn test_platform_display() {
        assert_eq!(format!("{}", Platform::Darwin), "macOS");
        assert_eq!(format!("{}", Platform::Linux), "Linux");
    }

    #[test]
    fn test_default_repo_name() {
        let name = default_repo_name();
        assert!(name == "macos-nix" || name == "nix-config");
    }
}
