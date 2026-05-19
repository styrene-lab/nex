use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use console::style;

use crate::discover;
use crate::forge::{
    ForgeArch, ForgeDiagnostic, ForgeEvent, ForgeOperation, ForgePlan, ForgePreflightReport,
    ForgeRequest, PolymerizeDefaults,
};
use crate::input::input;
use crate::output;

const NIXOS_ISO_URL_X86: &str =
    "https://channels.nixos.org/nixos-24.11/latest-nixos-minimal-x86_64-linux.iso";
const NIXOS_ISO_URL_ARM: &str =
    "https://channels.nixos.org/nixos-24.11/latest-nixos-minimal-aarch64-linux.iso";
/// ISO filename includes arch to prevent cross-arch cache collisions.
fn iso_filename(arch: Arch) -> String {
    format!("nixos-minimal-{}.iso", arch.label())
}

// ── Interactive prompt helpers ───────────────────────────────────────────────

struct DiskInfo {
    device: String,
    label: String,
}

/// List removable/external disks suitable for flashing.
fn list_removable_disks() -> Vec<DiskInfo> {
    if crate::discover::detect_platform() == crate::discover::Platform::Darwin {
        list_removable_disks_macos()
    } else {
        list_removable_disks_linux()
    }
}

fn list_removable_disks_macos() -> Vec<DiskInfo> {
    let output = Command::new("diskutil")
        .args(["list", "-plist", "external", "physical"])
        .output();

    // Fallback: parse diskutil list text output
    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => {
            return list_removable_disks_macos_text();
        }
    };

    // The plist contains AllDisksAndPartitions with DeviceIdentifier entries
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut disks = Vec::new();

    // Simple plist parsing — look for whole-disk identifiers (disk4, not disk4s1)
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("<string>disk") {
            let dev = trimmed
                .trim_start_matches("<string>")
                .trim_end_matches("</string>");
            // Skip partition identifiers like "disk4s1" — we only want "disk4"
            if dev.contains('s') {
                continue;
            }
            if let Some(info) = get_disk_info_macos(dev) {
                disks.push(info);
            }
        }
    }

    disks
}

fn list_removable_disks_macos_text() -> Vec<DiskInfo> {
    let output = Command::new("diskutil").arg("list").output();
    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut disks = Vec::new();

    for line in stdout.lines() {
        // Lines like "/dev/disk4 (external, physical):"
        if line.starts_with("/dev/disk") && line.contains("external") {
            let dev = line.split_whitespace().next().unwrap_or("");
            if !dev.is_empty() {
                if let Some(info) = get_disk_info_macos(&dev.replace("/dev/", "")) {
                    disks.push(info);
                }
            }
        }
    }

    disks
}

fn get_disk_info_macos(dev_id: &str) -> Option<DiskInfo> {
    let output = Command::new("diskutil")
        .args(["info", &format!("/dev/{dev_id}")])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let info = String::from_utf8_lossy(&output.stdout);
    let mut size = String::new();
    let mut name = String::new();

    for line in info.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Disk Size:") {
            // "Disk Size:                 31.5 GB (31457280000 Bytes)..."
            size = trimmed
                .trim_start_matches("Disk Size:")
                .trim()
                .split('(')
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
        }
        if trimmed.starts_with("Media Name:") || trimmed.starts_with("Device / Media Name:") {
            name = trimmed.split(':').nth(1).unwrap_or("").trim().to_string();
        }
    }

    if name.is_empty() {
        name = "Unknown".to_string();
    }

    Some(DiskInfo {
        device: format!("/dev/{dev_id}"),
        label: format!("/dev/{dev_id}  {name}  {size}"),
    })
}

fn list_removable_disks_linux() -> Vec<DiskInfo> {
    let output = Command::new("lsblk")
        .args(["-d", "-J", "-o", "NAME,SIZE,MODEL,TRAN,TYPE,RM"])
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };

    let json: serde_json::Value = match serde_json::from_slice(&output.stdout) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let devices = json["blockdevices"].as_array();
    let devices = match devices {
        Some(d) => d,
        None => return Vec::new(),
    };

    let mut disks = Vec::new();
    for dev in devices {
        let dtype = dev["type"].as_str().unwrap_or("");
        // rm can be bool, string "1", or integer 1 depending on lsblk version
        let rm = dev["rm"].as_bool().unwrap_or_else(|| {
            dev["rm"].as_str() == Some("1")
                || dev["rm"].as_u64() == Some(1)
                || dev["rm"].as_i64() == Some(1)
        });
        let tran = dev["tran"].as_str().unwrap_or("");
        let name = dev["name"].as_str().unwrap_or("");

        // Only physical disks that are removable or USB
        if dtype != "disk" {
            continue;
        }
        if !rm && tran != "usb" {
            continue;
        }
        // Skip loop, sr, ram
        if name.starts_with("loop") || name.starts_with("sr") || name.starts_with("ram") {
            continue;
        }

        let size = dev["size"].as_str().unwrap_or("?");
        let model = dev["model"].as_str().unwrap_or("Unknown").trim();

        disks.push(DiskInfo {
            device: format!("/dev/{name}"),
            label: format!("/dev/{name}  {model}  {size}  {tran}"),
        });
    }

    disks
}

fn prompt_profile() -> Result<Option<String>> {
    let input: String = input()
        .input_text(
            "  Profile (GitHub user/repo, local path, or blank for generic)",
            Some(""),
        )
        .context("failed to read profile")?;

    // Strip quotes and whitespace from input
    let input = input
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_string();
    if input.is_empty() {
        return Ok(None);
    }

    // Local path?
    let path = std::path::Path::new(&input);
    if path.exists() && (path.join("profile.toml").exists() || input.ends_with(".toml")) {
        return Ok(Some(input));
    }

    // Treat as GitHub ref
    Ok(Some(input))
}

fn prompt_hostname() -> Result<String> {
    let hostname: String = input()
        .input_text("  Hostname for target", Some("nixos"))
        .context("failed to read hostname")?;

    let h = hostname.trim();
    if h.is_empty() {
        bail!("hostname cannot be empty");
    }
    if h.starts_with('-') || h.ends_with('-') {
        bail!("hostname cannot start or end with a hyphen");
    }
    if !h.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        bail!("hostname must be alphanumeric with hyphens only");
    }

    Ok(h.to_string())
}

#[derive(Clone, Copy, PartialEq)]
enum Arch {
    X86_64,
    Aarch64,
}

impl From<ForgeArch> for Arch {
    fn from(arch: ForgeArch) -> Self {
        match arch {
            ForgeArch::X86_64 => Arch::X86_64,
            ForgeArch::Aarch64 => Arch::Aarch64,
        }
    }
}

impl Arch {
    fn iso_url(&self) -> &'static str {
        match self {
            Arch::X86_64 => NIXOS_ISO_URL_X86,
            Arch::Aarch64 => NIXOS_ISO_URL_ARM,
        }
    }

    fn target_triple(&self) -> &'static str {
        match self {
            Arch::X86_64 => "x86_64-unknown-linux-gnu",
            Arch::Aarch64 => "aarch64-unknown-linux-gnu",
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Arch::X86_64 => "x86_64",
            Arch::Aarch64 => "aarch64",
        }
    }
}

fn prompt_arch() -> Result<Arch> {
    let items: Vec<String> = vec![
        "x86_64  (Intel/AMD)".to_string(),
        "aarch64 (Raspberry Pi, ARM)".to_string(),
    ];
    let selection = input()
        .select("  Target architecture", &items, 0)
        .context("failed to select architecture")?;

    Ok(if selection == 0 {
        Arch::X86_64
    } else {
        Arch::Aarch64
    })
}

fn prompt_disk() -> Result<Option<String>> {
    let mut disks = list_removable_disks();

    if disks.is_empty() {
        println!(
            "  {} no removable disks detected — building bundle only",
            style("i").cyan()
        );
        return Ok(None);
    }

    let mut labels: Vec<String> = disks.iter().map(|d| d.label.clone()).collect();
    labels.push("Skip (build bundle only)".to_string());

    let selection = input()
        .select("  Flash to USB", &labels, labels.len() - 1)
        .context("failed to select disk")?;

    if selection == disks.len() {
        Ok(None)
    } else {
        Ok(Some(disks.remove(selection).device))
    }
}

fn prompt_wifi() -> Result<Option<(String, String)>> {
    let configure = input()
        .confirm("  Pre-configure WiFi for first boot?", false)
        .context("failed to read wifi preference")?;

    if !configure {
        return Ok(None);
    }

    let ssid: String = input()
        .input_text("  WiFi SSID", None)
        .context("failed to read SSID")?;

    if ssid.trim().is_empty() {
        bail!("SSID cannot be empty");
    }

    let psk = input()
        .password("  WiFi password (blank for open network)")
        .context("failed to read WiFi password")?;

    if psk.is_empty() {
        println!("  {} open network (no password)", style("i").cyan());
    }

    Ok(Some((ssid, psk)))
}

fn prompt_ssh_key() -> Result<Option<String>> {
    // Check for existing SSH pubkey
    let home = dirs::home_dir().unwrap_or_default();
    let candidates = [
        home.join(".ssh/id_ed25519.pub"),
        home.join(".ssh/id_rsa.pub"),
    ];

    let pubkey_path = candidates.iter().find(|p| p.exists());

    if pubkey_path.is_none() {
        // Check if we can derive from StyreneIdentity
        let identity_path = styrene_identity::file_signer::FileSigner::default_path();
        if identity_path.exists() {
            println!(
                "  {} no SSH pubkey found at ~/.ssh/ — derive from identity with `nex identity ssh --add`",
                style("i").cyan()
            );
        }
        return Ok(None);
    }

    // Safety: we checked pubkey_path.is_none() above and returned
    let path = match pubkey_path {
        Some(p) => p,
        None => return Ok(None),
    };
    let bake = input()
        .confirm(
            &format!("  Bake SSH key for target access? ({})", path.display()),
            true,
        )
        .context("failed to read SSH key preference")?;

    if !bake {
        return Ok(None);
    }

    let key =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(Some(key.trim().to_string()))
}

// ── Main entry point ────────────────────────────────────────────────────────

/// Run `nex forge` — build a bootable NixOS installer USB.
/// If profile is None, builds a generic styx installer.
/// When run interactively with missing flags, prompts for each value.
pub fn run(
    profile_ref: Option<&str>,
    hostname: Option<&str>,
    disk: Option<&str>,
    output_dir: Option<&Path>,
    arch_flag: Option<&str>,
    dry_run: bool,
) -> Result<()> {
    run_with_options(
        profile_ref,
        hostname,
        disk,
        output_dir,
        arch_flag,
        dry_run,
        ForgeRunOptions::default(),
    )
}

struct ForgeRunOptions {
    prompt_optional_inputs: bool,
    allow_placeholder_nex: bool,
    polymerize_defaults: Option<PolymerizeDefaults>,
}

impl Default for ForgeRunOptions {
    fn default() -> Self {
        Self {
            prompt_optional_inputs: true,
            allow_placeholder_nex: true,
            polymerize_defaults: None,
        }
    }
}

fn run_with_options(
    profile_ref: Option<&str>,
    hostname: Option<&str>,
    disk: Option<&str>,
    output_dir: Option<&Path>,
    arch_flag: Option<&str>,
    dry_run: bool,
    options: ForgeRunOptions,
) -> Result<()> {
    let is_interactive = std::io::IsTerminal::is_terminal(&std::io::stdin());

    println!();
    println!("  {} — build NixOS installer", style("nex forge").bold());
    println!();

    // ── Resolve inputs: flags win, then prompt, then defaults ──

    let profile_ref_owned: Option<String> = match profile_ref {
        Some(p) => Some(p.to_string()),
        None if is_interactive => prompt_profile()?,
        None => None,
    };
    let profile_ref = profile_ref_owned.as_deref();
    let is_styx = profile_ref.is_none();

    let hostname_owned: String = match hostname {
        Some(h) => h.to_string(),
        None if is_interactive => prompt_hostname()?,
        None => "nixos".to_string(),
    };
    let hostname = hostname_owned.as_str();

    let arch = match arch_flag {
        Some(a) if matches!(a.to_lowercase().as_str(), "aarch64" | "arm64" | "arm") => {
            Arch::Aarch64
        }
        Some(a) if matches!(a.to_lowercase().as_str(), "x86_64" | "x86" | "amd64") => Arch::X86_64,
        Some(other) => bail!("unknown architecture: {other} (use x86_64 or aarch64)"),
        None if is_interactive => prompt_arch()?,
        None => Arch::X86_64,
    };

    let disk_owned: Option<String> = match disk {
        Some(d) => Some(d.to_string()),
        None if is_interactive => prompt_disk()?,
        None => None,
    };
    let disk = disk_owned.as_deref();

    let wifi = if options.prompt_optional_inputs && is_interactive {
        prompt_wifi()?
    } else {
        None
    };

    let ssh_key = if options.prompt_optional_inputs && is_interactive {
        prompt_ssh_key()?
    } else {
        None
    };

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
    let iso_path = bundle_dir.join(iso_filename(arch));
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
        download_file(arch.iso_url(), &iso_path)?;
        let size_mb = std::fs::metadata(&iso_path)?.len() / (1024 * 1024);
        println!("  {} NixOS ISO ({} MB)", style("✓").green().bold(), size_mb);
    }

    // ── 4. Write defaults for polymerize ─────────────────────────────
    let defaults_dir = styrene_dir.join("defaults");
    std::fs::create_dir_all(&defaults_dir)?;
    std::fs::write(defaults_dir.join("hostname"), hostname)?;

    let user = options
        .polymerize_defaults
        .as_ref()
        .map(|defaults| defaults.username.clone())
        .filter(|username| !username.is_empty())
        .unwrap_or_else(|| std::env::var("USER").unwrap_or_else(|_| "user".to_string()));
    let timezone = options
        .polymerize_defaults
        .as_ref()
        .map(|defaults| defaults.timezone.clone())
        .filter(|timezone| !timezone.is_empty())
        .unwrap_or_else(|| "America/New_York".to_string());
    std::fs::write(defaults_dir.join("username"), &user)?;
    std::fs::write(defaults_dir.join("timezone"), timezone)?;
    std::fs::write(defaults_dir.join("arch"), arch.label())?;

    // Write WiFi credentials if provided
    if let Some((ref ssid, ref psk)) = wifi {
        std::fs::write(defaults_dir.join("wifi_ssid"), ssid)?;
        // Write PSK with restricted permissions
        #[cfg(unix)]
        {
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(defaults_dir.join("wifi_psk"))?;
            f.write_all(psk.as_bytes())?;
        }
        #[cfg(not(unix))]
        std::fs::write(defaults_dir.join("wifi_psk"), psk)?;
        println!("  {} WiFi: {ssid}", style("✓").green().bold());
    }

    // Write SSH authorized key if provided
    if let Some(ref key) = ssh_key {
        std::fs::write(defaults_dir.join("ssh_authorized_keys"), key)?;
        println!(
            "  {} SSH key baked for target access",
            style("✓").green().bold()
        );
    }

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
    output::status(&format!(
        "bundling nex binary for {}-linux...",
        arch.label()
    ));
    let nex_bin_path = styrene_dir.join("nex");
    match fetch_nex_binary(&nex_bin_path, arch) {
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

                if !options.allow_placeholder_nex {
                    bail!("Cannot bundle placeholder nex binary for declarative forge request.");
                }
                let cont = input().confirm("  Continue without working nex binary?", false)?;
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
            if !options.allow_placeholder_nex {
                bail!("Could not fetch nex binary for declarative forge request: {e}");
            }
            println!("  {} Could not fetch nex binary: {e}", style("!").yellow());
            println!("    Build manually and copy to: {}", nex_bin_path.display());
        }
    }

    // ── 7. Write bundle manifest ─────────────────────────────────────
    let manifest = format!(
        "version: 2\n\
         hostname: {hostname}\n\
         profile: {profile}\n\
         arch: {arch}\n\
         styx: {is_styx}\n\
         created: {created}\n",
        profile = profile_ref.unwrap_or("none"),
        arch = arch.label(),
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
        let flash_iso_path = prepare_flash_iso_with_bundle(&bundle_dir, &iso_path)?;
        flash_to_usb(&flash_iso_path, device)?;
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

pub fn run_plan(request_path: &Path) -> Result<()> {
    let request = crate::forge::load_request(request_path)?;
    let plan = crate::forge::plan_request(&request)?;
    println!("{}", serde_json::to_string_pretty(&plan)?);
    Ok(())
}

pub fn run_request(request_path: &Path, events: &str, dry_run: bool) -> Result<()> {
    let events = EventMode::parse(events)?;
    emit_event(
        events,
        ForgeEvent::PhaseStarted {
            schema_version: crate::forge::FORGE_SCHEMA_VERSION,
            phase: "load-request".to_string(),
        },
    )?;
    let request = crate::forge::load_request(request_path)?;
    emit_event(
        events,
        ForgeEvent::PhaseCompleted {
            schema_version: crate::forge::FORGE_SCHEMA_VERSION,
            phase: "load-request".to_string(),
        },
    )?;

    emit_event(
        events,
        ForgeEvent::PhaseStarted {
            schema_version: crate::forge::FORGE_SCHEMA_VERSION,
            phase: "plan".to_string(),
        },
    )?;
    let plan = crate::forge::plan_request(&request)?;
    for warning in &plan.warnings {
        emit_event(
            events,
            ForgeEvent::Warning {
                schema_version: crate::forge::FORGE_SCHEMA_VERSION,
                code: warning.code.clone(),
                message: warning.message.clone(),
            },
        )?;
    }
    if plan.is_blocked() {
        for blocker in &plan.blockers {
            emit_event(
                events,
                ForgeEvent::Blocker {
                    schema_version: crate::forge::FORGE_SCHEMA_VERSION,
                    code: blocker.code.clone(),
                    message: blocker.message.clone(),
                },
            )?;
        }
        emit_event(
            events,
            ForgeEvent::RunFailed {
                schema_version: crate::forge::FORGE_SCHEMA_VERSION,
                message: "forge request has blockers".to_string(),
            },
        )?;
        bail_with_plan_blockers(&plan);
    }
    emit_event(
        events,
        ForgeEvent::PhaseCompleted {
            schema_version: crate::forge::FORGE_SCHEMA_VERSION,
            phase: "plan".to_string(),
        },
    )?;

    let preflight = preflight_request(&request, &plan);
    for warning in &preflight.warnings {
        emit_event(
            events,
            ForgeEvent::Warning {
                schema_version: crate::forge::FORGE_SCHEMA_VERSION,
                code: warning.code.clone(),
                message: warning.message.clone(),
            },
        )?;
    }
    if !preflight.valid {
        for error in &preflight.errors {
            emit_event(
                events,
                ForgeEvent::Blocker {
                    schema_version: crate::forge::FORGE_SCHEMA_VERSION,
                    code: error.code.clone(),
                    message: error.message.clone(),
                },
            )?;
        }
        emit_event(
            events,
            ForgeEvent::RunFailed {
                schema_version: crate::forge::FORGE_SCHEMA_VERSION,
                message: "forge host preflight failed".to_string(),
            },
        )?;
        bail_with_preflight_errors(&preflight);
    }

    if dry_run {
        let plan_json = serde_json::to_string_pretty(&plan)?;
        if events == EventMode::Jsonl {
            eprintln!("{plan_json}");
        } else {
            println!("{plan_json}");
        }
        emit_event(
            events,
            ForgeEvent::RunCompleted {
                schema_version: crate::forge::FORGE_SCHEMA_VERSION,
            },
        )?;
        return Ok(());
    }

    confirm_destructive_actions(&request, &plan)?;
    execute_request(&request, events)?;
    emit_event(
        events,
        ForgeEvent::ArtifactCreated {
            schema_version: crate::forge::FORGE_SCHEMA_VERSION,
            path: plan.output_dir,
        },
    )?;
    emit_event(
        events,
        ForgeEvent::RunCompleted {
            schema_version: crate::forge::FORGE_SCHEMA_VERSION,
        },
    )?;
    Ok(())
}

pub fn run_preflight(request_path: &Path, json: bool) -> Result<()> {
    let request = crate::forge::load_request(request_path)?;
    let plan = crate::forge::plan_request(&request)?;
    let mut preflight = preflight_request(&request, &plan);
    preflight.errors.extend(plan.blockers);
    preflight.warnings.extend(plan.warnings);
    preflight.valid = preflight.errors.is_empty();

    if json {
        println!("{}", serde_json::to_string_pretty(&preflight)?);
    } else if preflight.valid {
        println!("forge preflight passed");
        for warning in &preflight.warnings {
            println!("  warning {}: {}", warning.code, warning.message);
        }
    } else {
        println!("forge preflight failed");
        for error in &preflight.errors {
            println!("  {}: {}", error.code, error.message);
        }
    }

    if preflight.valid {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EventMode {
    Human,
    Jsonl,
}

impl EventMode {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "human" => Ok(Self::Human),
            "jsonl" => Ok(Self::Jsonl),
            other => bail!("unsupported forge event format {other}; use human or jsonl"),
        }
    }
}

fn emit_event(mode: EventMode, event: ForgeEvent) -> Result<()> {
    if mode == EventMode::Jsonl {
        println!("{}", serde_json::to_string(&event)?);
    }
    Ok(())
}

fn bail_with_plan_blockers(plan: &ForgePlan) -> ! {
    for blocker in &plan.blockers {
        eprintln!(
            "{} {}: {}",
            style("!").red().bold(),
            blocker.code,
            blocker.message
        );
    }
    std::process::exit(1);
}

fn bail_with_preflight_errors(report: &ForgePreflightReport) -> ! {
    for error in &report.errors {
        eprintln!(
            "{} {}: {}",
            style("!").red().bold(),
            error.code,
            error.message
        );
    }
    std::process::exit(1);
}

fn confirm_destructive_actions(request: &ForgeRequest, plan: &ForgePlan) -> Result<()> {
    if plan.destructive_actions.is_empty() {
        return Ok(());
    }
    if !request.safety.allow_destructive_flash {
        bail!("destructive flash is not allowed by request safety policy");
    }
    if !request.safety.require_operator_confirmation {
        return Ok(());
    }

    println!();
    println!("  {}", style("Destructive forge actions").red().bold());
    for action in &plan.destructive_actions {
        println!("  {} {}", style("!").red().bold(), action.message);
    }
    println!();
    let confirmed = input()
        .confirm("  Proceed with destructive forge action?", false)
        .context("failed to read forge confirmation")?;
    if !confirmed {
        bail!("operator declined destructive forge action");
    }
    Ok(())
}

fn preflight_request(request: &ForgeRequest, plan: &ForgePlan) -> ForgePreflightReport {
    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    match request.operation {
        ForgeOperation::Bundle | ForgeOperation::UsbInstall => {
            require_command("curl", &mut errors);
            warn_command(
                "nix",
                &mut warnings,
                "Nix is recommended so forge can build/cache a Linux Nex entrypoint.",
            );
        }
        ForgeOperation::Image | ForgeOperation::Netboot | ForgeOperation::RemotePolymerize => {
            errors.push(ForgeDiagnostic::new(
                "FORGE_RUN_OPERATION_UNSUPPORTED",
                "forge run currently supports bundle and usb-install operations only.",
            ));
        }
    }

    if let Some(profile) = request.profile.as_deref() {
        if looks_like_local_profile(profile) {
            let profile_path = Path::new(profile);
            let profile_file = if profile_path.is_dir() {
                profile_path.join("profile.toml")
            } else {
                profile_path.to_path_buf()
            };
            if !profile_file.exists() {
                errors.push(ForgeDiagnostic::new(
                    "PROFILE_NOT_FOUND",
                    format!("Local profile does not exist: {}", profile_file.display()),
                ));
            }
        } else {
            warn_command(
                "gh",
                &mut warnings,
                "GitHub profile resolution will fall back to curl if gh is unavailable.",
            );
        }
    }

    if let Some(iso) = plan.iso.as_ref() {
        if let Some(parent) = iso.cache_path.parent() {
            if let Err(error) = std::fs::create_dir_all(parent) {
                errors.push(ForgeDiagnostic::new(
                    "OUTPUT_DIR_NOT_WRITABLE",
                    format!(
                        "Cannot create output directory {}: {error}",
                        parent.display()
                    ),
                ));
            } else {
                let probe = parent.join(".nex-forge-preflight");
                match std::fs::write(&probe, b"") {
                    Ok(()) => {
                        let _ = std::fs::remove_file(&probe);
                    }
                    Err(error) => errors.push(ForgeDiagnostic::new(
                        "OUTPUT_DIR_NOT_WRITABLE",
                        format!(
                            "Cannot write to output directory {}: {error}",
                            parent.display()
                        ),
                    )),
                }
            }
        }
    }

    if request.operation == ForgeOperation::UsbInstall {
        require_command("sudo", &mut errors);
        require_command("dd", &mut errors);
        require_command("sync", &mut errors);
        if !command_exists("xorriso") {
            require_command("nix", &mut errors);
            warnings.push(ForgeDiagnostic::new(
                "XORRISO_USING_NIX_FALLBACK",
                "xorriso is not on PATH; forge will run it through `nix run nixpkgs#xorriso -- ...`.",
            ));
        }

        if crate::discover::detect_platform() == crate::discover::Platform::Darwin {
            require_command("diskutil", &mut errors);
        } else {
            warn_command(
                "partprobe",
                &mut warnings,
                "partprobe is used to refresh the partition table on Linux hosts.",
            );
        }

        if let Some(device) = request.target.disk.as_deref() {
            preflight_usb_device(device, &mut warnings, &mut errors);
        }
    }

    ForgePreflightReport::from_diagnostics(warnings, errors)
}

fn looks_like_local_profile(profile: &str) -> bool {
    profile.starts_with('/')
        || profile.starts_with("./")
        || profile.starts_with("../")
        || profile.ends_with(".toml")
}

fn command_exists(command: &str) -> bool {
    Command::new("which")
        .arg(command)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn require_command(command: &str, errors: &mut Vec<ForgeDiagnostic>) {
    if !command_exists(command) {
        errors.push(ForgeDiagnostic::new(
            "MISSING_HOST_TOOL",
            format!("Required host command not found on PATH: {command}"),
        ));
    }
}

fn warn_command(command: &str, warnings: &mut Vec<ForgeDiagnostic>, message: &str) {
    if !command_exists(command) {
        warnings.push(ForgeDiagnostic::new(
            "MISSING_OPTIONAL_HOST_TOOL",
            format!("{message} Missing command: {command}"),
        ));
    }
}

fn preflight_usb_device(
    device: &str,
    warnings: &mut Vec<ForgeDiagnostic>,
    errors: &mut Vec<ForgeDiagnostic>,
) {
    if crate::discover::detect_platform() == crate::discover::Platform::Darwin {
        let output = Command::new("diskutil").args(["info", device]).output();
        let Ok(output) = output else {
            errors.push(ForgeDiagnostic::new(
                "USB_DEVICE_NOT_FOUND",
                format!("diskutil cannot inspect target device {device}."),
            ));
            return;
        };
        if !output.status.success() {
            errors.push(ForgeDiagnostic::new(
                "USB_DEVICE_NOT_FOUND",
                format!("Target device does not exist or is not accessible: {device}."),
            ));
            return;
        }
        let info = String::from_utf8_lossy(&output.stdout);
        if !info.contains("Removable Media") && !info.contains("External") {
            errors.push(ForgeDiagnostic::new(
                "USB_DEVICE_NOT_REMOVABLE",
                format!("{device} does not appear to be removable/external media."),
            ));
        }
        if !device.starts_with("/dev/disk") {
            warnings.push(ForgeDiagnostic::new(
                "USB_DEVICE_NAME_UNUSUAL",
                format!("macOS target device should usually look like /dev/diskN, got {device}."),
            ));
        }
    } else if !Path::new(device).exists() {
        errors.push(ForgeDiagnostic::new(
            "USB_DEVICE_NOT_FOUND",
            format!("Target device does not exist: {device}."),
        ));
    }
}

fn ensure_flash_host_tools(is_macos: bool) -> Result<()> {
    let mut errors = Vec::new();
    require_command("sudo", &mut errors);
    require_command("dd", &mut errors);
    require_command("sync", &mut errors);
    if !command_exists("xorriso") {
        require_command("nix", &mut errors);
    }
    if is_macos {
        require_command("diskutil", &mut errors);
    }

    if errors.is_empty() {
        return Ok(());
    }

    for error in &errors {
        eprintln!(
            "{} {}: {}",
            style("!").red().bold(),
            error.code,
            error.message
        );
    }
    if !command_exists("xorriso") && command_exists("nix") {
        eprintln!(
            "  hint: xorriso will be run through {}",
            style("nix run nixpkgs#xorriso -- ...").cyan()
        );
    }
    bail!("missing required host tools for USB flashing");
}

fn execute_request(request: &ForgeRequest, events: EventMode) -> Result<()> {
    match request.operation {
        ForgeOperation::Bundle | ForgeOperation::UsbInstall => {
            emit_event(
                events,
                ForgeEvent::PhaseStarted {
                    schema_version: crate::forge::FORGE_SCHEMA_VERSION,
                    phase: "build-installer".to_string(),
                },
            )?;
            let arch = Arch::from(request.arch);
            let arch_label = arch.label().to_string();
            let disk = if request.operation == ForgeOperation::UsbInstall {
                request.target.disk.as_deref()
            } else {
                None
            };
            run_with_options(
                request.profile.as_deref(),
                Some(&request.hostname),
                disk,
                request.output_dir.as_deref(),
                Some(&arch_label),
                false,
                ForgeRunOptions {
                    prompt_optional_inputs: false,
                    allow_placeholder_nex: false,
                    polymerize_defaults: request.polymerize_defaults.clone(),
                },
            )?;
            emit_event(
                events,
                ForgeEvent::PhaseCompleted {
                    schema_version: crate::forge::FORGE_SCHEMA_VERSION,
                    phase: "build-installer".to_string(),
                },
            )?;
            Ok(())
        }
        ForgeOperation::Image => {
            bail!("forge run for image operations is not implemented yet; use forge plan/check")
        }
        ForgeOperation::Netboot => {
            bail!("forge run for netboot operations is not implemented yet")
        }
        ForgeOperation::RemotePolymerize => {
            bail!("forge run for remote polymerize operations is not implemented yet")
        }
    }
}

pub fn run_check(
    template_path: &Path,
    metadata_path: Option<&Path>,
    json: bool,
    no_execute: bool,
) -> Result<()> {
    let result = crate::forge::check_template(template_path, metadata_path, no_execute);
    let (report, exit_code) = match result {
        Ok(report) => {
            let exit_code = report.exit_code();
            (report, exit_code)
        }
        Err(error) => (
            crate::forge::ForgeCheckReport::evaluator_error(error.to_string()),
            2,
        ),
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else if report.valid {
        println!("forge template valid: {}", report.template.id);
    } else {
        println!("forge template invalid: {}", report.template.id);
        for error in &report.errors {
            println!("  {}: {}", error.code, error.message);
        }
    }

    std::process::exit(exit_code);
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

/// Fetch profile.toml content from a local path or GitHub.
fn fetch_profile_toml(repo_ref: &str) -> Result<String> {
    let path = Path::new(repo_ref);
    if path.exists() {
        let profile_path = if path.is_dir() {
            path.join("profile.toml")
        } else {
            path.to_path_buf()
        };

        if !profile_path.exists() {
            bail!(
                "local profile path {} does not contain profile.toml",
                path.display()
            );
        }

        return std::fs::read_to_string(&profile_path)
            .with_context(|| format!("failed to read {}", profile_path.display()));
    }

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
/// Strategy: nix cross-build (works from macOS) → GitHub release → self-copy → placeholder.
fn fetch_nex_binary(dest: &Path, arch: Arch) -> Result<()> {
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
                        .rfind(|l| l.starts_with("/nix/store/"))
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    let bin_check = Path::new(&store_path).join("bin/nex");
                    if !store_path.is_empty() && bin_check.exists() {
                        // Export the nix closure so it works on target
                        let bundle_dir = dest.parent().unwrap_or(Path::new("/tmp"));
                        let cache_dir = bundle_dir.join("nix-cache");
                        let nix_copy_status = Command::new("nix")
                            .args([
                                "copy",
                                "--to",
                                &format!("file://{}", cache_dir.display()),
                                &store_path,
                            ])
                            .status();
                        if !nix_copy_status.map(|s| s.success()).unwrap_or(false) {
                            eprintln!("  warning: nix copy failed — the bundled nex binary may not work on the target without network");
                        }

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
    let target = arch.target_triple();
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
    let mut home_lines = vec![
        "{ pkgs, username, ... }:".to_string(),
        String::new(),
        "{".to_string(),
        "  home = {".to_string(),
        "    username = username;".to_string(),
        "    homeDirectory = \"/home/${username}\";".to_string(),
        "    stateVersion = \"25.05\";".to_string(),
    ];
    home_lines.push("  };".to_string());
    home_lines.push(String::new());
    home_lines.push("  home.sessionPath = [".to_string());
    home_lines.push("    \"$HOME/.local/bin\"".to_string());
    if let Some(paths) = profile
        .get("shell")
        .and_then(|s| s.get("paths"))
        .and_then(|p| p.as_array())
    {
        for path in paths {
            if let Some(path_str) = path.as_str() {
                if path_str != "$HOME/.local/bin" && path_str != "~/.local/bin" {
                    home_lines.push(format!("    \"{path_str}\""));
                }
            }
        }
    }
    home_lines.push("  ];".to_string());
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
                lines.push(
                    "  environment.etc.\"dconf/db/local.d/01-nex-favorites\".text = ''".to_string(),
                );
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

    // Extra NixOS services
    if let Some(services) = linux.get("services").and_then(|v| v.as_array()) {
        let services: Vec<&str> = services.iter().filter_map(|v| v.as_str()).collect();
        if !services.is_empty() {
            lines.push("  # Extra services".to_string());
            for service in services {
                lines.push(format!("  services.{service}.enable = true;"));
            }
            lines.push(String::new());
        }
    }

    // Kernel parameters
    if let Some(params) = linux.get("kernel_params").and_then(|v| v.as_array()) {
        let params: Vec<String> = params
            .iter()
            .filter_map(|v| v.as_str())
            .map(nix_string)
            .collect();
        if !params.is_empty() {
            lines.push(format!("  boot.kernelParams = [ {} ];", params.join(" ")));
            lines.push(String::new());
        }
    }

    // Firewall ports
    if let Some(firewall) = linux.get("firewall") {
        if let Some(ports) = firewall.get("allowed_tcp_ports").and_then(|v| v.as_array()) {
            let ports: Vec<String> = ports
                .iter()
                .filter_map(|v| v.as_integer())
                .map(|p| p.to_string())
                .collect();
            if !ports.is_empty() {
                lines.push(format!(
                    "  networking.firewall.allowedTCPPorts = [ {} ];",
                    ports.join(" ")
                ));
            }
        }
        if let Some(ports) = firewall.get("allowed_udp_ports").and_then(|v| v.as_array()) {
            let ports: Vec<String> = ports
                .iter()
                .filter_map(|v| v.as_integer())
                .map(|p| p.to_string())
                .collect();
            if !ports.is_empty() {
                lines.push(format!(
                    "  networking.firewall.allowedUDPPorts = [ {} ];",
                    ports.join(" ")
                ));
            }
        }
        lines.push(String::new());
    }

    // K3s substrate support. Profiles may point at a token file, but should not
    // embed the token itself because Nix store paths are world-readable.
    if let Some(k3s) = linux.get("k3s") {
        let enabled = k3s.get("enable").and_then(|v| v.as_bool()).unwrap_or(true);
        if enabled {
            lines.push("  # K3s".to_string());
            lines.push("  services.k3s = {".to_string());
            lines.push("    enable = true;".to_string());
            let role = k3s.get("role").and_then(|v| v.as_str()).unwrap_or("server");
            lines.push(format!("    role = {};", nix_string(role)));

            if let Some(cluster_init) = k3s.get("cluster_init").and_then(|v| v.as_bool()) {
                lines.push(format!(
                    "    clusterInit = {};",
                    if cluster_init { "true" } else { "false" }
                ));
            }
            if let Some(disable_agent) = k3s.get("disable_agent").and_then(|v| v.as_bool()) {
                lines.push(format!(
                    "    disableAgent = {};",
                    if disable_agent { "true" } else { "false" }
                ));
            }
            if let Some(server_addr) = k3s.get("server_addr").and_then(|v| v.as_str()) {
                lines.push(format!("    serverAddr = {};", nix_string(server_addr)));
            }
            if let Some(token_file) = k3s.get("token_file").and_then(|v| v.as_str()) {
                lines.push(format!("    tokenFile = {};", nix_string(token_file)));
            }

            let mut flags: Vec<String> = Vec::new();
            if let Some(disabled) = k3s.get("disable").and_then(|v| v.as_array()) {
                for component in disabled.iter().filter_map(|v| v.as_str()) {
                    flags.push(format!("--disable={component}"));
                }
            }
            if let Some(extra_flags) = k3s.get("extra_flags").and_then(|v| v.as_array()) {
                for flag in extra_flags.iter().filter_map(|v| v.as_str()) {
                    flags.push(flag.to_string());
                }
            }
            if !flags.is_empty() {
                lines.push("    extraFlags = [".to_string());
                for flag in flags {
                    lines.push(format!("      {}", nix_string(&flag)));
                }
                lines.push("    ];".to_string());
            }
            lines.push("  };".to_string());
            lines.push(String::new());
        }
    }

    let mut extra_configs = Vec::new();
    if let Some(extra_config) = linux.get("extra_config").and_then(|v| v.as_str()) {
        extra_configs.push(extra_config);
    }
    if let Some(fragments) = linux
        .get("extra_config_fragments")
        .and_then(|v| v.as_array())
    {
        for fragment in fragments.iter().filter_map(|v| v.as_str()) {
            extra_configs.push(fragment);
        }
    }
    for extra_config in extra_configs {
        append_extra_nixos_config(lines, extra_config);
    }
}

fn nix_string(value: &str) -> String {
    format!("{value:?}")
}

fn append_extra_nixos_config(lines: &mut Vec<String>, extra_config: &str) {
    let extra_config = extra_config.trim();
    if extra_config.is_empty() {
        return;
    }

    lines.push("  # Extra NixOS config from profile".to_string());
    for line in extra_config.lines() {
        if line.trim().is_empty() {
            lines.push(String::new());
        } else {
            lines.push(format!("  {}", line));
        }
    }
    lines.push(String::new());
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

fn run_xorriso(args: &[String]) -> Result<std::process::ExitStatus> {
    if command_exists("xorriso") {
        return Command::new("xorriso")
            .args(args)
            .status()
            .context("failed to execute xorriso");
    }
    Command::new("nix")
        .args(["run", "nixpkgs#xorriso", "--"])
        .args(args)
        .status()
        .context("failed to execute xorriso through nix")
}

fn run_xorriso_output(args: &[String]) -> Result<std::process::Output> {
    if command_exists("xorriso") {
        return Command::new("xorriso")
            .args(args)
            .output()
            .context("failed to execute xorriso");
    }
    Command::new("nix")
        .args(["run", "nixpkgs#xorriso", "--"])
        .args(args)
        .output()
        .context("failed to execute xorriso through nix")
}

/// Build a bootable ISO that carries the styrene bundle at /styrene.
fn prepare_flash_iso_with_bundle(bundle_dir: &Path, iso_path: &Path) -> Result<std::path::PathBuf> {
    let bundled_iso = bundle_dir.join("nex-installer-with-styrene.iso");
    let styrene_dir = bundle_dir.join("styrene");
    let manifest = bundle_dir.join("bundle.yaml");

    if !styrene_dir.is_dir() {
        bail!(
            "missing styrene bundle directory: {}",
            styrene_dir.display()
        );
    }
    if !manifest.is_file() {
        bail!("missing forge bundle manifest: {}", manifest.display());
    }
    if bundled_iso.exists() {
        std::fs::remove_file(&bundled_iso).with_context(|| {
            format!(
                "failed to replace previous bundled ISO {}",
                bundled_iso.display()
            )
        })?;
    }

    output::status("embedding styrene installer payload into ISO...");
    let xorriso_args = [
        "-drive_access".to_string(),
        "exclusive:unrestricted".to_string(),
        "-indev".to_string(),
        iso_path.display().to_string(),
        "-outdev".to_string(),
        format!("stdio:{}", bundled_iso.display()),
        "-boot_image".to_string(),
        "any".to_string(),
        "replay".to_string(),
        "-map".to_string(),
        styrene_dir.display().to_string(),
        "/styrene".to_string(),
        "-map".to_string(),
        manifest.display().to_string(),
        "/bundle.yaml".to_string(),
    ];
    let status = run_xorriso(&xorriso_args)
        .context("failed to run xorriso to embed styrene payload into ISO")?;

    if !status.success() {
        bail!(
            "failed to embed styrene installer payload into ISO; refusing to flash an incomplete installer"
        );
    }

    let probe_args = [
        "-indev".to_string(),
        format!("stdio:{}", bundled_iso.display()),
        "-find".to_string(),
        "/styrene".to_string(),
        "-maxdepth".to_string(),
        "1".to_string(),
    ];
    let styrene_probe = run_xorriso_output(&probe_args).context("failed to inspect bundled ISO")?;
    if !styrene_probe.status.success()
        || !String::from_utf8_lossy(&styrene_probe.stdout).contains("/styrene")
    {
        bail!("bundled ISO verification failed: /styrene payload is not present");
    }

    Ok(bundled_iso)
}

/// Flash ISO to a USB device. The ISO must already contain the styrene payload.
fn flash_to_usb(iso_path: &Path, device: &str) -> Result<()> {
    println!();
    println!(
        "  {} Flashing to {}",
        style("!").yellow().bold(),
        style(device).bold()
    );

    // Safety: confirm device exists and is removable
    let is_macos = crate::discover::detect_platform() == crate::discover::Platform::Darwin;
    ensure_flash_host_tools(is_macos)?;

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

    let confirm = if std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        input().confirm("  Continue?", false)?
    } else {
        use std::io::Read;
        let mut answer = String::new();
        std::io::stdin()
            .read_to_string(&mut answer)
            .context("failed to read non-interactive flash confirmation")?;
        matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes")
    };

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

    // Strategy: embed the styrene payload into the bootable ISO before flashing,
    // then dd that complete image to the whole disk. Do not append a data
    // partition after writing the NixOS hybrid ISO; macOS/gptfdisk can reject
    // that layout with "Invalid partition data", which previously produced
    // bootable but incomplete installer media.

    // Unmount all partitions before dd
    if is_macos {
        let _ = Command::new("diskutil")
            .args(["unmountDisk", device])
            .status();
    }

    output::status("writing complete NixOS installer ISO to USB (this takes a few minutes)...");

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
        bail!("failed to write complete installer ISO to {device}");
    }
    let sync_status = Command::new("sync").status();
    if !sync_status.map(|s| s.success()).unwrap_or(false) {
        eprintln!("  warning: sync failed — data may not be fully flushed to disk");
    }

    if is_macos {
        let _ = Command::new("diskutil").args(["eject", device]).status();
    }

    println!();
    println!(
        "  {} USB ready — bootable NixOS ISO with embedded styrene installer payload.",
        style("✓").green().bold()
    );
    println!();
    println!("  If firmware reports a Secure Boot violation, disable Secure Boot for this boot.");
    println!("  The NixOS minimal installer is not Secure Boot signed.");
    println!();
    println!("  Boot from USB, then locate the mounted installer payload:");
    println!(
        "    {}",
        style("sudo find /run /mnt /media -maxdepth 5 -type f -name nex").cyan()
    );
    println!("  Then run polymerize with the discovered styrene directory as the bundle.");

    Ok(())
}

fn chrono_now() -> String {
    let output = Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_else(|| "unknown".to_string());
    output.trim().to_string()
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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
    fn test_generate_linux_config_services_kernel_firewall() {
        let profile: toml::Value = toml::from_str(
            r#"
            services = ["openssh"]
            kernel_params = ["quiet"]

            [firewall]
            allowed_tcp_ports = [22, 6443]
            allowed_udp_ports = [8472]
        "#,
        )
        .unwrap();
        let mut lines = Vec::new();
        generate_linux_config(&mut lines, &profile);
        let output = lines.join("\n");

        assert!(output.contains("services.openssh.enable = true"));
        assert!(output.contains("boot.kernelParams = [ \"quiet\" ];"));
        assert!(output.contains("networking.firewall.allowedTCPPorts = [ 22 6443 ];"));
        assert!(output.contains("networking.firewall.allowedUDPPorts = [ 8472 ];"));
    }

    #[test]
    fn test_generate_linux_config_k3s_server() {
        let profile: toml::Value = toml::from_str(
            r#"
            [k3s]
            enable = true
            role = "server"
            cluster_init = true
            token_file = "/var/lib/rancher/k3s/server/node-token"
            disable = ["traefik", "servicelb"]
            extra_flags = ["--write-kubeconfig-mode=0644", "--flannel-backend=vxlan"]
        "#,
        )
        .unwrap();
        let mut lines = Vec::new();
        generate_linux_config(&mut lines, &profile);
        let output = lines.join("\n");

        assert!(output.contains("services.k3s = {"));
        assert!(output.contains("role = \"server\";"));
        assert!(output.contains("clusterInit = true;"));
        assert!(output.contains("tokenFile = \"/var/lib/rancher/k3s/server/node-token\";"));
        assert!(output.contains("\"--disable=traefik\""));
        assert!(output.contains("\"--disable=servicelb\""));
        assert!(output.contains("\"--write-kubeconfig-mode=0644\""));
        assert!(!output.contains("token ="));
    }

    #[test]
    fn test_generate_linux_config_k3s_agent() {
        let profile: toml::Value = toml::from_str(
            r#"
            [k3s]
            role = "agent"
            server_addr = "https://192.168.0.50:6443"
            token_file = "/run/secrets/k3s-token"
        "#,
        )
        .unwrap();
        let mut lines = Vec::new();
        generate_linux_config(&mut lines, &profile);
        let output = lines.join("\n");

        assert!(output.contains("role = \"agent\";"));
        assert!(output.contains("serverAddr = \"https://192.168.0.50:6443\";"));
        assert!(output.contains("tokenFile = \"/run/secrets/k3s-token\";"));
    }

    #[test]
    fn test_generate_linux_config_extra_config() {
        let profile: toml::Value = toml::from_str(
            r#"
            extra_config = """
            virtualisation.docker.enable = true;
            services.haproxy.enable = true;
            """
            extra_config_fragments = [
              "networking.useDHCP = false;",
            ]
        "#,
        )
        .unwrap();
        let mut lines = Vec::new();
        generate_linux_config(&mut lines, &profile);
        let output = lines.join("\n");

        assert!(output.contains("# Extra NixOS config from profile"));
        assert!(output.contains("virtualisation.docker.enable = true;"));
        assert!(output.contains("services.haproxy.enable = true;"));
        assert!(output.contains("networking.useDHCP = false;"));
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
    fn test_resolve_profile_chain_local_directory() {
        let dir =
            std::env::temp_dir().join(format!("nex-forge-local-profile-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("profile.toml"),
            r#"
            [meta]
            name = "local-profile"

            [packages]
            nix = ["git"]
            "#,
        )
        .unwrap();

        let resolved = resolve_profile_chain(dir.to_str().unwrap()).unwrap();
        assert_eq!(resolved.chain, vec![dir.to_string_lossy().to_string()]);
        assert!(resolved.merged.contains("local-profile"));
        assert!(resolved.merged.contains("git"));

        let _ = std::fs::remove_dir_all(&dir);
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
