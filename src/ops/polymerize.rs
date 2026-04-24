//! Interactive NixOS installer — runs on the target machine after booting from USB.
//!
//! `nex polymerize` walks the user through system setup: network, hostname,
//! user account, disk, timezone, and optional nex profile.  It then
//! partitions, generates hardware config, lays down a NixOS flake, and
//! runs `nixos-install`.
//!
//! When launched from a nex-forge USB, pre-baked defaults (hostname,
//! profile, etc.) are loaded from the bundle and offered as defaults the
//! user can accept or override.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use console::style;

// ── Drop guard for WiFi credential cleanup ──────────────────────────────

mod scopeguard {
    /// Ensures a file is removed when dropped, even on early return or panic.
    pub struct WpaCleanup<'a>(pub &'a std::path::Path);

    impl Drop for WpaCleanup<'_> {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(self.0);
        }
    }
}

// ── Bundle defaults (written by `nex forge`) ─────────────────────────────

#[derive(Default)]
struct Defaults {
    hostname: Option<String>,
    username: Option<String>,
    timezone: Option<String>,
    profile_ref: Option<String>,
    profile_toml: Option<String>,
}

fn load_defaults(bundle: Option<&Path>) -> Defaults {
    let dir = match resolve_bundle_dir(bundle) {
        Some(d) => d,
        None => return Defaults::default(),
    };

    let read = |name: &str| {
        std::fs::read_to_string(dir.join(name))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };

    Defaults {
        hostname: read("defaults/hostname"),
        username: read("defaults/username"),
        timezone: read("defaults/timezone"),
        profile_ref: read("profile/source").or_else(|| read("nex/source")),
        profile_toml: read("profile/profile.toml").or_else(|| read("nex/profile.toml")),
    }
}

/// Look for the styrene/ bundle dir — explicit arg, or auto-detect from USB mounts.
fn resolve_bundle_dir(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        if p.exists() {
            return Some(p.to_path_buf());
        }
    }

    // Auto-detect: scan common mount points for styrene/ bundle
    let candidates = [
        "/iso/styrene", // NixOS live ISO mounts at /iso
        "/iso",         // files might be at ISO root
        "/mnt/styrene",
        "/mnt",
        "/tmp/nex/styrene", // manual mount point from docs
        "/tmp/nex",
        "/run/media",
    ];
    for c in &candidates {
        let p = PathBuf::from(c);
        if !p.exists() {
            continue;
        }
        // Check for any sign of a nex bundle
        if p.join("profile").exists()
            || p.join("defaults").exists()
            || p.join("nex").is_file()
            || p.join("profile.toml").exists()
            || p.join("styrene/profile").exists()
        {
            // If this dir has a styrene/ subdir, use that
            if p.join("styrene/profile").exists() || p.join("styrene/defaults").exists() {
                return Some(p.join("styrene"));
            }
            return Some(p);
        }
    }

    // Check /run/media/*/styrene (live ISO automounts)
    if let Ok(entries) = std::fs::read_dir("/run/media") {
        for entry in entries.flatten() {
            let s = entry.path().join("styrene");
            if s.exists() {
                return Some(s);
            }
        }
    }

    None
}

// ── Main entry point ─────────────────────────────────────────────────────

pub fn run(bundle: Option<&Path>) -> Result<()> {
    // Must be root
    if !running_as_root() {
        bail!("nex polymerize must be run as root (use sudo)");
    }

    let defaults = load_defaults(bundle);

    println!();
    println!(
        "  {}",
        style("╔══════════════════════════════════════════════════════╗").cyan()
    );
    println!(
        "  {}",
        style("║          nex polymerize — NixOS installer            ║").cyan()
    );
    println!(
        "  {}",
        style("╚══════════════════════════════════════════════════════╝").cyan()
    );

    if let Some(ref p) = defaults.profile_ref {
        println!(
            "  {} Bundled profile: {}",
            style("i").cyan(),
            style(p).bold()
        );
    } else {
        println!(
            "  {} No bundled profile — generic styx install",
            style("i").cyan()
        );
    }
    println!();

    // ── 1. Network ───────────────────────────────────────────────────
    step_network()?;

    // ── 2. Hostname ──────────────────────────────────────────────────
    let hostname = step_hostname(&defaults)?;

    // ── 3. User account ──────────────────────────────────────────────
    let username = step_username(&defaults)?;

    // ── 4. Timezone ──────────────────────────────────────────────────
    let timezone = step_timezone(&defaults)?;

    // ── 5. Disk ──────────────────────────────────────────────────────
    let disk = step_disk()?;

    // ── 6. Profile ───────────────────────────────────────────────────
    let (profile_ref, profile_toml) = step_profile(&defaults)?;

    // ── Confirm ──────────────────────────────────────────────────────
    println!();
    println!("  {}", style("── Summary ──").bold());
    println!("  Hostname:  {}", style(&hostname).cyan());
    println!("  User:      {}", style(&username).cyan());
    println!("  Timezone:  {}", style(&timezone).cyan());
    println!("  Disk:      {}", style(&disk).cyan());
    if let Some(ref p) = profile_ref {
        println!("  Profile:   {}", style(p).cyan());
    } else {
        println!("  Profile:   {}", style("none (base NixOS)").dim());
    }
    // Warn about special disk layouts
    check_disk_for_special_layouts(&disk);

    println!();

    let confirm = dialoguer::Confirm::new()
        .with_prompt("  Proceed with installation? (THIS WILL ERASE THE DISK)")
        .default(false)
        .interact()?;

    if !confirm {
        println!("  Aborted.");
        return Ok(());
    }

    // ── Execute ──────────────────────────────────────────────────────
    println!();
    exec_partition(&disk)?;
    exec_mount(&disk)?;
    exec_generate_hardware(&username)?;
    exec_write_config(
        &hostname,
        &username,
        &timezone,
        &disk,
        profile_toml.as_deref(),
    )?;
    exec_install(&hostname, &username)?;
    exec_chown_config(&username)?;
    exec_set_passwords(&username)?;

    // Write nex config so future `nex` commands find the installed repo
    exec_write_nex_config(&hostname, &username)?;

    println!();
    println!(
        "  {}",
        style("╔══════════════════════════════════════════════════════╗").green()
    );
    println!(
        "  {}",
        style("║          Installation complete!                       ║").green()
    );
    println!(
        "  {}",
        style("║                                                       ║").green()
    );
    println!(
        "  {}",
        style("║  Reboot:   umount -R /mnt && reboot                   ║").green()
    );
    println!(
        "  {}",
        style("╚══════════════════════════════════════════════════════╝").green()
    );
    println!();

    Ok(())
}

// ── Interactive steps ────────────────────────────────────────────────────

fn step_network() -> Result<()> {
    println!("  {}", style("── Network ──").bold());

    // Check if we already have connectivity (ethernet, pre-configured WiFi, etc.)
    let has_net = Command::new("ping")
        .args(["-c1", "-W2", "1.1.1.1"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if has_net {
        println!(
            "  {} Network connected (ethernet or pre-configured)",
            style("✓").green().bold()
        );
        println!();
        return Ok(());
    }

    // Try bringing up ethernet via DHCP before falling back to WiFi
    let _ = Command::new("systemctl")
        .args(["start", "NetworkManager"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    std::thread::sleep(std::time::Duration::from_secs(2));

    // Re-check after NM start (ethernet may have auto-connected)
    let has_net = Command::new("ping")
        .args(["-c1", "-W2", "1.1.1.1"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if has_net {
        println!(
            "  {} Network connected (ethernet)",
            style("✓").green().bold()
        );
        println!();
        return Ok(());
    }

    println!("  No wired connection detected. Scanning for WiFi...");
    println!();

    // Bring up wlan interface
    let _ = Command::new("rfkill").arg("unblock").arg("wifi").output();

    // Try nmcli first (available in NixOS live)
    let scan = Command::new("nmcli")
        .args([
            "-t",
            "-f",
            "SSID,SIGNAL,SECURITY",
            "device",
            "wifi",
            "list",
            "--rescan",
            "yes",
        ])
        .output();

    let networks: Vec<(String, String, String)> = match scan {
        Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|line| {
                let parts: Vec<&str> = line.splitn(3, ':').collect();
                if parts.len() >= 2 && !parts[0].is_empty() {
                    Some((
                        parts[0].to_string(),
                        parts.get(1).unwrap_or(&"").to_string(),
                        parts.get(2).unwrap_or(&"").to_string(),
                    ))
                } else {
                    None
                }
            })
            .collect(),
        _ => Vec::new(),
    };

    if networks.is_empty() {
        // Offer wpa_supplicant fallback
        let setup = dialoguer::Confirm::new()
            .with_prompt("  No WiFi networks found via nmcli. Enter WiFi credentials manually?")
            .default(true)
            .interact()?;

        if setup {
            let ssid: String = dialoguer::Input::new()
                .with_prompt("  SSID")
                .interact_text()?;
            let password: String = dialoguer::Password::new()
                .with_prompt("  Password")
                .interact()?;

            // Try nmcli first, fall back to wpa_supplicant
            let connected = Command::new("nmcli")
                .args(["device", "wifi", "connect", &ssid, "password", &password])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);

            if !connected {
                // wpa_supplicant fallback for environments without NetworkManager
                println!("  nmcli unavailable, trying wpa_supplicant...");
                // Escape quotes in SSID/PSK to prevent wpa_supplicant config injection
                let safe_ssid = ssid.replace('\\', "\\\\").replace('"', "\\\"");
                let safe_password = password.replace('\\', "\\\\").replace('"', "\\\"");
                let wpa_conf =
                    format!("network={{\n  ssid=\"{safe_ssid}\"\n  psk=\"{safe_password}\"\n}}\n");
                let wpa_path = std::path::Path::new("/tmp/wpa_supplicant.conf");
                // Write with restrictive permissions (0600) to protect credentials
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    let mut f = std::fs::OpenOptions::new()
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .mode(0o600)
                        .open(wpa_path)?;
                    std::io::Write::write_all(&mut f, wpa_conf.as_bytes())?;
                }
                // Ensure cleanup on all exit paths
                let _wpa_cleanup = scopeguard::WpaCleanup(wpa_path);
                // Find the wireless interface
                let iface = find_wifi_interface().unwrap_or_else(|| "wlan0".to_string());
                let wpa_status = Command::new("wpa_supplicant")
                    .args(["-B", "-i", &iface, "-c", &wpa_path.to_string_lossy()])
                    .status();
                if !wpa_status.map(|s| s.success()).unwrap_or(false) {
                    eprintln!("  warning: wpa_supplicant failed to start");
                }
                let dhcp_status = Command::new("dhcpcd").arg(&iface).status();
                if !dhcp_status.map(|s| s.success()).unwrap_or(false) {
                    eprintln!("  warning: dhcpcd failed — may not get an IP address");
                }
            }

            // Wait briefly for connection
            std::thread::sleep(std::time::Duration::from_secs(3));
        } else {
            println!(
                "  {} Continuing without network (offline install)",
                style("!").yellow()
            );
            return Ok(());
        }
    } else {
        // Show network picker
        let labels: Vec<String> = networks
            .iter()
            .map(|(ssid, signal, sec)| format!("{ssid}  (signal: {signal}%  {sec})"))
            .collect();

        let selection = dialoguer::Select::new()
            .with_prompt("  Select WiFi network")
            .items(&labels)
            .default(0)
            .interact()?;

        let ssid = &networks[selection].0;
        let sec = &networks[selection].2;

        if sec.is_empty() || sec.contains("--") {
            // Open network
            let nmcli_status = Command::new("nmcli")
                .args(["device", "wifi", "connect", ssid])
                .status();
            if !nmcli_status.map(|s| s.success()).unwrap_or(false) {
                eprintln!("  warning: nmcli failed to connect to {ssid}");
            }
        } else {
            let password: String = dialoguer::Password::new()
                .with_prompt(format!("  Password for {ssid}"))
                .interact()?;

            let nmcli_status = Command::new("nmcli")
                .args(["device", "wifi", "connect", ssid, "password", &password])
                .status();
            if !nmcli_status.map(|s| s.success()).unwrap_or(false) {
                eprintln!("  warning: nmcli failed to connect to {ssid}");
            }
        }

        // Wait for connection
        std::thread::sleep(std::time::Duration::from_secs(3));
    }

    // Verify
    let connected = Command::new("ping")
        .args(["-c1", "-W3", "1.1.1.1"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if connected {
        println!("  {} Network connected", style("✓").green().bold());
    } else {
        println!(
            "  {} Could not verify connectivity — continuing anyway",
            style("!").yellow()
        );
    }

    // WiFi credentials cleaned up by WpaCleanup drop guard if wpa_supplicant was used
    println!();

    Ok(())
}

fn step_hostname(defaults: &Defaults) -> Result<String> {
    println!("  {}", style("── Hostname ──").bold());

    loop {
        let mut input = dialoguer::Input::<String>::new().with_prompt("  Hostname");

        if let Some(ref h) = defaults.hostname {
            input = input.default(h.clone());
        } else {
            input = input.default("nixos".to_string());
        }

        let hostname = input.interact_text()?;

        if let Err(msg) = validate_hostname(&hostname) {
            println!("  {} {msg}", style("!").yellow());
            continue;
        }

        println!();
        return Ok(hostname);
    }
}

fn step_username(defaults: &Defaults) -> Result<String> {
    println!("  {}", style("── User account ──").bold());

    loop {
        let mut input = dialoguer::Input::<String>::new().with_prompt("  Username");

        if let Some(ref u) = defaults.username {
            input = input.default(u.clone());
        } else {
            input = input.default(std::env::var("USER").unwrap_or_else(|_| "user".to_string()));
        }

        let username = input.interact_text()?;

        if let Err(msg) = validate_username(&username) {
            println!("  {} {msg}", style("!").yellow());
            continue;
        }

        println!();
        return Ok(username);
    }
}

fn validate_hostname(h: &str) -> std::result::Result<(), &'static str> {
    if h.is_empty() {
        return Err("hostname cannot be empty");
    }
    if h.len() > 63 {
        return Err("hostname must be 63 characters or fewer");
    }
    if h.starts_with('-') || h.ends_with('-') {
        return Err("hostname cannot start or end with a hyphen");
    }
    if !h.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err("hostname must contain only letters, digits, and hyphens");
    }
    Ok(())
}

fn validate_username(u: &str) -> std::result::Result<(), &'static str> {
    if u.is_empty() {
        return Err("username cannot be empty");
    }
    if u == "root" {
        return Err("cannot use 'root' — it's reserved by the system");
    }
    if u.len() > 32 {
        return Err("username must be 32 characters or fewer");
    }
    if !u.starts_with(|c: char| c.is_ascii_lowercase() || c == '_') {
        return Err("username must start with a lowercase letter or underscore");
    }
    if !u
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return Err(
            "username must contain only lowercase letters, digits, hyphens, and underscores",
        );
    }
    Ok(())
}

fn step_timezone(defaults: &Defaults) -> Result<String> {
    println!("  {}", style("── Timezone ──").bold());

    // Try to detect from system
    let detected = std::fs::read_to_string("/etc/localtime")
        .ok()
        .and_then(|_| {
            // Read the symlink target
            std::fs::read_link("/etc/localtime").ok()
        })
        .and_then(|p| {
            let s = p.display().to_string();
            s.find("zoneinfo/").map(|i| s[i + 9..].to_string())
        });

    let default_tz = defaults
        .timezone
        .clone()
        .or(detected)
        .unwrap_or_else(|| "America/New_York".to_string());

    let timezone: String = dialoguer::Input::new()
        .with_prompt("  Timezone")
        .default(default_tz)
        .interact_text()?;

    println!();
    Ok(timezone)
}

fn step_disk() -> Result<String> {
    println!("  {}", style("── Target disk ──").bold());

    // Detect the boot device so we can warn/filter it
    let boot_dev = detect_boot_device();

    // List disks with lsblk JSON for structured parsing
    let output = Command::new("lsblk")
        .args(["-d", "-J", "-o", "NAME,SIZE,MODEL,TRAN,TYPE"])
        .output()
        .context("lsblk not found")?;

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_default();

    let disks: Vec<(String, String, bool)> = json
        .get("blockdevices")
        .and_then(|b| b.as_array())
        .unwrap_or(&Vec::new())
        .iter()
        .filter(|d| {
            let dtype = d.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let name = d.get("name").and_then(|v| v.as_str()).unwrap_or("");
            dtype == "disk"
                && !name.starts_with("loop")
                && !name.starts_with("sr")
                && !name.starts_with("ram")
        })
        .map(|d| {
            let name = d.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let size = d.get("size").and_then(|v| v.as_str()).unwrap_or("?");
            let model = d.get("model").and_then(|v| v.as_str()).unwrap_or("");
            let tran = d.get("tran").and_then(|v| v.as_str()).unwrap_or("");
            let is_boot = boot_dev.as_deref() == Some(name);
            let tag = if is_boot { " [BOOT USB - skip]" } else { "" };
            let label = format!("/dev/{name}  {size}  {model}  ({tran}){tag}");
            (format!("/dev/{name}"), label, is_boot)
        })
        .collect();

    // Filter out boot device from selectable options
    let selectable: Vec<&(String, String, bool)> =
        disks.iter().filter(|(_, _, is_boot)| !is_boot).collect();

    if selectable.is_empty() {
        bail!("No installable disks found. Is the target machine's storage visible?");
    }

    let labels: Vec<&str> = selectable.iter().map(|(_, l, _)| l.as_str()).collect();
    let selection = dialoguer::Select::new()
        .with_prompt("  Select target disk (WILL BE ERASED)")
        .items(&labels)
        .interact()?;

    let disk = selectable[selection].0.clone();
    println!();
    Ok(disk)
}

/// Strip partition suffix from a device name to get the base disk.
/// e.g., "sda1" → "sda", "nvme0n1p2" → "nvme0n1", "mmcblk0p1" → "mmcblk0"
fn strip_partition_suffix(dev: &str) -> Option<String> {
    // NVMe/eMMC: ends with pN where N is digits
    // e.g., nvme0n1p1 → strip "p1" → nvme0n1
    if dev.contains("nvme") || dev.contains("mmcblk") {
        // Find last 'p' followed by only digits
        if let Some(p_pos) = dev.rfind('p') {
            let after_p = &dev[p_pos + 1..];
            if !after_p.is_empty() && after_p.chars().all(|c| c.is_ascii_digit()) {
                let base = &dev[..p_pos];
                if !base.is_empty() {
                    return Some(base.to_string());
                }
            }
        }
        // No partition suffix — return as-is
        return Some(dev.to_string());
    }

    // SATA/SCSI: ends with digits (sda1 → sda, sdb2 → sdb)
    let base = dev.trim_end_matches(|c: char| c.is_ascii_digit());
    if !base.is_empty() {
        Some(base.to_string())
    } else {
        None
    }
}

/// Detect the device the live ISO was booted from, so we can filter it out.
fn detect_boot_device() -> Option<String> {
    // Read /proc/cmdline for root= parameter
    if let Ok(cmdline) = std::fs::read_to_string("/proc/cmdline") {
        for part in cmdline.split_whitespace() {
            if let Some(root) = part.strip_prefix("root=") {
                let dev = root.trim_start_matches("/dev/");
                if let Some(base) = strip_partition_suffix(dev) {
                    return Some(base);
                }
            }
        }
    }

    // Fallback: find what's mounted at /iso or /nix/.ro-store (NixOS live ISO mounts)
    if let Ok(mounts) = std::fs::read_to_string("/proc/mounts") {
        for line in mounts.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 && (parts[1] == "/iso" || parts[1].contains("ro-store")) {
                if let Some(base) = strip_partition_suffix(parts[0].trim_start_matches("/dev/")) {
                    return Some(base);
                }
            }
        }
    }

    None
}

fn step_profile(defaults: &Defaults) -> Result<(Option<String>, Option<String>)> {
    println!("  {}", style("── Nex profile ──").bold());

    if let Some(ref profile_ref) = defaults.profile_ref {
        println!("  Bundled profile: {}", style(profile_ref).cyan());
        let use_bundled = dialoguer::Confirm::new()
            .with_prompt("  Use this profile?")
            .default(true)
            .interact()?;

        if use_bundled {
            println!();
            return Ok((Some(profile_ref.clone()), defaults.profile_toml.clone()));
        }
    }

    let options = &[
        "Enter a nex profile (GitHub user/repo)",
        "Skip — install base NixOS only",
    ];

    let choice = dialoguer::Select::new()
        .with_prompt("  Profile")
        .items(options)
        .default(0)
        .interact()?;

    match choice {
        0 => {
            let profile_ref: String = dialoguer::Input::new()
                .with_prompt("  Profile (user/repo)")
                .interact_text()?;

            // Fetch and resolve extends chain
            println!("  Resolving profile chain...");
            let result = crate::ops::forge::resolve_profile_chain(&profile_ref);

            match result {
                Ok(resolved) => {
                    println!("  {} Resolved {}", style("✓").green(), &profile_ref);
                    for layer in &resolved.chain {
                        println!("    {} {}", style("↳").dim(), style(layer).dim());
                    }
                    println!();
                    Ok((Some(profile_ref), Some(resolved.merged)))
                }
                Err(e) => {
                    println!("  {} Could not fetch: {e}", style("!").yellow());
                    let cont = dialoguer::Confirm::new()
                        .with_prompt("  Continue without profile?")
                        .default(true)
                        .interact()?;
                    if !cont {
                        bail!("Aborted");
                    }
                    println!();
                    Ok((None, None))
                }
            }
        }
        _ => {
            println!();
            Ok((None, None))
        }
    }
}

// ── Execution steps ──────────────────────────────────────────────────────

fn exec_partition(disk: &str) -> Result<()> {
    println!("  {} Partitioning {}...", style(">>>").bold(), disk);

    let is_efi = Path::new("/sys/firmware/efi").exists();

    let part_prefix = if disk.contains("nvme") || disk.contains("mmcblk") {
        format!("{disk}p")
    } else {
        disk.to_string()
    };

    if is_efi {
        run_cmd(
            "parted",
            &[
                disk, "--script", "--", "mklabel", "gpt", "mkpart", "ESP", "fat32", "1MiB",
                "512MiB", "set", "1", "esp", "on", "mkpart", "root", "ext4", "512MiB", "100%",
            ],
        )?;

        std::thread::sleep(std::time::Duration::from_secs(1));

        run_cmd("mkfs.fat", &["-F32", &format!("{part_prefix}1")])?;
        run_cmd("mkfs.ext4", &["-F", &format!("{part_prefix}2")])?;
    } else {
        // BIOS: MBR with single ext4 partition + GRUB
        run_cmd(
            "parted",
            &[
                disk, "--script", "--", "mklabel", "msdos", "mkpart", "primary", "ext4", "1MiB",
                "100%", "set", "1", "boot", "on",
            ],
        )?;

        std::thread::sleep(std::time::Duration::from_secs(1));

        run_cmd("mkfs.ext4", &["-F", &format!("{part_prefix}1")])?;
    }

    println!(
        "  {} Partitioned ({})",
        style("✓").green().bold(),
        if is_efi { "EFI" } else { "BIOS" }
    );
    Ok(())
}

fn exec_mount(disk: &str) -> Result<()> {
    println!("  {} Mounting filesystems...", style(">>>").bold());

    // Clean up any pre-existing mounts from a previous failed attempt
    let _ = Command::new("umount").args(["-R", "/mnt"]).status();

    let is_efi = Path::new("/sys/firmware/efi").exists();

    let part_prefix = if disk.contains("nvme") || disk.contains("mmcblk") {
        format!("{disk}p")
    } else {
        disk.to_string()
    };

    if is_efi {
        run_cmd("mount", &[&format!("{part_prefix}2"), "/mnt"])?;
        std::fs::create_dir_all("/mnt/boot")?;
        run_cmd("mount", &[&format!("{part_prefix}1"), "/mnt/boot"])?;
    } else {
        run_cmd("mount", &[&format!("{part_prefix}1"), "/mnt"])?;
    }

    println!("  {} Mounted", style("✓").green().bold());
    Ok(())
}

/// Path on the live ISO that maps to the installed system's user-owned config
/// directory. Writing here means the config lives at ~/nix-config after reboot,
/// which avoids the /etc/nixos sudo trap.
fn config_dir_for(username: &str) -> String {
    format!("/mnt/home/{username}/nix-config")
}

fn exec_generate_hardware(username: &str) -> Result<()> {
    println!(
        "  {} Generating hardware-configuration.nix...",
        style(">>>").bold()
    );

    let output = Command::new("nixos-generate-config")
        .args(["--root", "/mnt", "--show-hardware-config"])
        .output()
        .context("nixos-generate-config not found")?;

    if !output.status.success() {
        bail!("nixos-generate-config failed");
    }

    let config_dir = config_dir_for(username);
    std::fs::create_dir_all(&config_dir)?;
    std::fs::write(
        format!("{config_dir}/hardware-configuration.nix"),
        &output.stdout,
    )?;

    println!("  {} Hardware detected", style("✓").green().bold());
    Ok(())
}

fn exec_write_config(
    hostname: &str,
    username: &str,
    timezone: &str,
    disk: &str,
    profile_toml: Option<&str>,
) -> Result<()> {
    println!("  {} Writing NixOS configuration...", style(">>>").bold());

    let config_dir_string = config_dir_for(username);
    let config_dir = Path::new(&config_dir_string);
    std::fs::create_dir_all(config_dir)?;

    // Parse profile for [linux] section if available
    let profile: Option<toml::Value> = profile_toml.and_then(|t| toml::from_str(t).ok());

    // ── flake.nix ────────────────────────────────────────────────────
    let system = crate::discover::detect_system();

    // COSMIC is in nixpkgs proper since 25.05 — no external flake needed.

    std::fs::write(
        config_dir.join("flake.nix"),
        format!(
            r#"{{
  description = "NixOS — installed by nex polymerize";

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
      specialArgs = {{ username = "{username}"; hostname = "{hostname}"; }};
      modules = [
        ./configuration.nix
        ./hardware-configuration.nix
        home-manager.nixosModules.home-manager
        {{
          home-manager = {{
            useGlobalPkgs = true;
            useUserPackages = true;
            backupFileExtension = "backup";
            extraSpecialArgs = {{ username = "{username}"; hostname = "{hostname}"; }};
            users."{username}" = import ./home.nix;
          }};
        }}
      ];
    }};
  }};
}}
"#
        ),
    )?;

    // ── configuration.nix ────────────────────────────────────────────
    let mut lines = Vec::new();
    lines.push("{ pkgs, lib, username, hostname, ... }:".to_string());
    lines.push(String::new());
    lines.push("{".to_string());
    lines.push(format!("  networking.hostName = \"{hostname}\";"));
    lines.push(String::new());
    lines
        .push("  nix.settings.experimental-features = [ \"nix-command\" \"flakes\" ];".to_string());
    lines.push("  nix.settings.download-buffer-size = 1073741824;".to_string());
    lines.push("  nixpkgs.config.allowUnfree = true;".to_string());
    lines.push(String::new());
    // Detect BIOS vs EFI
    if Path::new("/sys/firmware/efi").exists() {
        lines.push("  boot.loader.systemd-boot.enable = true;".to_string());
        lines.push("  boot.loader.efi.canTouchEfiVariables = true;".to_string());
    } else {
        lines.push("  boot.loader.grub.enable = true;".to_string());
        lines.push(format!("  boot.loader.grub.device = \"{disk}\";"));
    }
    lines.push(String::new());
    lines.push(format!("  users.users.\"{username}\" = {{"));
    lines.push("    isNormalUser = true;".to_string());
    lines.push(
        "    extraGroups = [ \"wheel\" \"networkmanager\" \"video\" \"audio\" ];".to_string(),
    );
    lines.push("    shell = pkgs.bash;".to_string());
    lines.push("  };".to_string());
    lines.push(String::new());
    lines.push("  networking.networkmanager.enable = true;".to_string());
    lines.push(String::new());
    lines.push(format!("  time.timeZone = \"{timezone}\";"));
    lines.push("  i18n.defaultLocale = \"en_US.UTF-8\";".to_string());
    lines.push(String::new());

    // Profile-driven config
    if let Some(ref profile) = profile {
        if let Some(linux) = profile.get("linux") {
            crate::ops::forge::generate_linux_config(&mut lines, linux);
        }
    }

    lines.push("  system.stateVersion = \"25.05\";".to_string());
    lines.push("}".to_string());
    lines.push(String::new());

    std::fs::write(config_dir.join("configuration.nix"), lines.join("\n"))?;

    // ── home.nix ─────────────────────────────────────────────────────
    let mut home = vec![
        "{ pkgs, username, ... }:".to_string(),
        String::new(),
        "{".to_string(),
        "  home = {".to_string(),
        "    username = username;".to_string(),
        "    homeDirectory = \"/home/${username}\";".to_string(),
        "    stateVersion = \"25.05\";".to_string(),
        "  };".to_string(),
    ];
    home.push(String::new());
    home.push("  home.sessionPath = [".to_string());
    home.push("    \"$HOME/.local/bin\"".to_string());
    home.push("    \"$HOME/.cargo/bin\"".to_string());
    home.push("    \"$HOME/.nix-profile/bin\"".to_string());
    home.push("  ];".to_string());
    home.push(String::new());
    home.push("  home.packages = with pkgs; [".to_string());

    // Packages from profile
    if let Some(ref profile) = profile {
        if let Some(pkgs) = profile
            .get("packages")
            .and_then(|p| p.get("nix"))
            .and_then(|n| n.as_array())
        {
            for pkg in pkgs {
                if let Some(name) = pkg.as_str() {
                    if is_valid_nix_pkg_name(name) {
                        home.push(format!("    {name}"));
                    }
                }
            }
        }
    }

    home.push("    git".to_string());
    home.push("    vim".to_string());
    home.push("  ];".to_string());
    home.push(String::new());

    // Shell aliases from profile
    if let Some(ref profile) = profile {
        if let Some(aliases) = profile
            .get("shell")
            .and_then(|s| s.get("aliases"))
            .and_then(|a| a.as_table())
        {
            home.push("  programs.bash.enable = true;".to_string());
            home.push("  programs.bash.shellAliases = {".to_string());
            for (name, cmd) in aliases {
                if let Some(cmd_str) = cmd.as_str() {
                    let escaped = cmd_str
                        .replace('\\', "\\\\")
                        .replace('"', "\\\"")
                        .replace("${", "\\${");
                    home.push(format!("    {name} = \"{escaped}\";"));
                }
            }
            home.push("  };".to_string());
            home.push(String::new());
        }

        // Environment variables
        if let Some(env) = profile
            .get("shell")
            .and_then(|s| s.get("env"))
            .and_then(|e| e.as_table())
        {
            home.push("  home.sessionVariables = {".to_string());
            for (key, val) in env {
                if let Some(val_str) = val.as_str() {
                    let escaped = val_str
                        .replace('\\', "\\\\")
                        .replace('"', "\\\"")
                        .replace("${", "\\${");
                    home.push(format!("    {key} = \"{escaped}\";"));
                }
            }
            home.push("  };".to_string());
            home.push(String::new());
        }

        // Git config (home-manager 25.05+ uses programs.git.settings.*)
        if let Some(git) = profile.get("git") {
            home.push("  programs.git = {".to_string());
            home.push("    enable = true;".to_string());
            home.push("    settings = {".to_string());
            if let Some(name) = git.get("name").and_then(|v| v.as_str()) {
                let escaped = name
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace("${", "\\${");
                home.push(format!("      user.name = \"{escaped}\";"));
            }
            if let Some(email) = git.get("email").and_then(|v| v.as_str()) {
                let escaped = email
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace("${", "\\${");
                home.push(format!("      user.email = \"{escaped}\";"));
            }
            if let Some(branch) = git.get("default_branch").and_then(|v| v.as_str()) {
                let escaped = branch
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace("${", "\\${");
                home.push(format!("      init.defaultBranch = \"{escaped}\";"));
            }
            if git.get("pull_rebase").and_then(|v| v.as_bool()) == Some(true) {
                home.push("      pull.rebase = true;".to_string());
            }
            if git.get("push_auto_setup_remote").and_then(|v| v.as_bool()) == Some(true) {
                home.push("      push.autoSetupRemote = true;".to_string());
            }
            home.push("    };".to_string());
            home.push("  };".to_string());
            home.push(String::new());
        }
    }

    home.push("  programs.home-manager.enable = true;".to_string());
    home.push("}".to_string());
    home.push(String::new());

    std::fs::write(config_dir.join("home.nix"), home.join("\n"))?;

    println!("  {} Configuration written", style("✓").green().bold());
    Ok(())
}

fn exec_install(hostname: &str, username: &str) -> Result<()> {
    // Set generous download buffer to avoid pressure warnings on large installs
    std::env::set_var("NIX_DOWNLOAD_BUFFER_SIZE", "1073741824");

    // Verify network — nixos-install needs to fetch flake inputs
    let has_net = Command::new("ping")
        .args(["-c1", "-W3", "1.1.1.1"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !has_net {
        println!(
            "  {} No network! nixos-install needs internet to fetch NixOS packages.",
            style("!").red().bold()
        );
        println!("    Connect to a network and retry, or Ctrl+C to abort.");

        let retry = dialoguer::Confirm::new()
            .with_prompt("  Retry?")
            .default(true)
            .interact()?;

        if !retry {
            bail!("nixos-install requires network to fetch flake inputs");
        }

        // Re-check
        let has_net = Command::new("ping")
            .args(["-c1", "-W3", "1.1.1.1"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if !has_net {
            bail!("Still no network. Cannot proceed with nixos-install.");
        }
    }

    println!(
        "  {} Running nixos-install (this takes a while)...",
        style(">>>").bold()
    );

    let config_dir = config_dir_for(username);
    let status = Command::new("nixos-install")
        .args([
            "--flake",
            &format!("{config_dir}#{hostname}"),
            "--no-root-passwd",
        ])
        .env("NIX_DOWNLOAD_BUFFER_SIZE", "1073741824")
        .env("NIX_CONFIG", "download-buffer-size = 1073741824")
        .status()
        .context("nixos-install not found")?;

    if !status.success() {
        bail!(
            "nixos-install failed. Check the configuration at {config_dir}/\n\
             You can fix issues and re-run: nixos-install --flake {config_dir}#{hostname}"
        );
    }

    println!("  {} NixOS installed", style("✓").green().bold());
    Ok(())
}

/// Hand the config directory to the target user. We wrote it as root from the
/// live ISO, so without this it would land at /home/<user>/nix-config owned by
/// root — defeating the whole point of moving off /etc/nixos.
fn exec_chown_config(username: &str) -> Result<()> {
    println!(
        "  {} Chowning /home/{username}/nix-config to {username}...",
        style(">>>").bold(),
    );

    let status = Command::new("nixos-enter")
        .args([
            "--root",
            "/mnt",
            "--",
            "chown",
            "-R",
            &format!("{username}:users"),
            &format!("/home/{username}/nix-config"),
        ])
        .status()
        .context("nixos-enter chown failed")?;

    if !status.success() {
        // Non-fatal: user can re-chown after first boot.
        println!(
            "  {} chown failed — run after reboot: \
             sudo chown -R {username}:users ~/nix-config",
            style("!").yellow()
        );
    } else {
        println!("  {} Config owned by {username}", style("✓").green().bold());
    }
    Ok(())
}

fn exec_set_passwords(username: &str) -> Result<()> {
    println!("  {}", style("── Set passwords ──").bold());

    println!("  Setting root password:");
    let root_ok = Command::new("nixos-enter")
        .args(["--root", "/mnt", "--", "passwd"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !root_ok {
        println!(
            "  {} Root password not set. Set it after reboot with: sudo passwd",
            style("!").yellow()
        );
    }

    println!("  Setting password for {username}:");
    let user_ok = Command::new("nixos-enter")
        .args(["--root", "/mnt", "--", "passwd", username])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !user_ok {
        println!(
            "  {} User password not set. Set it after reboot with: sudo passwd {username}",
            style("!").yellow()
        );
    }

    Ok(())
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn running_as_root() -> bool {
    std::env::var("EUID")
        .or_else(|_| std::env::var("UID"))
        .map(|v| v == "0")
        .unwrap_or_else(|_| {
            // Fallback: check if we can read /root
            Path::new("/root").read_dir().is_ok()
        })
}

/// Write ~/.config/nex/config.toml into the installed system so nex can find the repo.
fn exec_write_nex_config(hostname: &str, username: &str) -> Result<()> {
    let nex_config_dir = format!("/mnt/home/{username}/.config/nex");
    std::fs::create_dir_all(&nex_config_dir)?;

    let config_content =
        format!("repo_path = \"/home/{username}/nix-config\"\nhostname = \"{hostname}\"\n");
    std::fs::write(format!("{nex_config_dir}/config.toml"), &config_content)?;

    // Fix ownership — nixos-enter resolves the username inside the installed system
    // where the user account actually exists (created by nixos-install).
    let chown_status = Command::new("nixos-enter")
        .args([
            "--root",
            "/mnt",
            "--",
            "chown",
            "-R",
            &format!("{username}:users"),
            &format!("/home/{username}/.config"),
        ])
        .status();
    if !chown_status.map(|s| s.success()).unwrap_or(false) {
        eprintln!("  warning: failed to fix ownership of ~/.config/nex — user may need to run: sudo chown -R {username}:users ~/.config");
    }

    Ok(())
}

/// Find the first wireless network interface.
fn find_wifi_interface() -> Option<String> {
    // /sys/class/net/*/wireless exists for WiFi interfaces
    if let Ok(entries) = std::fs::read_dir("/sys/class/net") {
        for entry in entries.flatten() {
            let wireless = entry.path().join("wireless");
            if wireless.exists() {
                return entry.file_name().to_str().map(String::from);
            }
        }
    }
    None
}

/// Check if a string is a valid Nix package identifier (no injection risk).
fn is_valid_nix_pkg_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

fn run_cmd(program: &str, args: &[&str]) -> Result<()> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("{program} not found"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = if stderr.trim().is_empty() {
            String::new()
        } else {
            format!(": {}", stderr.trim().lines().last().unwrap_or(""))
        };
        bail!("{program} failed{detail}");
    }
    Ok(())
}

/// Check if a disk has LUKS or LVM signatures that would need special handling.
fn check_disk_for_special_layouts(disk: &str) {
    // Check for LUKS
    let has_luks = Command::new("blkid")
        .args(["-o", "value", "-s", "TYPE", disk])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("crypto_LUKS"))
        .unwrap_or(false);

    if has_luks {
        println!(
            "  {} Disk has LUKS encryption. Polymerize will overwrite it with plain ext4.",
            style("!").yellow()
        );
        println!(
            "    {}",
            style("For encrypted installs, partition manually before running polymerize.").dim()
        );
    }

    // Check for LVM
    let has_lvm = Command::new("pvs")
        .args(["--noheadings", "-o", "pv_name"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(disk))
        .unwrap_or(false);

    if has_lvm {
        println!(
            "  {} Disk has LVM volumes. Polymerize will overwrite them with plain partitions.",
            style("!").yellow()
        );
        println!(
            "    {}",
            style("For LVM installs, partition manually before running polymerize.").dim()
        );
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_hostname_valid() {
        assert!(validate_hostname("gamingpc").is_ok());
        assert!(validate_hostname("nixos").is_ok());
        assert!(validate_hostname("my-host").is_ok());
        assert!(validate_hostname("a").is_ok());
        assert!(validate_hostname("host-123").is_ok());
    }

    #[test]
    fn test_validate_hostname_invalid() {
        assert!(validate_hostname("").is_err());
        assert!(validate_hostname("-starts-bad").is_err());
        assert!(validate_hostname("ends-bad-").is_err());
        assert!(validate_hostname("has spaces").is_err());
        assert!(validate_hostname("has.dots").is_err());
        assert!(validate_hostname("has_under").is_err());
        assert!(validate_hostname(&"a".repeat(64)).is_err());
    }

    #[test]
    fn test_validate_username_valid() {
        assert!(validate_username("wilson").is_ok());
        assert!(validate_username("chris").is_ok());
        assert!(validate_username("user-name").is_ok());
        assert!(validate_username("user_name").is_ok());
        assert!(validate_username("_private").is_ok());
        assert!(validate_username("a1b2").is_ok());
    }

    #[test]
    fn test_validate_username_invalid() {
        assert!(validate_username("").is_err());
        assert!(validate_username("root").is_err());
        assert!(validate_username("Root").is_err());
        assert!(validate_username("1user").is_err());
        assert!(validate_username("user name").is_err());
        assert!(validate_username("user.name").is_err());
        assert!(validate_username(&"a".repeat(33)).is_err());
    }

    #[test]
    fn test_is_valid_nix_pkg_name() {
        assert!(is_valid_nix_pkg_name("git"));
        assert!(is_valid_nix_pkg_name("proton-ge-bin"));
        assert!(is_valid_nix_pkg_name("python3.11"));
        assert!(is_valid_nix_pkg_name("obs-studio"));
        assert!(!is_valid_nix_pkg_name(""));
        assert!(!is_valid_nix_pkg_name("git; rm -rf /"));
        assert!(!is_valid_nix_pkg_name("pkg with spaces"));
        assert!(!is_valid_nix_pkg_name("pkg\"injection"));
    }

    #[test]
    fn test_strip_partition_suffix_sata() {
        assert_eq!(strip_partition_suffix("sda1"), Some("sda".to_string()));
        assert_eq!(strip_partition_suffix("sdb2"), Some("sdb".to_string()));
        assert_eq!(strip_partition_suffix("sda"), Some("sda".to_string()));
    }

    #[test]
    fn test_strip_partition_suffix_nvme() {
        assert_eq!(
            strip_partition_suffix("nvme0n1p1"),
            Some("nvme0n1".to_string())
        );
        assert_eq!(
            strip_partition_suffix("nvme0n1p2"),
            Some("nvme0n1".to_string())
        );
        assert_eq!(
            strip_partition_suffix("nvme0n1"),
            Some("nvme0n1".to_string())
        );
    }

    #[test]
    fn test_strip_partition_suffix_emmc() {
        assert_eq!(
            strip_partition_suffix("mmcblk0p1"),
            Some("mmcblk0".to_string())
        );
        assert_eq!(
            strip_partition_suffix("mmcblk0"),
            Some("mmcblk0".to_string())
        );
    }

    #[test]
    fn test_strip_partition_suffix_no_aggressive_strip() {
        // Should NOT strip trailing letters from non-NVMe/eMMC devices
        assert_eq!(strip_partition_suffix("sdp1"), Some("sdp".to_string()));
    }

    #[test]
    fn test_validate_hostname_max_length() {
        // Exactly 63 chars should be ok
        assert!(validate_hostname(&"a".repeat(63)).is_ok());
        assert!(validate_hostname(&"a".repeat(64)).is_err());
    }

    #[test]
    fn test_validate_username_hyphen_middle() {
        // Hyphens allowed in middle, not start
        assert!(validate_username("my-user").is_ok());
        assert!(validate_username("-user").is_err());
    }

    #[test]
    fn test_nix_pkg_name_with_nix_interpolation() {
        // ${} would be Nix string interpolation — must not be allowed
        assert!(!is_valid_nix_pkg_name("pkg${evil}"));
        assert!(!is_valid_nix_pkg_name("$(cmd)"));
    }

    #[test]
    fn test_strip_partition_suffix_nvme_multiple_partitions() {
        assert_eq!(
            strip_partition_suffix("nvme0n1p10"),
            Some("nvme0n1".to_string())
        );
        assert_eq!(
            strip_partition_suffix("nvme1n1p3"),
            Some("nvme1n1".to_string())
        );
    }

    #[test]
    fn test_strip_partition_suffix_bare_device() {
        // No partition suffix at all
        assert_eq!(strip_partition_suffix("sda"), Some("sda".to_string()));
        assert_eq!(
            strip_partition_suffix("nvme0n1"),
            Some("nvme0n1".to_string())
        );
    }

    #[test]
    fn test_validate_hostname_unicode() {
        assert!(validate_hostname("héllo").is_err());
        assert!(validate_hostname("host🔥").is_err());
    }

    #[test]
    fn test_validate_username_numeric_only() {
        // Can't start with digit
        assert!(validate_username("123").is_err());
        // But digits after first char are fine
        assert!(validate_username("a123").is_ok());
    }
}
