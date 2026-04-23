use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use console::style;

use crate::discover;
use crate::output;

const NIXOS_ISO_URL: &str =
    "https://channels.nixos.org/nixos-24.11/latest-nixos-minimal-x86_64-linux.iso";
const NIXOS_ISO_NAME: &str = "nixos-minimal-x86_64.iso";

/// Run `nex forge` — build a bootable NixOS installer USB.
/// If profile is None, builds a generic styx installer.
pub fn run(
    profile_ref: Option<&str>,
    hostname: Option<&str>,
    disk: Option<&str>,
    output_dir: Option<&Path>,
    dry_run: bool,
) -> Result<()> {
    let is_styx = profile_ref.is_none();
    let label = if is_styx { "styx" } else { "nex forge" };

    println!();
    println!("  {} — build NixOS installer", style(label).bold());
    println!();

    let hostname = hostname.unwrap_or("nixos");
    let bundle_name = profile_ref
        .map(|r| r.replace('/', "_"))
        .unwrap_or_else(|| "styx".to_string());

    // Resolve output directory
    let bundle_dir = match output_dir {
        Some(p) => p.to_path_buf(),
        None => std::env::temp_dir().join("nex-forge").join(&bundle_name),
    };

    if dry_run {
        output::dry_run(&format!(
            "would build installer at {}",
            bundle_dir.display()
        ));
        if let Some(p) = profile_ref {
            output::dry_run(&format!("profile: {p}"));
        } else {
            output::dry_run("mode: generic styx installer (no profile)");
        }
        output::dry_run(&format!("hostname default: {hostname}"));
        if let Some(d) = disk {
            output::dry_run(&format!("would flash to: {d}"));
        }
        return Ok(());
    }

    // ── 1. Fetch and resolve profile chain ─────────────────────────
    let profile_toml = if let Some(pref) = profile_ref {
        output::status("resolving profile chain...");
        let resolved = resolve_profile_chain(pref)?;
        println!(
            "  {} profile: {} ({})",
            style("✓").green().bold(),
            style(pref).cyan(),
            if resolved.chain.len() > 1 {
                format!("{} layers merged", resolved.chain.len())
            } else {
                "standalone".to_string()
            }
        );
        for layer in &resolved.chain {
            println!("    {} {}", style("↳").dim(), style(layer).dim());
        }
        Some(resolved.merged)
    } else {
        println!(
            "  {} generic styx installer (no profile baked in)",
            style("i").cyan()
        );
        None
    };

    // ── 2. Create bundle structure ───────────────────────────────────
    std::fs::create_dir_all(&bundle_dir)?;
    let styrene_dir = bundle_dir.join("styrene");
    std::fs::create_dir_all(&styrene_dir)?;

    // ── 3. Download NixOS ISO ────────────────────────────────────────
    let iso_path = bundle_dir.join(NIXOS_ISO_NAME);
    // Validate cached ISO — remove if suspiciously small (partial download)
    if iso_path.exists() {
        let size = std::fs::metadata(&iso_path).map(|m| m.len()).unwrap_or(0);
        if size < 100 * 1024 * 1024 {
            // Less than 100MB is definitely a partial download
            let _ = std::fs::remove_file(&iso_path);
        }
    }

    if iso_path.exists() {
        let size_mb = std::fs::metadata(&iso_path)
            .map(|m| m.len() / (1024 * 1024))
            .unwrap_or(0);
        println!(
            "  {} NixOS ISO (cached, {} MB)",
            style("✓").green().bold(),
            size_mb
        );
    } else {
        output::status("downloading NixOS minimal ISO...");
        download_file(NIXOS_ISO_URL, &iso_path)?;
        let size_mb = std::fs::metadata(&iso_path)?.len() / (1024 * 1024);
        println!("  {} NixOS ISO ({} MB)", style("✓").green().bold(), size_mb);
    }

    // ── 4. Write defaults for polymerize ─────────────────────────────
    let defaults_dir = styrene_dir.join("defaults");
    std::fs::create_dir_all(&defaults_dir)?;
    std::fs::write(defaults_dir.join("hostname"), hostname)?;

    let user = std::env::var("USER").unwrap_or_else(|_| "user".to_string());
    std::fs::write(defaults_dir.join("username"), &user)?;
    std::fs::write(defaults_dir.join("timezone"), "America/New_York")?;

    // ── 5. Write profile into bundle (if specified) ──────────────────
    if let Some(ref toml_content) = profile_toml {
        let profile_dir = styrene_dir.join("profile");
        std::fs::create_dir_all(&profile_dir)?;
        std::fs::write(profile_dir.join("profile.toml"), toml_content)?;
        if let Some(pref) = profile_ref {
            std::fs::write(profile_dir.join("source"), format!("{pref}\n"))?;
        }
    }

    // ── 6. Bundle nex binary for target arch ────────────────────────
    output::status("bundling nex binary for x86_64-linux...");
    let nex_bin_path = styrene_dir.join("nex");
    match fetch_nex_binary(&nex_bin_path) {
        Ok(()) => {
            // Verify it's not a placeholder — check content, not just size
            let content = std::fs::read_to_string(&nex_bin_path).unwrap_or_default();
            let is_placeholder = content.contains("nex binary not available");
            let size = std::fs::metadata(&nex_bin_path)
                .map(|m| m.len())
                .unwrap_or(0);
            if is_placeholder {
                println!(
                    "  {} nex binary is a placeholder ({} bytes)",
                    style("!").red().bold(),
                    size
                );
                println!("    The USB will not have a working installer.");
                println!("    To fix: build nex for Linux and copy to the bundle:");
                println!(
                    "      {}",
                    style(format!("cargo build --release --target x86_64-unknown-linux-gnu && cp target/x86_64-unknown-linux-gnu/release/nex {}", nex_bin_path.display())).cyan()
                );
                println!(
                    "    Or on a Linux machine: {}",
                    style(format!(
                        "cargo build --release && cp target/release/nex {}",
                        nex_bin_path.display()
                    ))
                    .cyan()
                );
                println!();

                let cont = dialoguer::Confirm::new()
                    .with_prompt("  Continue without working nex binary?")
                    .default(false)
                    .interact()?;
                if !cont {
                    bail!("Cannot bundle nex binary for Linux. Build it separately or run forge on a Linux machine.");
                }
            } else {
                println!(
                    "  {} nex binary bundled ({} MB)",
                    style("✓").green().bold(),
                    size / (1024 * 1024)
                );
            }
        }
        Err(e) => {
            println!("  {} Could not fetch nex binary: {e}", style("!").yellow());
            println!("    Build manually and copy to: {}", nex_bin_path.display());
        }
    }

    // ── 7. Write bundle manifest ─────────────────────────────────────
    let manifest = format!(
        "version: 2\n\
         hostname: {hostname}\n\
         profile: {profile}\n\
         arch: x86_64\n\
         styx: {is_styx}\n\
         created: {created}\n",
        profile = profile_ref.unwrap_or("none"),
        created = chrono_now(),
    );
    std::fs::write(bundle_dir.join("bundle.yaml"), manifest)?;

    println!();
    println!(
        "  {} Bundle ready at {}",
        style("✓").green().bold(),
        style(bundle_dir.display()).cyan()
    );
    println!();

    // ── 8. Flash to USB if requested ─────────────────────────────────
    if let Some(device) = disk {
        flash_to_usb(&bundle_dir, &iso_path, device)?;
    } else {
        println!("  To flash to USB:");
        println!();
        if let Some(pref) = profile_ref {
            println!(
                "    {}",
                style(format!(
                    "nex forge {pref} --hostname {hostname} --disk /dev/sdX"
                ))
                .cyan()
            );
        } else {
            println!(
                "    {}",
                style(format!("nex forge --hostname {hostname} --disk /dev/sdX")).cyan()
            );
        }
        println!();
        println!("  On the target machine after booting the USB:");
        println!("    {}", style("sudo ./styrene/nex polymerize").cyan());
    }
    println!();

    Ok(())
}

/// Resolved profile chain — base profiles merged in order.
pub struct ResolvedProfile {
    /// The merged TOML content (base first, overlays applied in order).
    pub merged: String,
    /// The chain of profile refs, from base to leaf.
    pub chain: Vec<String>,
}

/// Recursively resolve a profile's `extends` chain and merge all layers.
/// Base profile values are set first, then each overlay adds/overrides.
pub fn resolve_profile_chain(repo_ref: &str) -> Result<ResolvedProfile> {
    let mut chain: Vec<String> = Vec::new();
    let mut layers: Vec<toml::Value> = Vec::new();

    // Walk the extends chain (leaf → base), collecting layers
    let mut current_ref = Some(repo_ref.to_string());
    while let Some(ref pref) = current_ref {
        // Prevent infinite loops
        if chain.contains(pref) {
            break;
        }
        chain.push(pref.clone());

        let toml_str = fetch_profile_toml(pref)?;
        let value: toml::Value = toml::from_str(&toml_str)
            .with_context(|| format!("invalid profile.toml from {pref}"))?;

        // Check for extends
        current_ref = value
            .get("meta")
            .and_then(|m| m.get("extends"))
            .and_then(|e| e.as_str())
            .map(String::from);

        layers.push(value);
    }

    // Reverse: base first, leaf last
    chain.reverse();
    layers.reverse();

    // Merge: start with base, overlay each subsequent layer
    let mut merged = layers.remove(0);
    for overlay in layers {
        merge_toml(&mut merged, overlay);
    }

    // Serialize back to TOML string
    let merged_str =
        toml::to_string_pretty(&merged).context("failed to serialize merged profile")?;

    Ok(ResolvedProfile {
        merged: merged_str,
        chain,
    })
}

/// Deep-merge TOML values: tables merge recursively, arrays concatenate
/// and deduplicate, scalar values from overlay win.
fn merge_toml(base: &mut toml::Value, overlay: toml::Value) {
    match (base, overlay) {
        (toml::Value::Table(base_table), toml::Value::Table(overlay_table)) => {
            for (key, value) in overlay_table {
                if let Some(base_value) = base_table.get_mut(&key) {
                    merge_toml(base_value, value);
                } else {
                    base_table.insert(key, value);
                }
            }
        }
        (toml::Value::Array(base_arr), toml::Value::Array(overlay_arr)) => {
            // Concatenate arrays, deduplicate by string value
            for item in overlay_arr {
                let dominated = match &item {
                    toml::Value::String(s) => base_arr
                        .iter()
                        .any(|existing| existing.as_str() == Some(s.as_str())),
                    _ => base_arr.contains(&item),
                };
                if !dominated {
                    base_arr.push(item);
                }
            }
        }
        (base, overlay) => {
            // Scalar values: overlay wins
            *base = overlay;
        }
    }
}

/// Fetch profile.toml content from GitHub.
fn fetch_profile_toml(repo_ref: &str) -> Result<String> {
    // Try gh CLI first (private repos)
    if let Ok(output) = Command::new("gh")
        .args([
            "api",
            &format!("repos/{repo_ref}/contents/profile.toml"),
            "-H",
            "Accept: application/vnd.github.raw+json",
        ])
        .output()
    {
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).to_string());
        }
    }

    // Fallback to curl
    let url = format!("https://raw.githubusercontent.com/{repo_ref}/main/profile.toml");
    let output = Command::new("curl")
        .args(["-fsSL", &url])
        .output()
        .context("curl failed")?;

    if !output.status.success() {
        bail!("could not fetch profile.toml from {repo_ref}");
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Download a file with curl, showing progress.
fn download_file(url: &str, dest: &Path) -> Result<()> {
    let status = Command::new("curl")
        .args([
            "-fSL",
            "--progress-bar",
            "-o",
            &dest.display().to_string(),
            url,
        ])
        .status()
        .context("failed to download")?;

    if !status.success() {
        bail!("download failed: {url}");
    }
    Ok(())
}

/// Bundle nex for the target architecture (currently x86_64-linux).
/// TODO: support aarch64-linux for ARM targets.
/// Strategy: nix cross-build (works from macOS) → GitHub release → self-copy → placeholder.
fn fetch_nex_binary(dest: &Path) -> Result<()> {
    let _nex_src = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent()?.parent()?.parent().map(|p| p.to_path_buf()));

    // ── Strategy 1: nix cross-build (reliable from macOS if nix is available) ──
    let has_nix = Command::new("nix")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if has_nix {
        // Find the nex source directory (cargo puts the binary in target/release/nex)
        let src_dir = find_nex_source();

        if let Some(ref src) = src_dir {
            println!("    Cross-building via nix...");

            let expr = format!(
                "let pkgs = import <nixpkgs> {{ crossSystem = \"x86_64-linux\"; }}; \
                 src = builtins.path {{ path = {src}; name = \"nex-src\"; \
                   filter = path: type: type != \"unknown\" && !(pkgs.lib.hasSuffix \".sock\" path); }}; \
                 in pkgs.rustPlatform.buildRustPackage {{ \
                   pname = \"nex\"; version = \"0.10.0\"; inherit src; \
                   cargoLock.lockFile = {src}/Cargo.lock; }}",
                src = src.display()
            );
            let build_output = Command::new("nix")
                .args([
                    "build",
                    "--impure",
                    "--no-link",
                    "--print-out-paths",
                    "--expr",
                    &expr,
                ])
                .output();

            if let Ok(output) = build_output {
                // Print build stderr for visibility
                let stderr = String::from_utf8_lossy(&output.stderr);
                for line in stderr.lines() {
                    if !line.is_empty() {
                        println!("    {line}");
                    }
                }

                if output.status.success() {
                    // --print-out-paths outputs the store path on stdout (may have multiple lines)
                    let store_path = String::from_utf8_lossy(&output.stdout)
                        .lines()
                        .filter(|l| l.starts_with("/nix/store/"))
                        .last()
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    let bin_check = Path::new(&store_path).join("bin/nex");
                    if !store_path.is_empty() && bin_check.exists() {
                        // Export the nix closure so it works on target
                        let bundle_dir = dest.parent().unwrap_or(Path::new("/tmp"));
                        let cache_dir = bundle_dir.join("nix-cache");
                        let _ = Command::new("nix")
                            .args([
                                "copy",
                                "--to",
                                &format!("file://{}", cache_dir.display()),
                                &store_path,
                            ])
                            .status();

                        // Write a bootstrap script as the "nex" entry point
                        let script = format!(
                            "#!/usr/bin/env bash\n\
                             set -euo pipefail\n\
                             SD=\"$(cd \"$(dirname \"${{BASH_SOURCE[0]}}\")\" && pwd)\"\n\
                             nix copy --from \"file://$SD/nix-cache\" --all --no-check-sigs 2>/dev/null || true\n\
                             exec {store_path}/bin/nex \"$@\"\n"
                        );
                        std::fs::write(dest, &script)?;
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            let _ = std::fs::set_permissions(
                                dest,
                                std::fs::Permissions::from_mode(0o755),
                            );
                        }
                        return Ok(());
                    }
                }
            }
        }
    }

    // ── Strategy 2: GitHub release download ──
    let target = "x86_64-unknown-linux-gnu";
    if let Ok(output) = Command::new("gh")
        .args([
            "api",
            "repos/styrene-lab/nex/releases/latest",
            "-q",
            &format!(".assets[] | select(.name | contains(\"{target}\")) | .browser_download_url"),
        ])
        .output()
    {
        if output.status.success() {
            let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !url.is_empty() && download_file(&url, dest).is_ok() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755));
                }
                return Ok(());
            }
        }
    }

    // ── Strategy 3: self-copy (already on Linux) ──
    if crate::discover::detect_platform() == crate::discover::Platform::Linux {
        let self_exe = std::env::current_exe().context("cannot find own binary")?;
        std::fs::copy(&self_exe, dest)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755));
        }
        return Ok(());
    }

    // ── Strategy 4: placeholder (forge will warn) ──
    std::fs::write(
        dest,
        "#!/bin/sh\necho 'nex binary not available for Linux — see forge output for instructions'\nexit 1\n",
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755));
    }
    Ok(())
}

/// Find the nex source directory from the running binary's path.
fn find_nex_source() -> Option<PathBuf> {
    // Check common locations
    let candidates = [
        dirs::home_dir().map(|h| h.join("workspace/styrene-labs/nex")),
        std::env::current_dir().ok(),
    ];
    for candidate in candidates.into_iter().flatten() {
        if candidate.join("Cargo.toml").exists() && candidate.join("src/main.rs").exists() {
            // Verify it's the nex crate
            if let Ok(content) = std::fs::read_to_string(candidate.join("Cargo.toml")) {
                if content.contains("nex-pkg") || content.contains("name = \"nex\"") {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

/// Scaffold a minimal NixOS configuration for the installer.
/// Retained for potential future use (pre-baking configs at forge time).
#[allow(dead_code)]
fn scaffold_nixos_config(config_dir: &Path, hostname: &str, profile_toml: &str) -> Result<()> {
    std::fs::create_dir_all(config_dir)?;

    let user = std::env::var("USER").unwrap_or_else(|_| "user".to_string());
    let system = discover::detect_system();

    // Parse profile to extract linux settings
    let profile: toml::Value = toml::from_str(profile_toml).context("invalid profile.toml")?;

    // flake.nix
    std::fs::write(
        config_dir.join("flake.nix"),
        format!(
            r#"{{
  description = "NixOS configuration — generated by nex forge";

  inputs = {{
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    home-manager = {{
      url = "github:nix-community/home-manager";
      inputs.nixpkgs.follows = "nixpkgs";
    }};
  }};

  outputs = {{ self, nixpkgs, home-manager }}:
  {{
    nixosConfigurations."{hostname}" = nixpkgs.lib.nixosSystem {{
      system = "{system}";
      specialArgs = {{ username = "{user}"; hostname = "{hostname}"; }};
      modules = [
        ./configuration.nix
        ./hardware-configuration.nix
        home-manager.nixosModules.home-manager
        {{
          home-manager = {{
            useGlobalPkgs = true;
            useUserPackages = true;
            backupFileExtension = "backup";
            extraSpecialArgs = {{ username = "{user}"; hostname = "{hostname}"; }};
            users."{user}" = import ./home.nix;
          }};
        }}
      ];
    }};
  }};
}}
"#
        ),
    )?;

    // configuration.nix — system-level config generated from profile
    let mut config_lines = Vec::new();
    config_lines.push("{ pkgs, lib, username, hostname, ... }:".to_string());
    config_lines.push(String::new());
    config_lines.push("{".to_string());
    config_lines.push(format!("  networking.hostName = \"{hostname}\";"));
    config_lines.push(String::new());

    // Nix settings
    config_lines
        .push("  nix.settings.experimental-features = [ \"nix-command\" \"flakes\" ];".to_string());
    config_lines.push("  nixpkgs.config.allowUnfree = true;".to_string());
    config_lines.push(String::new());

    // Boot
    config_lines.push("  boot.loader.systemd-boot.enable = true;".to_string());
    config_lines.push("  boot.loader.efi.canTouchEfiVariables = true;".to_string());
    config_lines.push(String::new());

    // User
    config_lines.push(format!("  users.users.\"{user}\" = {{"));
    config_lines.push("    isNormalUser = true;".to_string());
    config_lines.push(
        "    extraGroups = [ \"wheel\" \"networkmanager\" \"video\" \"audio\" ];".to_string(),
    );
    config_lines.push("    shell = pkgs.bash;".to_string());
    config_lines.push("  };".to_string());
    config_lines.push(String::new());

    // Networking
    config_lines.push("  networking.networkmanager.enable = true;".to_string());
    config_lines.push(String::new());

    // Locale / timezone
    config_lines.push("  # time.timeZone is set at install time by polymerize".to_string());
    config_lines.push("  i18n.defaultLocale = \"en_US.UTF-8\";".to_string());
    config_lines.push(String::new());

    // Generate from [linux] section of profile
    if let Some(linux) = profile.get("linux") {
        generate_linux_config(&mut config_lines, linux);
    }

    config_lines.push("  system.stateVersion = \"25.05\";".to_string());
    config_lines.push("}".to_string());
    config_lines.push(String::new());

    std::fs::write(
        config_dir.join("configuration.nix"),
        config_lines.join("\n"),
    )?;

    // hardware-configuration.nix — placeholder, polymerize will generate the real one
    std::fs::write(
        config_dir.join("hardware-configuration.nix"),
        r#"# Placeholder — polymerize.sh generates the real one via nixos-generate-config
{ config, lib, pkgs, modulesPath, ... }:

{
  imports = [
    (modulesPath + "/installer/scan/not-detected.nix")
  ];
}
"#,
    )?;

    // home.nix — user-level config from profile packages
    let mut home_lines = Vec::new();
    home_lines.push("{ pkgs, username, ... }:".to_string());
    home_lines.push(String::new());
    home_lines.push("{".to_string());
    home_lines.push("  home = {".to_string());
    home_lines.push("    username = username;".to_string());
    home_lines.push("    homeDirectory = \"/home/${username}\";".to_string());
    home_lines.push("    stateVersion = \"25.05\";".to_string());
    home_lines.push("  };".to_string());
    home_lines.push(String::new());
    home_lines.push("  home.sessionPath = [ \"$HOME/.local/bin\" ];".to_string());
    home_lines.push(String::new());

    // Packages from profile
    home_lines.push("  home.packages = with pkgs; [".to_string());
    if let Some(pkgs) = profile
        .get("packages")
        .and_then(|p| p.get("nix"))
        .and_then(|n| n.as_array())
    {
        for pkg in pkgs {
            if let Some(name) = pkg.as_str() {
                home_lines.push(format!("    {name}"));
            }
        }
    }
    home_lines.push("  ];".to_string());
    home_lines.push(String::new());
    home_lines.push("  programs.home-manager.enable = true;".to_string());
    home_lines.push("}".to_string());
    home_lines.push(String::new());

    std::fs::write(config_dir.join("home.nix"), home_lines.join("\n"))?;

    Ok(())
}

/// Generate NixOS config lines from the [linux] section of a profile.
/// Public so `polymerize` can reuse the same generation logic.
pub fn generate_linux_config(lines: &mut Vec<String>, linux: &toml::Value) {
    // Desktop environment
    if let Some(de) = linux.get("desktop").and_then(|v| v.as_str()) {
        match de {
            "gnome" => {
                lines.push("  # Desktop: GNOME".to_string());
                lines.push("  services.xserver.enable = true;".to_string());
                lines.push("  services.xserver.displayManager.gdm.enable = true;".to_string());
                lines.push("  services.xserver.desktopManager.gnome.enable = true;".to_string());
            }
            "kde" | "plasma" => {
                lines.push("  # Desktop: KDE Plasma".to_string());
                lines.push("  services.desktopManager.plasma6.enable = true;".to_string());
                lines.push("  services.displayManager.sddm.enable = true;".to_string());
                lines.push("  services.displayManager.sddm.wayland.enable = true;".to_string());
            }
            "cosmic" => {
                lines.push("  # Desktop: COSMIC".to_string());
                lines.push("  services.desktopManager.cosmic.enable = true;".to_string());
                lines.push("  services.displayManager.cosmic-greeter.enable = true;".to_string());
            }
            _ => {}
        }
        lines.push(String::new());
    }

    // GPU
    if let Some(gpu) = linux.get("gpu") {
        let driver = gpu.get("driver").and_then(|v| v.as_str()).unwrap_or("");
        let lib32 = gpu.get("32bit").and_then(|v| v.as_bool()).unwrap_or(false);
        let _vulkan = gpu.get("vulkan").and_then(|v| v.as_bool()).unwrap_or(true);
        let vaapi = gpu.get("vaapi").and_then(|v| v.as_bool()).unwrap_or(false);
        let opencl = gpu.get("opencl").and_then(|v| v.as_bool()).unwrap_or(false);

        // Multiple drivers can be specified comma-separated: "amdgpu,nvidia"
        let drivers: Vec<&str> = driver.split(',').map(|d| d.trim()).collect();

        lines.push("  hardware.graphics.enable = true;".to_string());
        if lib32 {
            lines.push("  hardware.graphics.enable32Bit = true;".to_string());
        }

        let mut video_drivers: Vec<&str> = Vec::new();
        let mut extra_packages: Vec<&str> = Vec::new();

        for drv in &drivers {
            match *drv {
                "amdgpu" => {
                    lines.push("  # GPU: AMD".to_string());
                    lines.push("  hardware.amdgpu.initrd.enable = true;".to_string());
                    if opencl {
                        lines.push("  hardware.amdgpu.opencl.enable = true;".to_string());
                    }
                    if vaapi {
                        extra_packages.push("libva-vdpau-driver");
                    }
                }
                "nvidia" => {
                    let nvidia_open = gpu
                        .get("nvidia_open")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true);
                    lines.push("  # GPU: NVIDIA".to_string());
                    video_drivers.push("nvidia");
                    lines.push("  hardware.nvidia.modesetting.enable = true;".to_string());
                    lines.push(format!(
                        "  hardware.nvidia.open = {};",
                        if nvidia_open { "true" } else { "false" }
                    ));
                }
                "nouveau" => {
                    lines.push("  # GPU: NVIDIA (open-source nouveau)".to_string());
                    video_drivers.push("nouveau");
                }
                "intel" => {
                    lines.push("  # GPU: Intel".to_string());
                    // Intel i915 is loaded automatically by the kernel
                    if vaapi {
                        extra_packages.push("intel-media-driver");
                    }
                }
                "" => {} // no driver specified, just enable graphics
                other => {
                    lines.push(format!("  # GPU: {other}"));
                }
            }
        }

        if !video_drivers.is_empty() {
            let drivers_str = video_drivers
                .iter()
                .map(|d| format!("\"{d}\""))
                .collect::<Vec<_>>()
                .join(" ");
            lines.push(format!(
                "  services.xserver.videoDrivers = [ {drivers_str} ];"
            ));
        }

        if !extra_packages.is_empty() {
            lines.push("  hardware.graphics.extraPackages = with pkgs; [".to_string());
            for pkg in &extra_packages {
                lines.push(format!("    {pkg}"));
            }
            lines.push("  ];".to_string());
        }
        lines.push(String::new());
    }

    // Audio
    if let Some(audio) = linux.get("audio") {
        let backend = audio
            .get("backend")
            .and_then(|v| v.as_str())
            .unwrap_or("pipewire");
        let low_latency = audio
            .get("low_latency")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let bluetooth = audio
            .get("bluetooth")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        lines.push("  # Audio".to_string());
        if backend == "pipewire" {
            lines.push("  services.pipewire = {".to_string());
            lines.push("    enable = true;".to_string());
            lines.push("    alsa.enable = true;".to_string());
            lines.push("    alsa.support32Bit = true;".to_string());
            lines.push("    pulse.enable = true;".to_string());
            if low_latency {
                lines.push("    extraConfig.pipewire.\"92-low-latency\" = {".to_string());
                lines.push("      \"context.properties\" = { \"default.clock.rate\" = 48000; \"default.clock.quantum\" = 64; };".to_string());
                lines.push("    };".to_string());
            }
            lines.push("  };".to_string());
        }
        if bluetooth {
            lines.push("  hardware.bluetooth.enable = true;".to_string());
            lines.push("  hardware.bluetooth.powerOnBoot = true;".to_string());
        }
        lines.push(String::new());
    }

    // Gaming
    if let Some(gaming) = linux.get("gaming") {
        let steam = gaming
            .get("steam")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let gamemode = gaming
            .get("gamemode")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let gamescope = gaming
            .get("gamescope")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let controllers = gaming
            .get("controllers")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let mangohud = gaming
            .get("mangohud")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let _proton_ge = gaming
            .get("proton_ge")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        lines.push("  # Gaming".to_string());
        if steam {
            lines.push("  programs.steam = {".to_string());
            lines.push("    enable = true;".to_string());
            lines.push(format!(
                "    gamescopeSession.enable = {};",
                if gamescope { "true" } else { "false" }
            ));
            lines.push("  };".to_string());
        }
        if gamemode {
            lines.push("  programs.gamemode.enable = true;".to_string());
        }
        if controllers {
            lines.push("  hardware.steam-hardware.enable = true;".to_string());
        }

        let mut pkgs = Vec::new();
        if mangohud {
            pkgs.push("mangohud");
        }
        // proton-ge-bin is installed via Steam's compatibility tools, not as a system package
        // if proton_ge { pkgs.push("proton-ge-bin"); }
        if !pkgs.is_empty() {
            lines.push("  environment.systemPackages = with pkgs; [".to_string());
            for p in &pkgs {
                lines.push(format!("    {p}"));
            }
            lines.push("  ];".to_string());
        }
        lines.push(String::new());
    }

    // GNOME dconf settings (via home-manager)
    if let Some(gnome) = linux.get("gnome") {
        lines.push("  # GNOME settings (applied via dconf in home-manager)".to_string());

        let dark = gnome
            .get("dark_mode")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if dark {
            // NixOS-level setting for GNOME dark mode
            lines.push("  environment.sessionVariables.GTK_THEME = \"Adwaita:dark\";".to_string());
        }

        // Favorite apps for the GNOME dock (via dconf)
        if let Some(favs) = gnome.get("favorite_apps").and_then(|v| v.as_array()) {
            let apps: Vec<String> = favs
                .iter()
                .filter_map(|v| v.as_str())
                .map(|s| format!("'{s}'"))
                .collect();
            if !apps.is_empty() {
                // This needs to go in home-manager's dconf settings, but since we're
                // generating system-level config here, we use environment.etc to write
                // a dconf profile that gets applied on login.
                lines.push("  # GNOME favorite apps — written as dconf db override".to_string());
                let apps_str = apps.join(", ");
                lines.push(format!(
                    "  environment.etc.\"dconf/db/local.d/01-nex-favorites\".text = ''",
                ));
                lines.push("    [org/gnome/shell]".to_string());
                lines.push(format!("    favorite-apps=[{apps_str}]"));
                if dark {
                    lines.push(String::new());
                    lines.push("    [org/gnome/desktop/interface]".to_string());
                    lines.push("    color-scheme='prefer-dark'".to_string());
                    lines.push("    gtk-theme='Adwaita-dark'".to_string());
                }
                lines.push("  '';".to_string());
                // Need dconf update to apply
                lines.push("  system.activationScripts.dconf-update = \"dconf update 2>/dev/null || true\";".to_string());
            }
        }

        // Extensions
        if let Some(exts) = gnome.get("extensions").and_then(|v| v.as_array()) {
            let ext_pkgs: Vec<&str> = exts.iter().filter_map(|v| v.as_str()).collect();
            if !ext_pkgs.is_empty() {
                // NixOS module system merges multiple environment.systemPackages declarations
                lines.push(
                    "  environment.systemPackages = with pkgs.gnomeExtensions; [".to_string(),
                );
                for ext in &ext_pkgs {
                    lines.push(format!("    {ext}"));
                }
                lines.push("  ];".to_string());
            }
        }

        lines.push(String::new());
    }

    // COSMIC desktop settings
    if let Some(cosmic) = linux.get("cosmic") {
        lines.push("  # COSMIC desktop settings".to_string());

        let dark = cosmic
            .get("dark_mode")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let autohide = cosmic
            .get("dock_autohide")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // COSMIC uses RON config files in ~/.config/cosmic/
        // Write as /etc/skel entries so new users get them on first login.
        lines.push(format!(
            "  environment.etc.\"skel/.config/cosmic/com.system76.CosmicTheme.Mode/v1/is-dark\".text = \"{}\";",
            dark
        ));

        if autohide {
            lines.push(
                "  environment.etc.\"skel/.config/cosmic/com.system76.CosmicPanel.Dock/v1/autohide\".text = \"true\";".to_string()
            );
        }

        if let Some(favs) = cosmic.get("dock_favorites").and_then(|v| v.as_array()) {
            let fav_list: Vec<String> = favs
                .iter()
                .filter_map(|v| v.as_str())
                .map(|s| {
                    if s.ends_with(".desktop") {
                        s.to_string()
                    } else {
                        format!("{s}.desktop")
                    }
                })
                .collect();
            if !fav_list.is_empty() {
                // Inner quotes must be escaped for Nix: \" inside "..."
                let ron = fav_list
                    .iter()
                    .map(|f| format!("\\\"{f}\\\""))
                    .collect::<Vec<_>>()
                    .join(", ");
                lines.push(format!(
                    "  environment.etc.\"skel/.config/cosmic/com.system76.CosmicAppList/v1/favorites\".text = \"[{ron}]\";",
                ));
            }
        }

        lines.push(String::new());
    }
}

/// Generate a legacy polymerize.sh installer script.
/// Superseded by `nex polymerize` but retained for non-nex environments.
#[allow(dead_code)]
fn generate_polymerize(hostname: &str, profile_ref: &str) -> String {
    format!(
        r##"#!/usr/bin/env bash
# polymerize.sh — NixOS installer generated by nex forge
# Profile: {profile_ref}
# Hostname: {hostname}
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${{BASH_SOURCE[0]}}")" && pwd)"
CONFIG_DIR="$SCRIPT_DIR/nixos-config"
NEX_DIR="$SCRIPT_DIR/nex"

echo "╔══════════════════════════════════════════════════════╗"
echo "║  nex forge — NixOS installer                        ║"
echo "║  Profile: {profile_ref}"
echo "║  Hostname: {hostname}"
echo "╚══════════════════════════════════════════════════════╝"
echo ""

# ── Disk selection ────────────────────────────────────────────────────
echo "Available disks:"
echo ""
lsblk -d -o NAME,SIZE,MODEL,TRAN | grep -v "loop\|sr\|ram"
echo ""
read -rp "Target disk (e.g. sda, nvme0n1): " TARGET_DISK
DISK="/dev/$TARGET_DISK"

if [ ! -b "$DISK" ]; then
    echo "Error: $DISK is not a block device"
    exit 1
fi

echo ""
echo "WARNING: This will ERASE ALL DATA on $DISK"
read -rp "Type 'yes' to continue: " CONFIRM
if [ "$CONFIRM" != "yes" ]; then
    echo "Aborted."
    exit 1
fi

# ── Partition ─────────────────────────────────────────────────────────
echo ""
echo ">>> Partitioning $DISK..."

# Detect NVMe vs SATA partition naming
if [[ "$DISK" == *nvme* ]] || [[ "$DISK" == *mmcblk* ]]; then
    PART_PREFIX="${{DISK}}p"
else
    PART_PREFIX="${{DISK}}"
fi

parted "$DISK" --script -- \
    mklabel gpt \
    mkpart ESP fat32 1MiB 512MiB \
    set 1 esp on \
    mkpart root ext4 512MiB 100%

sleep 1

mkfs.fat -F32 "${{PART_PREFIX}}1"
mkfs.ext4 -F "${{PART_PREFIX}}2"

# ── Mount ─────────────────────────────────────────────────────────────
echo ">>> Mounting filesystems..."
mount "${{PART_PREFIX}}2" /mnt
mkdir -p /mnt/boot
mount "${{PART_PREFIX}}1" /mnt/boot

# ── Generate hardware config ─────────────────────────────────────────
echo ">>> Generating hardware-configuration.nix..."
nixos-generate-config --root /mnt --show-hardware-config > "$CONFIG_DIR/hardware-configuration.nix"

# ── Copy config to target ────────────────────────────────────────────
echo ">>> Installing NixOS configuration..."
mkdir -p /mnt/etc/nixos
cp -r "$CONFIG_DIR"/* /mnt/etc/nixos/

# ── Install ───────────────────────────────────────────────────────────
echo ">>> Running nixos-install (this takes a while)..."
nixos-install --flake /mnt/etc/nixos#{hostname} --no-root-passwd

# ── Post-install: install nex and apply profile ──────────────────────
echo ">>> Installing nex and applying profile..."

# Copy nex profile into the installed system for first-boot apply
mkdir -p /mnt/etc/nex-forge
cp -r "$NEX_DIR"/* /mnt/etc/nex-forge/ 2>/dev/null || true

# Create a first-boot service that applies the nex profile
cat > /mnt/etc/nixos/nex-firstboot.sh << 'FIRSTBOOT'
#!/usr/bin/env bash
# Applied by nex forge — runs once on first boot
set -euo pipefail

MARKER="/etc/nex-forge/.applied"
if [ -f "$MARKER" ]; then
    exit 0
fi

echo "nex forge: applying profile on first boot..."

# Install nex if not present
if ! command -v nex &>/dev/null; then
    if command -v nix &>/dev/null; then
        nix profile install github:styrene-lab/nex 2>/dev/null || true
    fi
fi

# Apply the bundled profile
if command -v nex &>/dev/null && [ -f /etc/nex-forge/source ]; then
    PROFILE=$(cat /etc/nex-forge/source | tr -d '[:space:]')
    nex profile apply "$PROFILE" || true
    nex switch || true
fi

touch "$MARKER"
echo "nex forge: first-boot profile applied."
FIRSTBOOT

chmod +x /mnt/etc/nixos/nex-firstboot.sh

# ── Done ──────────────────────────────────────────────────────────────
echo ""
echo "╔══════════════════════════════════════════════════════╗"
echo "║  Installation complete!                              ║"
echo "║                                                      ║"
echo "║  1. Set a root password:  nixos-enter --root /mnt    ║"
echo "║                           passwd                     ║"
echo "║     Set user password:    passwd {user}              ║"
echo "║                                                      ║"
echo "║  2. Reboot:               umount -R /mnt && reboot   ║"
echo "╚══════════════════════════════════════════════════════╝"
"##,
        profile_ref = profile_ref,
        hostname = hostname,
        user = std::env::var("USER").unwrap_or_else(|_| "user".to_string()),
    )
}

/// Flash ISO + bundle to a USB device.
fn flash_to_usb(bundle_dir: &Path, iso_path: &Path, device: &str) -> Result<()> {
    println!();
    println!(
        "  {} Flashing to {}",
        style("!").yellow().bold(),
        style(device).bold()
    );

    // Safety: confirm device exists and is removable
    let is_macos = crate::discover::detect_platform() == crate::discover::Platform::Darwin;

    if is_macos {
        // Verify it's an external disk
        let output = Command::new("diskutil")
            .args(["info", device])
            .output()
            .context("diskutil not found")?;
        let info = String::from_utf8_lossy(&output.stdout);
        if !info.contains("Removable Media") && !info.contains("External") {
            bail!("{device} does not appear to be removable media. Aborting for safety.");
        }
    } else {
        // Linux: check /sys/block/*/removable
        let dev_name = Path::new(device)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        let removable_path = format!("/sys/block/{dev_name}/removable");
        if let Ok(val) = std::fs::read_to_string(&removable_path) {
            if val.trim() != "1" {
                // Check if it's USB via transport
                let transport =
                    std::fs::read_to_string(format!("/sys/block/{dev_name}/device/transport"))
                        .unwrap_or_default();
                if !transport.trim().contains("usb") {
                    bail!(
                        "{device} does not appear to be removable USB media. Aborting for safety."
                    );
                }
            }
        }
    }

    println!(
        "  {} This will ERASE ALL DATA on {}",
        style("WARNING").red().bold(),
        style(device).bold()
    );

    let confirm = dialoguer::Confirm::new()
        .with_prompt("  Continue?")
        .default(false)
        .interact()?;

    if !confirm {
        println!("  Aborted.");
        return Ok(());
    }

    // Unmount
    if is_macos {
        let _ = Command::new("diskutil")
            .args(["unmountDisk", device])
            .status();
    } else {
        // Unmount all partitions
        let _ = Command::new("umount")
            .args([&format!("{device}*")])
            .status();
    }

    // Strategy: dd the ISO raw to the whole disk (preserves hybrid MBR+GPT
    // bootloader), then use sgdisk to append a FAT32 data partition in the
    // free space after the ISO. Works on both macOS and Linux.

    {
        // Unmount all partitions before dd
        if is_macos {
            let _ = Command::new("diskutil")
                .args(["unmountDisk", device])
                .status();
        }

        // dd ISO raw to disk — preserves the hybrid MBR+GPT bootloader
        output::status("writing NixOS ISO to USB (this takes a few minutes)...");

        // macOS: use /dev/rdiskN (raw device) for 10x faster writes
        let dd_target = if is_macos {
            device.replace("/dev/disk", "/dev/rdisk")
        } else {
            device.to_string()
        };

        let dd_status = Command::new("sudo")
            .args([
                "dd",
                &format!("if={}", iso_path.display()),
                &format!("of={dd_target}"),
                "bs=4M",
                "status=progress",
            ])
            .status()
            .context("dd failed")?;

        if !dd_status.success() {
            bail!("failed to write ISO to {device}");
        }
        let _ = Command::new("sync").status();

        // Check for sgdisk (gptfdisk) — required for the data partition
        let has_sgdisk = Command::new("which")
            .arg("sgdisk")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if !has_sgdisk {
            println!();
            println!(
                "  {} ISO written and bootable, but sgdisk not found.",
                style("!").yellow()
            );
            println!("    Install it to add the data partition:");
            if is_macos {
                println!("      brew install gptfdisk");
            } else {
                println!("      nix-shell -p gptfdisk");
            }
            println!("    Then manually:");
            println!("      sudo sgdisk -e {device}");
            println!("      sudo sgdisk -n 4:0:0 -t 4:0700 -c 4:NEXDATA {device}");
            println!("    Or just copy styrene/ onto a separate USB stick.");
            println!();
            return Ok(());
        }

        // Move the backup GPT header to the true end of the disk
        output::status("extending partition table...");
        let _ = Command::new("sudo").args(["sgdisk", "-e", device]).status();

        // Add a FAT32 data partition in the free space after the ISO
        output::status("creating data partition for installer files...");
        let sgdisk_ok = Command::new("sudo")
            .args([
                "sgdisk",
                "-n",
                "4:0:0",
                "-t",
                "4:0700",
                "-c",
                "4:NEXDATA",
                device,
            ])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if !sgdisk_ok {
            println!(
                "  {} Could not create data partition. Copy styrene/ manually.",
                style("!").yellow()
            );
            return Ok(());
        }

        // Re-read partition table
        if is_macos {
            // macOS needs a moment after GPT modification
            std::thread::sleep(std::time::Duration::from_secs(2));
            let _ = Command::new("diskutil")
                .args(["unmountDisk", device])
                .status();
        } else {
            let _ = Command::new("sudo").args(["partprobe", device]).status();
            std::thread::sleep(std::time::Duration::from_secs(2));
        }

        // Determine partition device name
        let part4 = if is_macos {
            format!("{device}s4") // macOS uses s4, not 4
        } else if device.contains("nvme") || device.contains("mmcblk") {
            format!("{device}p4")
        } else {
            format!("{device}4")
        };

        // Format the data partition
        output::status("formatting data partition...");
        if is_macos {
            let _ = Command::new("sudo")
                .args(["newfs_msdos", "-F", "32", "-v", "NEXDATA", &part4])
                .status();
        } else {
            let _ = Command::new("sudo")
                .args(["mkfs.vfat", "-F", "32", "-n", "NEXDATA", &part4])
                .status();
        }

        // Mount and copy styrene files
        output::status("copying installer files...");
        let mount_point = "/tmp/nex-usb-data";
        std::fs::create_dir_all(mount_point)?;

        let mount_ok = if is_macos {
            Command::new("sudo")
                .args(["mount", "-t", "msdos", &part4, mount_point])
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        } else {
            Command::new("sudo")
                .args(["mount", &part4, mount_point])
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        };

        if mount_ok {
            let _ = Command::new("sudo")
                .args([
                    "cp",
                    "-r",
                    &bundle_dir.join("styrene").display().to_string(),
                    &format!("{mount_point}/"),
                ])
                .status();

            if is_macos {
                let _ = Command::new("sudo").args(["umount", mount_point]).status();
                let _ = Command::new("diskutil").args(["eject", device]).status();
            } else {
                let _ = Command::new("sudo").args(["umount", mount_point]).status();
            }

            println!();
            println!(
                "  {} USB ready — bootable NixOS ISO + data partition with installer.",
                style("✓").green().bold()
            );
        } else {
            println!();
            println!(
                "  {} ISO written (bootable) but data partition mount failed.",
                style("!").yellow()
            );
            println!(
                "    Copy styrene/ manually: {}",
                bundle_dir.join("styrene").display()
            );
        }

        println!();
        println!("  Boot from USB, then:");
        println!(
            "    {}",
            style("mkdir -p /tmp/nex && mount -L NEXDATA /tmp/nex").cyan()
        );
        println!(
            "    {}",
            style("sudo /tmp/nex/styrene/nex polymerize --bundle /tmp/nex/styrene").cyan()
        );
    }

    Ok(())
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_toml_tables_deep() {
        let mut base: toml::Value = toml::from_str(
            r#"
            [meta]
            name = "base"
            [packages]
            nix = ["git", "vim"]
            [shell.aliases]
            ls = "ls -la"
        "#,
        )
        .unwrap();

        let overlay: toml::Value = toml::from_str(
            r#"
            [meta]
            name = "overlay"
            description = "added"
            [packages]
            nix = ["vim", "htop"]
            [shell.aliases]
            ll = "ls -l"
        "#,
        )
        .unwrap();

        merge_toml(&mut base, overlay);

        // Overlay scalar wins
        assert_eq!(base["meta"]["name"].as_str().unwrap(), "overlay");
        // Overlay adds new keys
        assert_eq!(base["meta"]["description"].as_str().unwrap(), "added");
        // Arrays concatenate and deduplicate
        let nix = base["packages"]["nix"].as_array().unwrap();
        let names: Vec<&str> = nix.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(names.contains(&"git"));
        assert!(names.contains(&"vim"));
        assert!(names.contains(&"htop"));
        assert_eq!(names.iter().filter(|&&n| n == "vim").count(), 1); // no duplicates
                                                                      // Tables merge recursively — base alias preserved, overlay added
        assert_eq!(base["shell"]["aliases"]["ls"].as_str().unwrap(), "ls -la");
        assert_eq!(base["shell"]["aliases"]["ll"].as_str().unwrap(), "ls -l");
    }

    #[test]
    fn test_merge_toml_overlay_alias_wins() {
        let mut base: toml::Value = toml::from_str(
            r#"
            [shell.aliases]
            clod = "old-command"
        "#,
        )
        .unwrap();

        let overlay: toml::Value = toml::from_str(
            r#"
            [shell.aliases]
            clod = "new-command"
        "#,
        )
        .unwrap();

        merge_toml(&mut base, overlay);
        assert_eq!(
            base["shell"]["aliases"]["clod"].as_str().unwrap(),
            "new-command"
        );
    }

    #[test]
    fn test_merge_toml_empty_overlay() {
        let mut base: toml::Value = toml::from_str(
            r#"
            [packages]
            nix = ["git"]
        "#,
        )
        .unwrap();

        let overlay: toml::Value = toml::from_str("").unwrap();
        merge_toml(&mut base, overlay);

        let nix = base["packages"]["nix"].as_array().unwrap();
        assert_eq!(nix.len(), 1);
    }

    #[test]
    fn test_generate_linux_config_amd() {
        let profile: toml::Value = toml::from_str(
            r#"
            [gpu]
            driver = "amdgpu"
            vulkan = true
            vaapi = true
            opencl = true
            32bit = true
        "#,
        )
        .unwrap();

        let mut lines = Vec::new();
        generate_linux_config(&mut lines, &profile);
        let output = lines.join("\n");

        assert!(output.contains("hardware.graphics.enable = true"));
        assert!(output.contains("hardware.graphics.enable32Bit = true"));
        assert!(output.contains("hardware.amdgpu.initrd.enable = true"));
        assert!(output.contains("hardware.amdgpu.opencl.enable = true"));
        assert!(output.contains("libva-vdpau-driver"));
        assert!(!output.contains("amdvlk"));
    }

    #[test]
    fn test_generate_linux_config_nvidia() {
        let profile: toml::Value = toml::from_str(
            r#"
            [gpu]
            driver = "nvidia"
        "#,
        )
        .unwrap();

        let mut lines = Vec::new();
        generate_linux_config(&mut lines, &profile);
        let output = lines.join("\n");

        assert!(output.contains("hardware.nvidia.modesetting.enable = true"));
        assert!(output.contains("hardware.nvidia.open = true")); // default
        assert!(output.contains("services.xserver.videoDrivers = [ \"nvidia\" ]"));
    }

    #[test]
    fn test_generate_linux_config_nvidia_old_gpu() {
        let profile: toml::Value = toml::from_str(
            r#"
            [gpu]
            driver = "nvidia"
            nvidia_open = false
        "#,
        )
        .unwrap();

        let mut lines = Vec::new();
        generate_linux_config(&mut lines, &profile);
        let output = lines.join("\n");

        assert!(output.contains("hardware.nvidia.open = false"));
    }

    #[test]
    fn test_generate_linux_config_multi_gpu() {
        let profile: toml::Value = toml::from_str(
            r#"
            [gpu]
            driver = "nvidia,intel"
            vaapi = true
        "#,
        )
        .unwrap();

        let mut lines = Vec::new();
        generate_linux_config(&mut lines, &profile);
        let output = lines.join("\n");

        assert!(output.contains("\"nvidia\""));
        assert!(output.contains("intel-media-driver"));
    }

    #[test]
    fn test_generate_linux_config_cosmic() {
        let profile: toml::Value = toml::from_str(
            r#"
            desktop = "cosmic"
        "#,
        )
        .unwrap();

        let mut lines = Vec::new();
        generate_linux_config(&mut lines, &profile);
        let output = lines.join("\n");

        assert!(output.contains("services.desktopManager.cosmic.enable = true"));
        assert!(output.contains("services.displayManager.cosmic-greeter.enable = true"));
    }

    #[test]
    fn test_generate_linux_config_gnome() {
        let profile: toml::Value = toml::from_str(
            r#"
            desktop = "gnome"
        "#,
        )
        .unwrap();

        let mut lines = Vec::new();
        generate_linux_config(&mut lines, &profile);
        let output = lines.join("\n");

        assert!(output.contains("services.xserver.desktopManager.gnome.enable = true"));
    }

    #[test]
    fn test_generate_linux_config_gaming() {
        let profile: toml::Value = toml::from_str(
            r#"
            [gaming]
            steam = true
            gamemode = true
            mangohud = true
            controllers = true
        "#,
        )
        .unwrap();

        let mut lines = Vec::new();
        generate_linux_config(&mut lines, &profile);
        let output = lines.join("\n");

        assert!(output.contains("programs.steam"));
        assert!(output.contains("programs.gamemode.enable = true"));
        assert!(output.contains("mangohud"));
        assert!(!output.contains("proton-ge-bin"));
    }

    #[test]
    fn test_generate_linux_config_cosmic_dock_quoting() {
        let profile: toml::Value = toml::from_str(
            r#"
            [cosmic]
            dark_mode = true
            dock_favorites = ["com.system76.CosmicFiles", "kitty"]
        "#,
        )
        .unwrap();

        let mut lines = Vec::new();
        generate_linux_config(&mut lines, &profile);
        let output = lines.join("\n");

        // Inner quotes must be escaped for Nix
        assert!(output.contains("\\\"com.system76.CosmicFiles.desktop\\\""));
        assert!(output.contains("\\\"kitty.desktop\\\""));
        // Should NOT have unescaped quotes that break Nix
        assert!(!output.contains("= \"[\"com"));
    }

    #[test]
    fn test_generate_linux_config_kde() {
        let profile: toml::Value = toml::from_str(r#"desktop = "kde""#).unwrap();
        let mut lines = Vec::new();
        generate_linux_config(&mut lines, &profile);
        let output = lines.join("\n");
        assert!(output.contains("plasma6.enable = true"));
        assert!(output.contains("sddm.enable = true"));
    }

    #[test]
    fn test_generate_linux_config_plasma_alias() {
        let profile: toml::Value = toml::from_str(r#"desktop = "plasma""#).unwrap();
        let mut lines = Vec::new();
        generate_linux_config(&mut lines, &profile);
        let output = lines.join("\n");
        assert!(output.contains("plasma6.enable = true"));
    }

    #[test]
    fn test_generate_linux_config_nouveau() {
        let profile: toml::Value = toml::from_str(
            r#"
            [gpu]
            driver = "nouveau"
        "#,
        )
        .unwrap();
        let mut lines = Vec::new();
        generate_linux_config(&mut lines, &profile);
        let output = lines.join("\n");
        assert!(output.contains("\"nouveau\""));
        assert!(!output.contains("nvidia.modesetting"));
    }

    #[test]
    fn test_generate_linux_config_intel_vaapi() {
        let profile: toml::Value = toml::from_str(
            r#"
            [gpu]
            driver = "intel"
            vaapi = true
        "#,
        )
        .unwrap();
        let mut lines = Vec::new();
        generate_linux_config(&mut lines, &profile);
        let output = lines.join("\n");
        assert!(output.contains("intel-media-driver"));
        assert!(output.contains("hardware.graphics.enable = true"));
    }

    #[test]
    fn test_generate_linux_config_empty_driver() {
        let profile: toml::Value = toml::from_str(
            r#"
            [gpu]
            driver = ""
        "#,
        )
        .unwrap();
        let mut lines = Vec::new();
        generate_linux_config(&mut lines, &profile);
        let output = lines.join("\n");
        assert!(output.contains("hardware.graphics.enable = true"));
        // Should not crash or generate broken config
        assert!(!output.contains("services.xserver.videoDrivers"));
    }

    #[test]
    fn test_generate_linux_config_audio_bluetooth_only() {
        let profile: toml::Value = toml::from_str(
            r#"
            [audio]
            bluetooth = true
        "#,
        )
        .unwrap();
        let mut lines = Vec::new();
        generate_linux_config(&mut lines, &profile);
        let output = lines.join("\n");
        assert!(output.contains("hardware.bluetooth.enable = true"));
        assert!(output.contains("pipewire")); // default backend
    }

    #[test]
    fn test_generate_linux_config_empty_gaming() {
        let profile: toml::Value = toml::from_str(
            r#"
            [gaming]
        "#,
        )
        .unwrap();
        let mut lines = Vec::new();
        generate_linux_config(&mut lines, &profile);
        let output = lines.join("\n");
        // Should not contain Steam or gamemode if none are true
        assert!(!output.contains("programs.steam"));
        assert!(!output.contains("programs.gamemode"));
    }

    #[test]
    fn test_generate_linux_config_no_sections() {
        let profile: toml::Value = toml::from_str("").unwrap();
        let mut lines = Vec::new();
        generate_linux_config(&mut lines, &profile);
        // Empty profile should generate nothing
        assert!(lines.is_empty());
    }

    #[test]
    fn test_merge_toml_base_arrays_preserved() {
        // Overlay has no packages — base packages should remain
        let mut base: toml::Value = toml::from_str(
            r#"
            [packages]
            nix = ["git", "vim", "eza"]
        "#,
        )
        .unwrap();
        let overlay: toml::Value = toml::from_str(
            r#"
            [meta]
            name = "overlay"
        "#,
        )
        .unwrap();
        merge_toml(&mut base, overlay);
        let nix = base["packages"]["nix"].as_array().unwrap();
        assert_eq!(nix.len(), 3);
    }

    #[test]
    fn test_merge_toml_circular_protection() {
        // resolve_profile_chain handles circular refs via the chain.contains check.
        // We can test the merge function itself handles the same value merged twice.
        let mut base: toml::Value = toml::from_str(
            r#"
            [shell.aliases]
            ls = "eza"
        "#,
        )
        .unwrap();
        let overlay = base.clone();
        merge_toml(&mut base, overlay);
        // Should not duplicate — same value
        assert_eq!(base["shell"]["aliases"]["ls"].as_str().unwrap(), "eza");
    }

    #[test]
    fn test_generate_linux_config_gnome_dark_and_favorites() {
        let profile: toml::Value = toml::from_str(
            r#"
            [gnome]
            dark_mode = true
            favorite_apps = ["firefox.desktop", "kitty.desktop"]
        "#,
        )
        .unwrap();
        let mut lines = Vec::new();
        generate_linux_config(&mut lines, &profile);
        let output = lines.join("\n");
        assert!(output.contains("Adwaita:dark"));
        assert!(output.contains("'firefox.desktop'"));
        assert!(output.contains("color-scheme='prefer-dark'"));
        assert!(output.contains("dconf update"));
    }

    #[test]
    fn test_generate_linux_config_gnome_no_dark() {
        let profile: toml::Value = toml::from_str(
            r#"
            [gnome]
            dark_mode = false
            favorite_apps = ["kitty.desktop"]
        "#,
        )
        .unwrap();
        let mut lines = Vec::new();
        generate_linux_config(&mut lines, &profile);
        let output = lines.join("\n");
        assert!(!output.contains("Adwaita:dark"));
        assert!(!output.contains("color-scheme"));
        assert!(output.contains("'kitty.desktop'"));
    }
}

fn chrono_now() -> String {
    // Simple ISO timestamp without pulling in chrono
    let output = Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_else(|| "unknown".to_string());
    output.trim().to_string()
}
