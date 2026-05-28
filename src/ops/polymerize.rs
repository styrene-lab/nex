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
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use console::style;
use zeroize::Zeroizing;

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
    arch: Option<String>,
    install_mode: Option<String>,
    network_require_wired: Option<bool>,
    network_wifi_allowed: Option<bool>,
    wifi_ssid: Option<String>,
    wifi_psk: Option<String>,
    ssh_authorized_keys: Option<Vec<String>>,
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

    let read_bool = |name: &str| {
        read(name).and_then(|value| match value.as_str() {
            "true" | "1" | "yes" => Some(true),
            "false" | "0" | "no" => Some(false),
            _ => None,
        })
    };

    let read_lines = |name: &str| {
        std::fs::read_to_string(dir.join(name))
            .ok()
            .map(|content| {
                content
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .filter(|lines| !lines.is_empty())
    };

    Defaults {
        hostname: read("defaults/hostname"),
        username: read("defaults/username"),
        timezone: read("defaults/timezone"),
        arch: read("defaults/arch"),
        install_mode: read("defaults/install_mode"),
        network_require_wired: read_bool("defaults/network_require_wired"),
        network_wifi_allowed: read_bool("defaults/network_wifi_allowed"),
        wifi_ssid: read("defaults/wifi_ssid"),
        wifi_psk: read("defaults/wifi_psk"),
        ssh_authorized_keys: read_lines("defaults/ssh_authorized_keys"),
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
    if let Some(ref mode) = defaults.install_mode {
        println!(
            "  {} Install mode: {}",
            style("i").cyan(),
            style(mode).bold()
        );
    }
    if let Some(ref arch) = defaults.arch {
        let current_arch = std::env::consts::ARCH;
        if arch != current_arch {
            println!(
                "  {} Bundle arch is {}, live system arch is {}",
                style("!").yellow(),
                style(arch).bold(),
                style(current_arch).bold()
            );
        }
    }
    println!();

    // ── 1. Network ───────────────────────────────────────────────────
    step_network(&defaults)?;

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
    if let Some(ref mode) = defaults.install_mode {
        println!("  Mode:      {}", style(mode).cyan());
    }
    if let Some(keys) = &defaults.ssh_authorized_keys {
        println!("  SSH keys:  {}", style(keys.len()).cyan());
    }
    // Warn about special disk layouts
    check_disk_for_special_layouts(&disk);

    println!();

    let confirm = crate::input::input().confirm(
        "  Proceed with installation? (THIS WILL ERASE THE DISK)",
        false,
    )?;

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
        defaults.ssh_authorized_keys.as_deref(),
    )?;
    exec_install(&hostname, &username)?;
    exec_persist_network(&defaults)?;
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
        style("║  Reboot:   sudo umount -R /mnt && sudo reboot         ║").green()
    );
    println!(
        "  {}",
        style("╚══════════════════════════════════════════════════════╝").green()
    );
    println!();

    Ok(())
}

// ── Interactive steps ────────────────────────────────────────────────────

fn step_network(defaults: &Defaults) -> Result<()> {
    println!("  {}", style("── Network ──").bold());

    let mut status = detect_network_status(2);
    if accept_existing_network(&status, defaults) {
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

    // Re-check after NM start (ethernet may have auto-connected).
    status = detect_network_status(2);
    if accept_existing_network(&status, defaults) {
        println!();
        return Ok(());
    }

    if defaults.network_wifi_allowed == Some(false) {
        println!(
            "  {} No active network detected. WiFi prompting is disabled by the forged bundle.",
            style("!").yellow()
        );
        println!("    Continuing; the install step will re-check package fetch connectivity.");
        println!();
        return Ok(());
    }

    if let (Some(ssid), Some(psk)) = (&defaults.wifi_ssid, &defaults.wifi_psk) {
        println!("  No wired connection detected. Trying bundled WiFi: {ssid}");
        let connected = connect_wifi(ssid, Some(psk));
        std::thread::sleep(std::time::Duration::from_secs(3));
        if connected && has_network_connectivity(3) {
            println!(
                "  {} Network connected (bundled WiFi)",
                style("✓").green().bold()
            );
            println!();
            return Ok(());
        }
        eprintln!("  warning: bundled WiFi did not establish connectivity; falling back to interactive setup");
        println!();
    }

    println!("  No wired connection detected. Scanning for WiFi...");
    println!();

    // Bring up wlan interface
    let _ = Command::new("rfkill").arg("unblock").arg("wifi").output();

    let scan = scan_wifi_networks();
    let networks = scan.networks;

    if networks.is_empty() {
        match scan.source {
            WifiScanSource::NmcliUnavailable => {
                println!(
                    "  {} WiFi scan tool nmcli is unavailable on this installer.",
                    style("!").yellow()
                );
            }
            WifiScanSource::NmcliFailed => {
                println!("  {} WiFi scan via nmcli failed.", style("!").yellow());
            }
            WifiScanSource::Nmcli => {
                println!(
                    "  {} No WiFi networks were discovered.",
                    style("!").yellow()
                );
            }
        }

        let setup = crate::input::input().confirm("  Enter WiFi credentials manually?", true)?;

        if setup {
            let ssid: String = crate::input::input().input_text("  SSID", None)?;
            let password: String = crate::input::input().password("  Password")?;

            if !connect_wifi(&ssid, Some(&password)) {
                eprintln!("  warning: WiFi connection attempt did not complete");
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

        let selection = crate::input::input().select("  Select WiFi network", &labels, 0)?;

        let ssid = &networks[selection].0;
        let sec = &networks[selection].2;

        if sec.is_empty() || sec.contains("--") {
            // Open network
            if !connect_wifi(ssid, None) {
                eprintln!("  warning: failed to connect to {ssid}");
            }
        } else {
            let password: String =
                crate::input::input().password(&format!("  Password for {ssid}"))?;

            if !connect_wifi(ssid, Some(&password)) {
                eprintln!("  warning: failed to connect to {ssid}");
            }
        }

        // Wait for connection
        std::thread::sleep(std::time::Duration::from_secs(3));
    }

    // Verify
    let connected = has_network_connectivity(3);

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

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct NetworkStatus {
    verified_connectivity: bool,
    active_wired: bool,
    active_wifi: bool,
}

fn accept_existing_network(status: &NetworkStatus, defaults: &Defaults) -> bool {
    if status.verified_connectivity {
        let transport = if status.active_wired {
            "wired"
        } else if status.active_wifi {
            "WiFi"
        } else {
            "pre-configured"
        };
        if defaults.network_require_wired == Some(true)
            && status.active_wifi
            && !status.active_wired
        {
            println!(
                "  {} Wired network was requested, but active WiFi already has connectivity.",
                style("!").yellow()
            );
        }
        println!(
            "  {} Network connected ({transport})",
            style("✓").green().bold()
        );
        return true;
    }

    if status.active_wired {
        println!(
            "  {} Wired network is active; external connectivity verification failed.",
            style("!").yellow()
        );
        println!("    Continuing; the install step will re-check package fetch connectivity.");
        return true;
    }

    if status.active_wifi {
        if defaults.network_require_wired == Some(true) {
            println!(
                "  {} Wired network was requested, but active WiFi is already configured.",
                style("!").yellow()
            );
        }
        if defaults.network_wifi_allowed == Some(false) {
            println!(
                "  {} WiFi prompting is disabled, but an operator-configured WiFi connection is active.",
                style("!").yellow()
            );
        }
        println!("    Continuing; the install step will re-check package fetch connectivity.");
        return true;
    }

    false
}

fn detect_network_status(timeout_seconds: u64) -> NetworkStatus {
    let mut status = NetworkStatus {
        verified_connectivity: has_network_connectivity(timeout_seconds),
        ..NetworkStatus::default()
    };

    if let Ok(output) = Command::new("nmcli")
        .args(["-t", "-f", "TYPE,STATE", "device", "status"])
        .output()
    {
        if output.status.success() {
            for line in crate::exec::captured_text(&output.stdout).lines() {
                let mut parts = line.splitn(2, ':');
                let dtype = parts.next().unwrap_or("");
                let state = parts.next().unwrap_or("");
                if !state.starts_with("connected") {
                    continue;
                }
                match dtype {
                    "ethernet" => status.active_wired = true,
                    "wifi" | "wireless" => status.active_wifi = true,
                    _ => {}
                }
            }
        }
    }

    status
}

fn has_network_connectivity(timeout_seconds: u64) -> bool {
    let timeout = timeout_seconds.to_string();
    let https_targets = [
        "https://cache.nixos.org/nix-cache-info",
        "https://github.com/",
    ];
    for target in https_targets {
        let status = Command::new("curl")
            .args([
                "-fsSL",
                "--connect-timeout",
                &timeout,
                "--max-time",
                &timeout,
                "-o",
                "/dev/null",
                target,
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if status {
            return true;
        }
    }

    Command::new("ping")
        .args(["-c1", &format!("-W{timeout_seconds}"), "1.1.1.1"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum WifiScanSource {
    Nmcli,
    #[default]
    NmcliUnavailable,
    NmcliFailed,
}

#[derive(Debug, Default)]
struct WifiScan {
    source: WifiScanSource,
    networks: Vec<(String, String, String)>,
}

fn scan_wifi_networks() -> WifiScan {
    if !command_available("nmcli") {
        return WifiScan {
            source: WifiScanSource::NmcliUnavailable,
            networks: Vec::new(),
        };
    }

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

    match scan {
        Ok(output) if output.status.success() => WifiScan {
            source: WifiScanSource::Nmcli,
            networks: parse_nmcli_wifi_list(&crate::exec::captured_text(&output.stdout)),
        },
        _ => WifiScan {
            source: WifiScanSource::NmcliFailed,
            networks: Vec::new(),
        },
    }
}

fn parse_nmcli_wifi_list(output: &str) -> Vec<(String, String, String)> {
    output
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
        .collect()
}

fn connect_wifi(ssid: &str, password: Option<&str>) -> bool {
    if connect_wifi_with_nmcli(ssid, password) {
        return true;
    }

    match connect_wifi_with_wpa_supplicant(ssid, password) {
        Ok(connected) => connected,
        Err(error) => {
            eprintln!("  warning: wpa_supplicant WiFi path failed: {error}");
            false
        }
    }
}

fn connect_wifi_with_nmcli(ssid: &str, password: Option<&str>) -> bool {
    if !command_available("nmcli") {
        return false;
    }
    let mut command = Command::new("nmcli");
    command.args(["device", "wifi", "connect", ssid]);
    if let Some(password) = password {
        command.args(["password", password]);
    }
    command.status().map(|s| s.success()).unwrap_or(false)
}

fn connect_wifi_with_wpa_supplicant(ssid: &str, password: Option<&str>) -> Result<bool> {
    if !command_available("wpa_supplicant") {
        bail!("wpa_supplicant is unavailable");
    }

    let iface = find_wifi_interface().unwrap_or_else(|| "wlan0".to_string());
    let wpa_path = std::path::Path::new("/tmp/nex-wpa_supplicant.conf");
    write_wpa_supplicant_config(wpa_path, ssid, password)?;
    let _wpa_cleanup = scopeguard::WpaCleanup(wpa_path);

    let wpa_status = Command::new("wpa_supplicant")
        .args(["-B", "-i", &iface, "-c", &wpa_path.to_string_lossy()])
        .status()
        .context("failed to start wpa_supplicant")?;
    if !wpa_status.success() {
        bail!("wpa_supplicant exited unsuccessfully");
    }

    std::thread::sleep(std::time::Duration::from_secs(2));
    request_dhcp_lease(&iface)
}

fn write_wpa_supplicant_config(path: &Path, ssid: &str, password: Option<&str>) -> Result<()> {
    let safe_ssid = escape_wpa_value(ssid)?;
    let network = if let Some(password) = password {
        let safe_password = escape_wpa_value(password)?;
        format!("network={{\n  ssid=\"{safe_ssid}\"\n  psk=\"{safe_password}\"\n}}\n")
    } else {
        format!("network={{\n  ssid=\"{safe_ssid}\"\n  key_mgmt=NONE\n}}\n")
    };

    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        std::io::Write::write_all(&mut f, network.as_bytes())?;
    }
    Ok(())
}

fn escape_wpa_value(value: &str) -> Result<String> {
    if value.chars().any(char::is_control) {
        bail!("WiFi values cannot contain control characters");
    }
    Ok(value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn request_dhcp_lease(iface: &str) -> Result<bool> {
    if command_available("dhcpcd") {
        let config = std::path::Path::new("/tmp/nex-dhcpcd.conf");
        std::fs::write(config, "hostname\nclientid\npersistent\n")?;
        let status = Command::new("dhcpcd")
            .args(["-f", &config.to_string_lossy(), iface])
            .status()
            .context("failed to run dhcpcd")?;
        if status.success() {
            return Ok(true);
        }
    }

    if command_available("dhclient") {
        let status = Command::new("dhclient")
            .arg(iface)
            .status()
            .context("failed to run dhclient")?;
        if status.success() {
            return Ok(true);
        }
    }

    if command_available("udhcpc") {
        let status = Command::new("udhcpc")
            .args(["-i", iface, "-q"])
            .status()
            .context("failed to run udhcpc")?;
        if status.success() {
            return Ok(true);
        }
    }

    Ok(has_network_connectivity(3))
}

fn command_available(command: &str) -> bool {
    Command::new(command)
        .arg("--help")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

fn step_hostname(defaults: &Defaults) -> Result<String> {
    println!("  {}", style("── Hostname ──").bold());

    loop {
        let default_hostname = defaults
            .hostname
            .clone()
            .unwrap_or_else(|| "nixos".to_string());

        let hostname = crate::input::input().input_text("  Hostname", Some(&default_hostname))?;

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
        let default_username = defaults
            .username
            .clone()
            .unwrap_or_else(|| std::env::var("USER").unwrap_or_else(|_| "user".to_string()));

        let username = crate::input::input().input_text("  Username", Some(&default_username))?;

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

    let timezone: String = crate::input::input().input_text("  Timezone", Some(&default_tz))?;

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

    let labels: Vec<String> = selectable.iter().map(|(_, l, _)| l.clone()).collect();
    let selection =
        crate::input::input().select("  Select target disk (WILL BE ERASED)", &labels, 0)?;

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
        let use_bundled = crate::input::input().confirm("  Use this profile?", true)?;

        if use_bundled {
            println!();
            return Ok((Some(profile_ref.clone()), defaults.profile_toml.clone()));
        }
    }

    let options: Vec<String> = vec![
        "Enter a nex profile (GitHub user/repo)".to_string(),
        "Skip — install base NixOS only".to_string(),
    ];

    let choice = crate::input::input().select("  Profile", &options, 0)?;

    match choice {
        0 => {
            let profile_ref: String =
                crate::input::input().input_text("  Profile (user/repo)", None)?;

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
                    let cont =
                        crate::input::input().confirm("  Continue without profile?", true)?;
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
    prepare_target_disk_for_partitioning(disk)?;

    let part_prefix = if disk.contains("nvme") || disk.contains("mmcblk") {
        format!("{disk}p")
    } else {
        disk.to_string()
    };

    if is_efi {
        run_parted(
            disk,
            &[
                "mklabel", "gpt", "mkpart", "ESP", "fat32", "1MiB", "512MiB", "set", "1", "esp",
                "on", "mkpart", "root", "ext4", "512MiB", "100%",
            ],
        )?;
        refresh_partition_table(disk);

        // Wait for partition device nodes to appear (kernel may take a moment)
        let part2 = format!("{part_prefix}2");
        let mut waited = 0;
        while !std::path::Path::new(&part2).exists() && waited < 10 {
            std::thread::sleep(std::time::Duration::from_secs(1));
            waited += 1;
        }
        if !std::path::Path::new(&part2).exists() {
            bail!("partition device {} did not appear after 10 seconds", part2);
        }

        run_cmd("mkfs.fat", &["-F32", &format!("{part_prefix}1")])?;
        run_cmd("mkfs.ext4", &["-F", &format!("{part_prefix}2")])?;
    } else {
        // BIOS: MBR with single ext4 partition + GRUB
        run_parted(
            disk,
            &[
                "mklabel", "msdos", "mkpart", "primary", "ext4", "1MiB", "100%", "set", "1",
                "boot", "on",
            ],
        )?;
        refresh_partition_table(disk);

        // Wait for partition device nodes to appear (kernel may take a moment)
        let part1 = format!("{part_prefix}1");
        let mut waited = 0;
        while !std::path::Path::new(&part1).exists() && waited < 10 {
            std::thread::sleep(std::time::Duration::from_secs(1));
            waited += 1;
        }
        if !std::path::Path::new(&part1).exists() {
            bail!("partition device {} did not appear after 10 seconds", part1);
        }

        run_cmd("mkfs.ext4", &["-F", &format!("{part_prefix}1")])?;
    }

    println!(
        "  {} Partitioned ({})",
        style("✓").green().bold(),
        if is_efi { "EFI" } else { "BIOS" }
    );
    Ok(())
}

fn prepare_target_disk_for_partitioning(disk: &str) -> Result<()> {
    println!("  {} Preparing target disk...", style(">>>").bold());

    let _ = Command::new("umount").args(["-R", "/mnt"]).status();
    let _ = Command::new("swapoff").arg("-a").status();

    let mut partitions = partition_paths(disk);
    partitions.reverse();
    for partition in &partitions {
        let _ = Command::new("swapoff").arg(partition).status();
        let _ = Command::new("umount").arg(partition).status();
        if command_available("wipefs") {
            let _ = Command::new("wipefs").args(["-a", partition]).status();
        }
    }

    if command_available("partx") {
        let _ = Command::new("partx").args(["-d", disk]).status();
    }

    if command_available("blockdev") {
        let _ = Command::new("blockdev")
            .args(["--flushbufs", disk])
            .status();
    }

    if command_available("wipefs") {
        let status = Command::new("wipefs")
            .args(["-a", disk])
            .status()
            .context("failed to run wipefs")?;
        if !status.success() {
            bail!(
                "target disk {disk} still appears to be in use; unmount/swapoff stale partitions or reboot the installer before retrying"
            );
        }
    }

    refresh_partition_table(disk);
    Ok(())
}

fn partition_paths(disk: &str) -> Vec<String> {
    let Ok(output) = Command::new("lsblk")
        .args(["-ln", "-o", "PATH", disk])
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    parse_lsblk_paths(&crate::exec::captured_text(&output.stdout), disk)
}

fn parse_lsblk_paths(output: &str, disk: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|path| !path.is_empty() && *path != disk)
        .map(str::to_string)
        .collect()
}

fn run_parted(disk: &str, commands: &[&str]) -> Result<()> {
    let mut args = vec![disk, "--script", "--"];
    args.extend_from_slice(commands);
    let output = Command::new("parted")
        .args(&args)
        .output()
        .context("parted not found")?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = crate::exec::captured_text(&output.stderr);
    refresh_partition_table(disk);
    if stderr.contains("unable to inform the kernel") {
        bail!(
            "parted wrote a partition table but the kernel refused to reload it because {disk} is still in use. Reboot the installer, then rerun polymerize."
        );
    }

    let detail = stderr
        .trim()
        .lines()
        .last()
        .map(|line| format!(": {line}"))
        .unwrap_or_default();
    bail!("parted failed{detail}");
}

fn refresh_partition_table(disk: &str) {
    if command_available("partprobe") {
        let _ = Command::new("partprobe").arg(disk).status();
    }
    if command_available("partx") {
        let _ = Command::new("partx").args(["-u", disk]).status();
    }
    if command_available("blockdev") {
        let _ = Command::new("blockdev").args(["--rereadpt", disk]).status();
    }
    if command_available("udevadm") {
        let _ = Command::new("udevadm").args(["settle"]).status();
    }
}

fn exec_mount(disk: &str) -> Result<()> {
    println!("  {} Mounting filesystems...", style(">>>").bold());

    // Clean up any pre-existing mounts from a previous failed attempt
    let umount = Command::new("umount").args(["-R", "/mnt"]).status();
    if !umount.map(|s| s.success()).unwrap_or(false) {
        tracing::warn!("umount -R /mnt failed — stale mounts may interfere");
    }

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
    ssh_authorized_keys: Option<&[String]>,
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
    if let Some(keys) = ssh_authorized_keys {
        lines.push("    openssh.authorizedKeys.keys = [".to_string());
        for key in keys {
            lines.push(format!("      {}", nix_string(key)));
        }
        lines.push("    ];".to_string());
    }
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
    if let Some(ref profile) = profile {
        if let Some(paths) = profile
            .get("shell")
            .and_then(|s| s.get("paths"))
            .and_then(|p| p.as_array())
        {
            for path in paths {
                if let Some(path_str) = path.as_str() {
                    if path_str != "$HOME/.local/bin" && path_str != "~/.local/bin" {
                        home.push(format!("    \"{path_str}\""));
                    }
                }
            }
        }
    }
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

fn nix_string(value: &str) -> String {
    format!("{value:?}")
}

fn exec_persist_network(defaults: &Defaults) -> Result<()> {
    let target_dir = Path::new("/mnt/etc/NetworkManager/system-connections");
    std::fs::create_dir_all(target_dir)?;

    let mut persisted = 0usize;
    if let (Some(ssid), Some(psk)) = (&defaults.wifi_ssid, &defaults.wifi_psk) {
        let path = target_dir.join("nex-forge-wifi.nmconnection");
        std::fs::write(&path, network_manager_wifi_connection(ssid, psk)?)?;
        set_secret_permissions(&path)?;
        persisted += 1;
    }

    let live_dir = Path::new("/etc/NetworkManager/system-connections");
    if let Ok(entries) = std::fs::read_dir(live_dir) {
        for entry in entries.flatten() {
            let source = entry.path();
            if !source.is_file() {
                continue;
            }
            let Some(name) = source.file_name() else {
                continue;
            };
            let dest = target_dir.join(name);
            std::fs::copy(&source, &dest)
                .with_context(|| format!("failed to persist {}", source.display()))?;
            set_secret_permissions(&dest)?;
            persisted += 1;
        }
    }

    if persisted > 0 {
        println!(
            "  {} Persisted {persisted} NetworkManager connection(s)",
            style("✓").green().bold()
        );
    }

    Ok(())
}

fn network_manager_wifi_connection(ssid: &str, psk: &str) -> Result<String> {
    if ssid.chars().any(char::is_control) || psk.chars().any(char::is_control) {
        bail!("WiFi SSID/PSK cannot contain control characters");
    }
    let uuid = std::fs::read_to_string("/proc/sys/kernel/random/uuid")
        .unwrap_or_else(|_| "00000000-0000-4000-8000-000000000000".to_string());
    Ok(format!(
        "[connection]\n\
         id=nex-forge-wifi\n\
         uuid={uuid}\n\
         type=wifi\n\
         autoconnect=true\n\
         \n\
         [wifi]\n\
         mode=infrastructure\n\
         ssid={ssid}\n\
         \n\
         [wifi-security]\n\
         key-mgmt=wpa-psk\n\
         psk={psk}\n\
         \n\
         [ipv4]\n\
         method=auto\n\
         \n\
         [ipv6]\n\
         method=auto\n",
        uuid = uuid.trim(),
    ))
}

#[cfg(unix)]
fn set_secret_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_secret_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

fn exec_install(hostname: &str, username: &str) -> Result<()> {
    // Set generous download buffer to avoid pressure warnings on large installs
    std::env::set_var("NIX_DOWNLOAD_BUFFER_SIZE", "1073741824");

    // Verify network — nixos-install needs to fetch flake inputs
    let has_net = has_network_connectivity(3);

    if !has_net {
        println!(
            "  {} No network! nixos-install needs internet to fetch NixOS packages.",
            style("!").red().bold()
        );
        println!("    Connect to a network and retry, or Ctrl+C to abort.");

        let retry = crate::input::input().confirm("  Retry?", true)?;

        if !retry {
            bail!("nixos-install requires network to fetch flake inputs");
        }

        // Re-check
        let has_net = has_network_connectivity(3);

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

    let root_password = prompt_install_password("  Root password")?;
    set_installed_password("root", &root_password)?;
    verify_installed_password_set("root")?;
    println!("  {} Root password set", style("✓").green().bold());

    let user_password = prompt_install_password(&format!("  Password for {username}"))?;
    set_installed_password(username, &user_password)?;
    verify_installed_password_set(username)?;
    println!("  {} User password set", style("✓").green().bold());

    Ok(())
}

fn prompt_install_password(prompt: &str) -> Result<Zeroizing<String>> {
    let password = Zeroizing::new(crate::input::input().password_with_confirm(prompt)?);
    validate_install_password(&password)?;
    Ok(password)
}

fn validate_install_password(password: &str) -> Result<()> {
    if password.is_empty() {
        bail!("password cannot be empty");
    }
    if password.contains('\n') || password.contains('\r') {
        bail!("password cannot contain newlines");
    }
    Ok(())
}

fn set_installed_password(user: &str, password: &str) -> Result<()> {
    let mut child = Command::new("nixos-enter")
        .args(["--root", "/mnt", "--", "chpasswd"])
        .stdin(Stdio::piped())
        .spawn()
        .context("failed to run nixos-enter chpasswd")?;

    {
        use std::io::Write;

        let stdin = child
            .stdin
            .as_mut()
            .context("failed to open chpasswd stdin")?;
        stdin
            .write_all(format!("{user}:{password}\n").as_bytes())
            .context("failed to send password to chpasswd")?;
    }

    let status = child.wait().context("failed waiting for chpasswd")?;
    if !status.success() {
        bail!("failed to set password for {user} in installed system");
    }

    Ok(())
}

fn verify_installed_password_set(user: &str) -> Result<()> {
    let output = Command::new("nixos-enter")
        .args(["--root", "/mnt", "--", "passwd", "--status", user])
        .output()
        .context("failed to verify installed password status")?;

    if !output.status.success() {
        bail!("failed to verify password status for {user}");
    }

    let stdout = crate::exec::captured_text(&output.stdout);
    let fields: Vec<&str> = stdout.split_whitespace().collect();
    if fields.get(1) != Some(&"P") {
        let state = fields.get(1).copied().unwrap_or("unknown");
        bail!("password for {user} is not set in installed system (state: {state})");
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

/// Write ~/.config/nex/config.pkl into the installed system so nex can find the repo.
fn exec_write_nex_config(hostname: &str, username: &str) -> Result<()> {
    let nex_config_dir = format!("/mnt/home/{username}/.config/nex");
    std::fs::create_dir_all(&nex_config_dir)?;

    let config_content = format!(
        "// Generated by nex.\nrepo_path = {:?}\nhostname = {:?}\n",
        format!("/home/{username}/nix-config"),
        hostname
    );
    std::fs::write(format!("{nex_config_dir}/config.pkl"), &config_content)?;

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
        let stderr = crate::exec::captured_text(&output.stderr);
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
        .map(|o| crate::exec::captured_text(&o.stdout).contains("crypto_LUKS"))
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
        .map(|o| crate::exec::captured_text(&o.stdout).contains(disk))
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
    fn test_load_defaults_reads_bundled_wifi_credentials() {
        let dir = tempfile::tempdir().unwrap();
        let defaults_dir = dir.path().join("defaults");
        std::fs::create_dir_all(&defaults_dir).unwrap();
        std::fs::write(defaults_dir.join("arch"), "x86_64\n").unwrap();
        std::fs::write(defaults_dir.join("install_mode"), "server\n").unwrap();
        std::fs::write(defaults_dir.join("network_require_wired"), "true\n").unwrap();
        std::fs::write(defaults_dir.join("network_wifi_allowed"), "false\n").unwrap();
        std::fs::write(defaults_dir.join("wifi_ssid"), "seed-net\n").unwrap();
        std::fs::write(defaults_dir.join("wifi_psk"), "secret-pass\n").unwrap();
        std::fs::write(
            defaults_dir.join("ssh_authorized_keys"),
            "ssh-ed25519 AAAAC3Nza seed\n\n",
        )
        .unwrap();

        let defaults = load_defaults(Some(dir.path()));

        assert_eq!(defaults.arch.as_deref(), Some("x86_64"));
        assert_eq!(defaults.install_mode.as_deref(), Some("server"));
        assert_eq!(defaults.network_require_wired, Some(true));
        assert_eq!(defaults.network_wifi_allowed, Some(false));
        assert_eq!(defaults.wifi_ssid.as_deref(), Some("seed-net"));
        assert_eq!(defaults.wifi_psk.as_deref(), Some("secret-pass"));
        assert_eq!(
            defaults.ssh_authorized_keys.as_deref(),
            Some(["ssh-ed25519 AAAAC3Nza seed".to_string()].as_slice())
        );
    }

    #[test]
    fn test_nix_string_escapes_authorized_keys() {
        assert_eq!(
            nix_string("ssh-ed25519 AAAA\"quoted"),
            "\"ssh-ed25519 AAAA\\\"quoted\""
        );
    }

    #[test]
    fn test_network_manager_wifi_connection_contains_autoconnect_profile() {
        let profile = network_manager_wifi_connection("seed-net", "secret-pass").unwrap();

        assert!(profile.contains("type=wifi"));
        assert!(profile.contains("\ntype=wifi\n"));
        assert!(profile.contains("autoconnect=true"));
        assert!(profile.contains("ssid=seed-net"));
        assert!(profile.contains("key-mgmt=wpa-psk"));
        assert!(profile.contains("psk=secret-pass"));
        assert!(profile.contains("method=auto"));
    }

    #[test]
    fn test_network_manager_wifi_connection_rejects_control_characters() {
        assert!(network_manager_wifi_connection("bad\nssid", "secret").is_err());
        assert!(network_manager_wifi_connection("seed-net", "bad\nsecret").is_err());
    }

    #[test]
    fn test_parse_nmcli_wifi_list() {
        let networks = parse_nmcli_wifi_list("SeedNet:83:WPA2\nOpenNet:42:\n:hidden\n");

        assert_eq!(
            networks,
            vec![
                ("SeedNet".to_string(), "83".to_string(), "WPA2".to_string()),
                ("OpenNet".to_string(), "42".to_string(), "".to_string()),
            ]
        );
    }

    #[test]
    fn test_parse_lsblk_paths_filters_target_disk() {
        let paths = parse_lsblk_paths(
            "/dev/nvme0n1\n/dev/nvme0n1p1\n/dev/nvme0n1p2\n/dev/nvme0n1p3\n",
            "/dev/nvme0n1",
        );

        assert_eq!(
            paths,
            vec![
                "/dev/nvme0n1p1".to_string(),
                "/dev/nvme0n1p2".to_string(),
                "/dev/nvme0n1p3".to_string(),
            ]
        );
    }

    #[test]
    fn test_escape_wpa_value_rejects_control_characters() {
        assert_eq!(escape_wpa_value("a\"b\\c").unwrap(), "a\\\"b\\\\c");
        assert!(escape_wpa_value("bad\nvalue").is_err());
    }

    #[test]
    fn test_validate_install_password_rejects_empty() {
        assert!(validate_install_password("").is_err());
    }

    #[test]
    fn test_validate_install_password_rejects_newlines() {
        assert!(validate_install_password("bad\npassword").is_err());
        assert!(validate_install_password("bad\rpassword").is_err());
    }

    #[test]
    fn test_validate_install_password_accepts_symbols() {
        assert!(validate_install_password("p:a$s w0rd!").is_ok());
    }

    #[test]
    fn test_accept_existing_network_allows_operator_configured_wifi() {
        let defaults = Defaults {
            network_require_wired: Some(true),
            network_wifi_allowed: Some(false),
            ..Defaults::default()
        };
        let status = NetworkStatus {
            verified_connectivity: true,
            active_wired: false,
            active_wifi: true,
        };

        assert!(accept_existing_network(&status, &defaults));
    }

    #[test]
    fn test_accept_existing_network_does_not_accept_absent_network() {
        let defaults = Defaults {
            network_require_wired: Some(true),
            network_wifi_allowed: Some(false),
            ..Defaults::default()
        };

        assert!(!accept_existing_network(
            &NetworkStatus::default(),
            &defaults
        ));
    }

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
