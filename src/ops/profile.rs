use std::collections::{BTreeMap, HashSet};
use std::process::Command;

use anyhow::{bail, Context, Result};
use console::style;

use crate::config::Config;
use crate::discover::Platform;
use crate::edit::{self, EditSession};
use crate::nixfile;
use crate::output;

/// Profile data parsed from a profile.toml
#[derive(Clone, serde::Deserialize)]
struct Profile {
    #[serde(alias = "fragment")]
    meta: Option<ProfileMeta>,
    packages: Option<ProfilePackages>,
    shell: Option<ProfileShell>,
    git: Option<ProfileGit>,
    kitty: Option<ProfileKitty>,
    macos: Option<ProfileMacos>,
    linux: Option<ProfileLinux>,
    security: Option<ProfileSecurity>,
}

#[derive(Clone, serde::Deserialize)]
struct ProfileMeta {
    name: Option<String>,
    description: Option<String>,
    extends: Option<String>,
    compose: Option<Vec<String>>,
}

#[derive(Clone, serde::Deserialize)]
#[allow(dead_code)]
struct ProfileKitty {
    font: Option<String>,
    font_size: Option<f64>,
    theme: Option<String>,
    window_padding: Option<u32>,
    scrollback_lines: Option<u32>,
    macos_option_as_alt: Option<bool>,
    macos_quit_when_last_window_closed: Option<bool>,
}

#[derive(Clone, serde::Deserialize)]
struct ProfilePackages {
    nix: Option<Vec<String>>,
    brews: Option<Vec<String>>,
    casks: Option<Vec<String>>,
    taps: Option<Vec<String>>,
}

#[derive(Clone, serde::Deserialize)]
struct ProfileShell {
    #[allow(dead_code)]
    default: Option<String>,
    aliases: Option<std::collections::HashMap<String, String>>,
    env: Option<std::collections::HashMap<String, String>>,
    #[serde(rename = "profileExtra")]
    profile_extra: Option<String>,
    #[serde(rename = "initExtra")]
    init_extra: Option<String>,
}

#[derive(Clone, serde::Deserialize)]
struct ProfileGit {
    name: Option<String>,
    email: Option<String>,
    default_branch: Option<String>,
    pull_rebase: Option<bool>,
    push_auto_setup_remote: Option<bool>,
}

#[derive(Clone, serde::Deserialize)]
#[allow(dead_code)]
struct ProfileMacos {
    show_all_extensions: Option<bool>,
    show_hidden_files: Option<bool>,
    auto_capitalize: Option<bool>,
    auto_correct: Option<bool>,
    natural_scroll: Option<bool>,
    tap_to_click: Option<bool>,
    three_finger_drag: Option<bool>,
    dock_autohide: Option<bool>,
    dock_show_recents: Option<bool>,
    fonts: Option<ProfileFonts>,
    // Extended settings
    dock: Option<ProfileDock>,
    appearance: Option<ProfileAppearance>,
    input: Option<ProfileInput>,
    finder: Option<ProfileFinder>,
    screenshots: Option<ProfileScreenshots>,
    default_apps: Option<ProfileDefaultApps>,
}

#[derive(Clone, serde::Deserialize)]
#[allow(dead_code)]
struct ProfileFonts {
    nerd: Option<Vec<String>>,
    families: Option<Vec<String>>,
}

#[derive(Clone, serde::Deserialize)]
#[allow(dead_code)]
struct ProfileDock {
    persistent_apps: Option<Vec<String>>,
    tile_size: Option<u32>,
    position: Option<String>,        // "bottom", "left", "right"
    minimize_effect: Option<String>, // "genie", "scale"
    magnification: Option<bool>,
    magnification_size: Option<u32>,
    launchanim: Option<bool>,
    mineffect: Option<String>,
    show_process_indicators: Option<bool>,
}

#[derive(Clone, serde::Deserialize)]
#[allow(dead_code)]
struct ProfileAppearance {
    dark_mode: Option<bool>,
    accent_color: Option<String>,
    highlight_color: Option<String>,
    reduce_transparency: Option<bool>,
    sidebar_icon_size: Option<u32>, // 1=small, 2=medium, 3=large
}

#[derive(Clone, serde::Deserialize)]
#[allow(dead_code)]
struct ProfileInput {
    key_repeat: Option<u32>,         // lower = faster (1-15, default 6)
    initial_key_repeat: Option<u32>, // lower = shorter delay (10-120, default 25)
    fn_as_standard: Option<bool>,    // true = F1..F12 are standard function keys
    press_and_hold: Option<bool>,    // false = enable key repeat instead of character picker
}

#[derive(Clone, serde::Deserialize)]
#[allow(dead_code)]
struct ProfileFinder {
    default_view: Option<String>, // "list", "icon", "column", "gallery"
    show_path_bar: Option<bool>,
    show_status_bar: Option<bool>,
    show_tab_bar: Option<bool>,
    new_window_path: Option<String>,
    search_scope: Option<String>, // "current", "previous", "computer"
    show_extensions: Option<bool>,
    warn_on_extension_change: Option<bool>,
}

#[derive(Clone, serde::Deserialize)]
#[allow(dead_code)]
struct ProfileScreenshots {
    location: Option<String>,
    format: Option<String>, // "png", "jpg", "pdf", "tiff"
    disable_shadow: Option<bool>,
}

#[derive(Clone, serde::Deserialize)]
#[allow(dead_code)]
struct ProfileDefaultApps {
    browser: Option<String>, // bundle id, e.g. "com.apple.Safari"
}

// ── Linux / NixOS profile structs ────────────────────────────────────────

#[derive(Clone, serde::Deserialize)]
#[allow(dead_code)]
struct ProfileLinux {
    desktop: Option<String>,         // "gnome", "kde", "cosmic"
    display_manager: Option<String>, // "gdm", "sddm", "greetd"
    gpu: Option<ProfileGpu>,
    audio: Option<ProfileAudio>,
    gaming: Option<ProfileGaming>,
    services: Option<Vec<String>>, // extra NixOS services to enable
    kernel_params: Option<Vec<String>>,
    gnome: Option<ProfileGnome>,
    kde: Option<ProfileKde>,
    cosmic: Option<ProfileCosmic>,
}

#[derive(Clone, serde::Deserialize)]
#[allow(dead_code)]
struct ProfileGpu {
    driver: Option<String>, // "amdgpu", "nvidia", "intel", "nouveau" (comma-separated for multi-GPU)
    vulkan: Option<bool>,
    opencl: Option<bool>,
    vaapi: Option<bool>, // hardware video acceleration
    #[serde(rename = "32bit")]
    lib32: Option<bool>, // 32-bit driver support (for Steam)
    nvidia_open: Option<bool>, // true for Turing+ (RTX 2000+), false for older (Kepler/Maxwell/Pascal)
}

#[derive(Clone, serde::Deserialize)]
#[allow(dead_code)]
struct ProfileAudio {
    backend: Option<String>, // "pipewire", "pulseaudio"
    low_latency: Option<bool>,
    bluetooth: Option<bool>,
}

#[derive(Clone, serde::Deserialize)]
#[allow(dead_code)]
struct ProfileGaming {
    steam: Option<bool>,
    gamemode: Option<bool>,
    mangohud: Option<bool>,
    gamescope: Option<bool>,
    controllers: Option<bool>, // enable game controller support
    proton_ge: Option<bool>,
}

#[derive(Clone, serde::Deserialize)]
#[allow(dead_code)]
struct ProfileGnome {
    dark_mode: Option<bool>,
    font_name: Option<String>,
    monospace_font: Option<String>,
    icon_theme: Option<String>,
    cursor_theme: Option<String>,
    button_layout: Option<String>,
    favorite_apps: Option<Vec<String>>,
    extensions: Option<Vec<String>>,
}

#[derive(Clone, serde::Deserialize)]
#[allow(dead_code)]
struct ProfileKde {
    color_scheme: Option<String>,
    icon_theme: Option<String>,
    cursor_theme: Option<String>,
    num_desktops: Option<u32>,
}

#[derive(Clone, serde::Deserialize)]
#[allow(dead_code)]
struct ProfileCosmic {
    dark_mode: Option<bool>,
    accent_color: Option<Vec<f64>>, // [r, g, b, a]
    dock_autohide: Option<bool>,
    dock_favorites: Option<Vec<String>>,
}

#[derive(Clone, serde::Deserialize)]
#[allow(dead_code)]
struct ProfileSecurity {
    touchid_sudo: Option<bool>,
}

/// A fetched profile plus its repo ref (needed for kitty file downloads).
struct ProfileLayer {
    repo_ref: String,
    profile: Profile,
}

/// Merged shell configuration across all profile layers.
#[derive(Default)]
struct MergedShell {
    aliases: BTreeMap<String, String>,
    env: BTreeMap<String, String>,
    profile_extra: Option<String>,
    init_extra: Option<String>,
    history_size: Option<u64>,
    history_file_size: Option<u64>,
    history_control: Option<Vec<String>>,
}

/// Merged git configuration. Last-writer-wins per field.
#[derive(Default)]
struct MergedGit {
    name: Option<String>,
    email: Option<String>,
    default_branch: Option<String>,
    pull_rebase: Option<bool>,
    push_auto_setup_remote: Option<bool>,
}

/// The single merged result of all profile layers.
struct MergedProfile {
    name: String,
    packages_nix: Vec<String>,
    packages_brews: Vec<String>,
    packages_casks: Vec<String>,
    packages_taps: Vec<String>,
    shell: MergedShell,
    git: MergedGit,
    kitty: Option<ProfileKitty>,
    macos: Option<ProfileMacos>,
    linux: Option<ProfileLinux>,
    security: Option<ProfileSecurity>,
}

/// Walk the extends chain and resolve compose fragments.
/// Returns layers in base-first order, ready for merging.
///
/// Resolution order for a profile with both extends and compose:
///   1. The extended parent (recursively resolved)
///   2. Each compose fragment in order
///   3. The profile's own inline sections (overrides)
fn collect_profiles(repo_ref: &str) -> Result<Vec<ProfileLayer>> {
    let mut layers = Vec::new();
    let mut visited = HashSet::new();
    collect_recursive(repo_ref, "profile.toml", &mut layers, &mut visited)?;
    Ok(layers)
}

fn collect_recursive(
    repo_ref: &str,
    file: &str,
    layers: &mut Vec<ProfileLayer>,
    visited: &mut HashSet<String>,
) -> Result<()> {
    let visit_key = format!("{repo_ref}:{file}");
    if !visited.insert(visit_key.clone()) {
        bail!("profile cycle detected: {visit_key}");
    }

    let profile = fetch_profile_file(repo_ref, file)?;

    // 1. If this profile extends another, resolve the parent first (base goes earliest)
    if let Some(parent_ref) = profile.meta.as_ref().and_then(|m| m.extends.clone()) {
        collect_recursive(&parent_ref, "profile.toml", layers, visited)?;
    }

    // 2. If this profile composes fragments, resolve each one.
    //    Fragments are paths within the SAME repo (e.g., "core/essentials" → "core/essentials.toml").
    if let Some(compose) = profile.meta.as_ref().and_then(|m| m.compose.clone()) {
        for fragment_path in &compose {
            let fragment_file = format!("{fragment_path}.toml");
            collect_recursive(repo_ref, &fragment_file, layers, visited)?;
        }
    }

    // 3. The profile itself is the final layer (its inline sections override fragments)
    layers.push(ProfileLayer {
        repo_ref: repo_ref.to_string(),
        profile,
    });

    Ok(())
}

impl MergedProfile {
    fn new() -> Self {
        Self {
            name: String::new(),
            packages_nix: Vec::new(),
            packages_brews: Vec::new(),
            packages_casks: Vec::new(),
            packages_taps: Vec::new(),
            shell: MergedShell::default(),
            git: MergedGit::default(),
            kitty: None,
            macos: None,
            linux: None,
            security: None,
        }
    }

    fn merge_layer(&mut self, layer: &ProfileLayer) {
        let profile = &layer.profile;

        // Name: use the outermost profile's name
        if let Some(meta) = &profile.meta {
            if let Some(name) = &meta.name {
                self.name = name.clone();
            }
        }

        // Packages: union with dedup
        if let Some(pkgs) = &profile.packages {
            union_dedup(&mut self.packages_nix, pkgs.nix.as_deref());
            union_dedup(&mut self.packages_brews, pkgs.brews.as_deref());
            union_dedup(&mut self.packages_casks, pkgs.casks.as_deref());
            union_dedup(&mut self.packages_taps, pkgs.taps.as_deref());
        }

        // Shell
        if let Some(shell) = &profile.shell {
            self.shell.merge_from(shell);
        }

        // Git: last-writer-wins per field
        if let Some(git) = &profile.git {
            if git.name.is_some() {
                self.git.name = git.name.clone();
            }
            if git.email.is_some() {
                self.git.email = git.email.clone();
            }
            if git.default_branch.is_some() {
                self.git.default_branch = git.default_branch.clone();
            }
            if git.pull_rebase.is_some() {
                self.git.pull_rebase = git.pull_rebase;
            }
            if git.push_auto_setup_remote.is_some() {
                self.git.push_auto_setup_remote = git.push_auto_setup_remote;
            }
        }

        // Kitty, macOS, Linux, Security: last-writer-wins (whole struct)
        if profile.kitty.is_some() {
            self.kitty = profile.kitty.clone();
        }
        if profile.macos.is_some() {
            self.macos = profile.macos.clone();
        }
        if profile.linux.is_some() {
            self.linux = profile.linux.clone();
        }
        if profile.security.is_some() {
            self.security = profile.security.clone();
        }
    }
}

fn union_dedup(target: &mut Vec<String>, source: Option<&[String]>) {
    if let Some(items) = source {
        let existing: HashSet<String> = target.iter().cloned().collect();
        for item in items {
            if !existing.contains(item) {
                target.push(item.clone());
            }
        }
    }
}

impl MergedShell {
    fn merge_from(&mut self, shell: &ProfileShell) {
        // Aliases: overlay wins on conflict
        if let Some(aliases) = &shell.aliases {
            for (k, v) in aliases {
                self.aliases.insert(k.clone(), v.clone());
            }
        }

        // Env vars: overlay wins on conflict
        if let Some(env) = &shell.env {
            for (k, v) in env {
                self.env.insert(k.clone(), v.clone());
            }
        }

        // Extract history settings from env into dedicated fields
        if let Some(val) = self.env.remove("HISTSIZE") {
            if let Ok(n) = val.parse::<u64>() {
                self.history_size = Some(n);
            }
        }
        if let Some(val) = self.env.remove("HISTFILESIZE") {
            if let Ok(n) = val.parse::<u64>() {
                self.history_file_size = Some(n);
            }
        }
        if let Some(val) = self.env.remove("HISTCONTROL") {
            self.history_control = Some(val.split(':').map(|s| s.to_string()).collect());
        }

        // profileExtra/initExtra: append if genuinely new
        Self::merge_multiline(&mut self.profile_extra, shell.profile_extra.as_deref());
        Self::merge_multiline(&mut self.init_extra, shell.init_extra.as_deref());
    }

    fn merge_multiline(target: &mut Option<String>, overlay: Option<&str>) {
        let overlay_trimmed = match overlay {
            Some(s) if !s.trim().is_empty() => s.trim(),
            _ => return,
        };
        match target {
            Some(existing) => {
                let existing_trimmed = existing.trim();
                if existing_trimmed.contains(overlay_trimmed) {
                    // Already present
                } else if overlay_trimmed.contains(existing_trimmed) {
                    *target = Some(overlay_trimmed.to_string());
                } else {
                    *target = Some(format!("{existing_trimmed}\n\n{overlay_trimmed}"));
                }
            }
            None => {
                *target = Some(overlay_trimmed.to_string());
            }
        }
    }
}

fn render_shell_nix(shell: &MergedShell) -> String {
    let mut lines = vec![
        "{ pkgs, ... }:".to_string(),
        String::new(),
        "{".to_string(),
        "  programs.bash.enable = true;".to_string(),
    ];

    if let Some(n) = shell.history_size {
        lines.push(format!("  programs.bash.historySize = {n};"));
    }
    if let Some(n) = shell.history_file_size {
        lines.push(format!("  programs.bash.historyFileSize = {n};"));
    }
    if let Some(ref items) = shell.history_control {
        let nix_list = items
            .iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(" ");
        lines.push(format!("  programs.bash.historyControl = [ {nix_list} ];"));
    }

    if !shell.aliases.is_empty() {
        lines.push("  programs.bash.shellAliases = {".to_string());
        for (name, cmd) in &shell.aliases {
            let escaped = escape_nix_string(cmd);
            lines.push(format!("    {name} = \"{escaped}\";"));
        }
        lines.push("  };".to_string());
    }

    render_multiline_attr(&mut lines, "profileExtra", &shell.profile_extra);
    render_multiline_attr(&mut lines, "initExtra", &shell.init_extra);

    if !shell.env.is_empty() {
        lines.push("  home.sessionVariables = {".to_string());
        for (key, val) in &shell.env {
            let escaped = escape_nix_string(val);
            lines.push(format!("    {key} = \"{escaped}\";"));
        }
        lines.push("  };".to_string());
    }

    // Ensure critical PATH directories are always present
    lines.push("  home.sessionPath = [".to_string());
    lines.push("    \"$HOME/.local/bin\"".to_string());
    lines.push("    \"$HOME/.cargo/bin\"".to_string());
    lines.push("    \"$HOME/.nix-profile/bin\"".to_string());
    lines.push("    \"/opt/homebrew/bin\"".to_string());
    lines.push("  ];".to_string());

    lines.push("}".to_string());
    lines.push(String::new());
    lines.join("\n")
}

fn escape_nix_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace("${", "\\${")
}

fn render_multiline_attr(lines: &mut Vec<String>, attr: &str, value: &Option<String>) {
    if let Some(ref text) = value {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            lines.push(format!(
                "  programs.bash.{attr} = ''\n{}\n  '';",
                indent_nix_multiline(trimmed, 4)
            ));
        }
    }
}

pub fn run(config: &Config, repo_ref: &str, dry_run: bool) -> Result<()> {
    // Phase 1: Collect all profile layers
    let layers = collect_profiles(repo_ref)?;

    // Print the chain
    if layers.len() > 1 {
        let chain: Vec<&str> = layers
            .iter()
            .filter_map(|l| l.profile.meta.as_ref().and_then(|m| m.name.as_deref()))
            .collect();
        println!();
        println!(
            "  {} profile chain: {}",
            style("nex profile").bold(),
            chain.join(" → ")
        );
        println!();
    } else if let Some(meta) = layers.last().and_then(|l| l.profile.meta.as_ref()) {
        println!();
        println!(
            "  {} applying profile {}",
            style("nex profile").bold(),
            style(meta.name.as_deref().unwrap_or(repo_ref)).cyan()
        );
        if let Some(desc) = &meta.description {
            println!("  {}", style(desc).dim());
        }
        println!();
    }

    // Phase 2: Merge all layers
    let mut merged = MergedProfile::new();
    for layer in &layers {
        merged.merge_layer(layer);
    }

    // Phase 3: Apply
    let mut session = EditSession::new();
    let mut changes = 0;

    // Packages
    let merged_pkgs = ProfilePackages {
        nix: if merged.packages_nix.is_empty() {
            None
        } else {
            Some(merged.packages_nix.clone())
        },
        brews: if merged.packages_brews.is_empty() {
            None
        } else {
            Some(merged.packages_brews.clone())
        },
        casks: if merged.packages_casks.is_empty() {
            None
        } else {
            Some(merged.packages_casks.clone())
        },
        taps: if merged.packages_taps.is_empty() {
            None
        } else {
            Some(merged.packages_taps.clone())
        },
    };
    changes += apply_nix_packages(config, &mut session, &merged_pkgs, dry_run)?;
    if config.platform == Platform::Darwin {
        changes += apply_brew_packages(config, &mut session, &merged_pkgs, dry_run)?;
        apply_taps(config, &merged_pkgs, dry_run)?;
    }

    // Kitty — per-layer (needs repo_ref for downloads)
    for layer in &layers {
        if layer.profile.kitty.is_some() {
            apply_kitty(config, &layer.repo_ref, &layer.profile.kitty, dry_run)?;
        }
    }

    // Shell — render once from merged data
    let has_shell = !merged.shell.aliases.is_empty()
        || !merged.shell.env.is_empty()
        || merged.shell.profile_extra.is_some()
        || merged.shell.init_extra.is_some()
        || merged.shell.history_size.is_some();

    if has_shell {
        if dry_run {
            output::dry_run("would apply shell configuration");
        } else {
            let scaffolded = config.repo.join("nix/modules/home").exists();
            let shell_nix = if scaffolded {
                config.repo.join("nix/modules/home/shell.nix")
            } else {
                config.repo.join("shell.nix")
            };

            let content = render_shell_nix(&merged.shell);
            if let Some(parent) = shell_nix.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&shell_nix, content)?;
            wire_shell_import(config, scaffolded)?;

            println!("  {} shell config written", style("✓").green());
            println!("    {}", style(shell_nix.display()).dim());
        }
    }

    // Git
    if merged.git.name.is_some() || merged.git.email.is_some() {
        apply_git_merged(config, &merged.git, dry_run)?;
    }

    // macOS
    if config.platform == Platform::Darwin {
        if let Some(ref macos) = merged.macos {
            apply_macos(config, macos, dry_run)?;
        }
    }

    // Linux
    if config.platform == Platform::Linux {
        if let Some(ref linux) = merged.linux {
            apply_linux(config, linux, dry_run)?;
        }
    }

    // Security
    if let Some(ref security) = merged.security {
        apply_security(config, security, dry_run)?;
    }

    if dry_run {
        println!();
        output::dry_run(&format!("{changes} package(s) would be added"));
        return Ok(());
    }

    if changes > 0 {
        session.commit_all()?;
        let _ = Command::new("git")
            .args(["add", "-A"])
            .current_dir(&config.repo)
            .output();
        let _ = Command::new("git")
            .args(["commit", "-m", &format!("nex profile apply: {repo_ref}")])
            .current_dir(&config.repo)
            .output();
    }

    let _ = crate::config::set_preference("profile", &format!("\"{repo_ref}\""));

    println!();
    println!(
        "  {} profile applied ({} packages added)",
        style("✓").green().bold(),
        changes
    );
    println!();
    println!("  Run {} to activate.", style("nex switch").bold());
    println!();

    Ok(())
}

/// Fetch a TOML file from a GitHub repo and parse it as a Profile.
/// For top-level profiles, `file` is "profile.toml".
/// For compose fragments, `file` is a path like "shell/bash.toml".
fn fetch_profile_file(repo_ref: &str, file: &str) -> Result<Profile> {
    let repo = if repo_ref.starts_with("http") {
        repo_ref.to_string()
    } else {
        repo_ref
            .trim_start_matches("github.com/")
            .trim_start_matches("https://github.com/")
            .to_string()
    };

    output::status(&format!("fetching {file} from {repo}..."));

    let content = fetch_toml_via_gh(&repo, file)
        .or_else(|_| fetch_toml_via_curl(&repo, file))
        .with_context(|| format!("could not fetch {file} from {repo}"))?;

    let profile: Profile =
        toml::from_str(&content).with_context(|| format!("invalid {file} from {repo}"))?;

    Ok(profile)
}

fn fetch_toml_via_gh(repo: &str, file: &str) -> Result<String> {
    let output = Command::new("gh")
        .args([
            "api",
            &format!("repos/{repo}/contents/{file}"),
            "-H",
            "Accept: application/vnd.github.raw+json",
        ])
        .output()
        .context("gh not available")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let hint = if stderr.contains("404") {
            format!("{file} not found in {repo} (check the path, or run `gh auth refresh -s repo`)")
        } else if stderr.contains("401") || stderr.contains("403") {
            format!(
                "access denied to {repo} — run `gh auth refresh -s repo` to grant private repo access"
            )
        } else {
            format!("gh api failed: {}", stderr.trim())
        };
        bail!("{hint}");
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn fetch_toml_via_curl(repo: &str, file: &str) -> Result<String> {
    let url = format!("https://raw.githubusercontent.com/{repo}/main/{file}");
    let output = Command::new("curl")
        .args(["-fsSL", &url])
        .output()
        .context("curl failed")?;

    if !output.status.success() {
        bail!("not available at {url} (private repo? use `gh auth login` first)");
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Add nix packages from the profile that aren't already declared.
fn apply_nix_packages(
    config: &Config,
    session: &mut EditSession,
    pkgs: &ProfilePackages,
    dry_run: bool,
) -> Result<usize> {
    let nix = match &pkgs.nix {
        Some(list) if !list.is_empty() => list,
        _ => return Ok(0),
    };

    // Gather what's already declared
    let mut existing = HashSet::new();
    for nix_file in config.all_nix_package_files() {
        for pkg in edit::list_packages(nix_file, &nixfile::NIX_PACKAGES)? {
            existing.insert(pkg);
        }
    }

    let new: Vec<&String> = nix.iter().filter(|p| !existing.contains(*p)).collect();
    if new.is_empty() {
        return Ok(0);
    }

    if dry_run {
        for pkg in &new {
            output::dry_run(&format!("would add nix package {pkg}"));
        }
        return Ok(new.len());
    }

    session.backup(&config.nix_packages_file)?;
    let mut added = 0;
    for pkg in &new {
        if edit::insert(&config.nix_packages_file, &nixfile::NIX_PACKAGES, pkg)? {
            println!("  {} {} {}", style("+").green(), pkg, style("(nix)").dim());
            added += 1;
        }
    }
    Ok(added)
}

/// Add brew formulae and casks from the profile.
fn apply_brew_packages(
    config: &Config,
    session: &mut EditSession,
    pkgs: &ProfilePackages,
    dry_run: bool,
) -> Result<usize> {
    let mut added = 0;

    // Brews
    if let Some(brews) = &pkgs.brews {
        let existing: HashSet<String> =
            edit::list_packages(&config.homebrew_file, &nixfile::HOMEBREW_BREWS)?
                .into_iter()
                .collect();
        let new: Vec<&String> = brews.iter().filter(|b| !existing.contains(*b)).collect();
        if !new.is_empty() {
            if dry_run {
                for b in &new {
                    output::dry_run(&format!("would add brew formula {b}"));
                }
                return Ok(new.len());
            }
            session.backup(&config.homebrew_file)?;
            for b in &new {
                if edit::insert(&config.homebrew_file, &nixfile::HOMEBREW_BREWS, b)? {
                    println!("  {} {} {}", style("+").green(), b, style("(brew)").dim());
                    added += 1;
                }
            }
        }
    }

    // Casks
    if let Some(casks) = &pkgs.casks {
        let existing: HashSet<String> =
            edit::list_packages(&config.homebrew_file, &nixfile::HOMEBREW_CASKS)?
                .into_iter()
                .collect();
        let new: Vec<&String> = casks.iter().filter(|c| !existing.contains(*c)).collect();
        if !new.is_empty() {
            if dry_run {
                for c in &new {
                    output::dry_run(&format!("would add brew cask {c}"));
                }
                return Ok(added + new.len());
            }
            session.backup(&config.homebrew_file)?;
            for c in &new {
                if edit::insert(&config.homebrew_file, &nixfile::HOMEBREW_CASKS, c)? {
                    println!("  {} {} {}", style("+").green(), c, style("(cask)").dim());
                    added += 1;
                }
            }
        }
    }

    Ok(added)
}

/// Add homebrew taps from the profile.
fn apply_taps(config: &Config, pkgs: &ProfilePackages, dry_run: bool) -> Result<()> {
    let taps = match &pkgs.taps {
        Some(list) if !list.is_empty() => list,
        _ => return Ok(()),
    };

    // Check if taps are declared in homebrew.nix
    let content = std::fs::read_to_string(&config.homebrew_file)
        .with_context(|| format!("reading {}", config.homebrew_file.display()))?;

    // If there's no taps section, we need to add one
    if !content.contains("taps = [") {
        if dry_run {
            for t in taps {
                output::dry_run(&format!("would add tap {t}"));
            }
            return Ok(());
        }
        // Insert taps block before the brews block
        let tap_lines: Vec<String> = taps.iter().map(|t| format!("      \"{t}\"")).collect();
        let tap_block = format!("\n    taps = [\n{}\n    ];\n", tap_lines.join("\n"));
        let patched = content.replace("    brews = [", &format!("{tap_block}    brews = ["));
        std::fs::write(&config.homebrew_file, patched)?;
    }

    Ok(())
}

/// Apply kitty terminal config by downloading files into the nix-darwin repo's
/// kitty-files directory (so home-manager picks them up on switch) AND into
/// ~/.config/kitty/ for immediate use.
fn apply_kitty(
    config: &Config,
    repo_ref: &str,
    kitty: &Option<ProfileKitty>,
    dry_run: bool,
) -> Result<()> {
    if dry_run {
        output::dry_run("would apply kitty configuration");
        return Ok(());
    }

    let repo = repo_ref
        .trim_start_matches("github.com/")
        .trim_start_matches("https://github.com/");

    // Download kitty directory tree
    let json_str = fetch_dir_listing(repo, "kitty").unwrap_or_default();
    let entries: Vec<serde_json::Value> = serde_json::from_str(&json_str).unwrap_or_default();
    if entries.is_empty() {
        return Ok(());
    }

    // Place into the nix-darwin repo so home-manager uses them on switch
    let repo_kitty_dir = config.repo.join("nix/modules/home/kitty-files");
    std::fs::create_dir_all(&repo_kitty_dir)?;
    download_tree(repo, "kitty", &entries, &repo_kitty_dir)?;

    // Also place directly into ~/.config/kitty/ for immediate use
    let user_kitty_dir = dirs::home_dir()
        .context("no home directory")?
        .join(".config/kitty");
    std::fs::create_dir_all(&user_kitty_dir)?;
    download_tree(repo, "kitty", &entries, &user_kitty_dir)?;

    if kitty.is_some() {
        println!("  {} kitty config applied", style("✓").green(),);
    }

    Ok(())
}

/// Fetch a GitHub directory listing via gh or curl.
fn fetch_dir_listing(repo: &str, path: &str) -> Result<String> {
    // Try gh first
    if let Ok(output) = Command::new("gh")
        .args(["api", &format!("repos/{repo}/contents/{path}?ref=main")])
        .output()
    {
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).to_string());
        }
    }

    // Fall back to curl
    let url = format!("https://api.github.com/repos/{repo}/contents/{path}?ref=main");
    let output = Command::new("curl")
        .args(["-fsSL", "-H", "Accept: application/vnd.github+json", &url])
        .output()
        .context("failed to list directory")?;

    if !output.status.success() {
        bail!("could not list {path} in {repo}");
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Download a file from a GitHub repo via gh (private) or raw.githubusercontent (public).
fn fetch_file(repo: &str, path: &str) -> Result<Vec<u8>> {
    // Try gh first
    if let Ok(output) = Command::new("gh")
        .args([
            "api",
            &format!("repos/{repo}/contents/{path}"),
            "-H",
            "Accept: application/vnd.github.raw+json",
        ])
        .output()
    {
        if output.status.success() {
            return Ok(output.stdout);
        }
    }

    // Fall back to raw URL
    let url = format!("https://raw.githubusercontent.com/{repo}/main/{path}");
    let output = Command::new("curl")
        .args(["-fsSL", &url])
        .output()
        .context("failed to download file")?;

    if !output.status.success() {
        bail!("could not download {path}");
    }

    Ok(output.stdout)
}

/// Recursively download files from a GitHub repo directory.
fn download_tree(
    repo: &str,
    path: &str,
    entries: &[serde_json::Value],
    local_dir: &std::path::Path,
) -> Result<()> {
    for entry in entries {
        let name = entry.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let entry_type = entry.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let entry_path = format!("{path}/{name}");

        if entry_type == "file" {
            let local_path = local_dir.join(name);
            if let Ok(data) = fetch_file(repo, &entry_path) {
                std::fs::write(&local_path, &data)?;
                println!(
                    "    {} {}",
                    style("+").green(),
                    style(local_path.display()).dim()
                );
            }
        } else if entry_type == "dir" {
            let subdir = local_dir.join(name);
            std::fs::create_dir_all(&subdir)?;
            if let Ok(listing) = fetch_dir_listing(repo, &entry_path) {
                let sub_entries: Vec<serde_json::Value> =
                    serde_json::from_str(&listing).unwrap_or_default();
                download_tree(repo, &entry_path, &sub_entries, &subdir)?;
            }
        }
    }
    Ok(())
}

/// Ensure shell.nix is imported by the home-manager configuration.
///
/// The host default.nix typically assigns home-manager.users.<user> to
/// `import ../../modules/home/base.nix`.  Since `import` yields an attrset
/// (not a module), we cannot put `imports = [...]` inside base.nix.
///
/// Instead, convert the home-manager user config to a module list:
///   home-manager.users.<user> = { imports = [ ./base.nix ./shell.nix ]; };
///
/// For flat layouts (home.nix), the file IS a module, so `imports` works directly.
fn wire_shell_import(config: &Config, scaffolded: bool) -> Result<()> {
    if scaffolded {
        // Find the host default.nix and patch the home-manager.users assignment
        let host_default = config
            .repo
            .join(format!("nix/hosts/{}/default.nix", config.hostname));
        if !host_default.exists() {
            println!(
                "  {} could not wire shell.nix import — {} not found",
                style("!").yellow(),
                style(host_default.display()).dim()
            );
            println!(
                "    Add {} to your home-manager imports manually",
                style("../../modules/home/shell.nix").bold()
            );
            return Ok(());
        }
        let content = std::fs::read_to_string(&host_default)?;
        if !content.contains("shell.nix") {
            // Current form: home-manager.users.${username} = import ../../modules/home/base.nix;
            // Target form:  home-manager.users.${username} = { imports = [ ... ]; };
            let patched = if content.contains("import ../../modules/home/base.nix") {
                content.replace(
                    "import ../../modules/home/base.nix;",
                    "{\n    imports = [\n      ../../modules/home/base.nix\n      ../../modules/home/shell.nix\n    ];\n  };",
                )
            } else if content.contains("../../modules/home/base.nix") {
                // Already uses imports list form — add shell.nix
                content.replace(
                    "../../modules/home/base.nix",
                    "../../modules/home/base.nix\n      ../../modules/home/shell.nix",
                )
            } else {
                println!(
                    "  {} could not find home-manager base.nix import in {}",
                    style("!").yellow(),
                    style(host_default.display()).dim()
                );
                println!(
                    "    Add {} to your home-manager imports manually",
                    style("../../modules/home/shell.nix").bold()
                );
                content
            };
            std::fs::write(&host_default, patched)?;
        }
    } else {
        // Flat layout: home.nix is a proper module, so imports work directly
        let home_nix = config.repo.join("home.nix");
        if home_nix.exists() {
            let content = std::fs::read_to_string(&home_nix)?;
            if !content.contains("shell.nix") {
                let patched = if content.contains("imports = [") {
                    content.replace("imports = [", "imports = [\n    ./shell.nix")
                } else {
                    content.replace("{\n", "{\n  imports = [ ./shell.nix ];\n\n")
                };
                std::fs::write(&home_nix, patched)?;
            }
        }
    }

    Ok(())
}

/// Indent each line of a multiline string for embedding in a nix '' string.
fn indent_nix_multiline(s: &str, spaces: usize) -> String {
    let indent = " ".repeat(spaces);
    s.lines()
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else {
                format!("{indent}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn apply_git_merged(_config: &Config, git: &MergedGit, dry_run: bool) -> Result<()> {
    if dry_run {
        output::dry_run("would apply git configuration");
        return Ok(());
    }
    if let Some(name) = &git.name {
        let _ = Command::new("git")
            .args(["config", "--global", "user.name", name])
            .output();
        println!("  {} git user.name = {}", style("✓").green(), name);
    }
    if let Some(email) = &git.email {
        let _ = Command::new("git")
            .args(["config", "--global", "user.email", email])
            .output();
        println!("  {} git user.email = {}", style("✓").green(), email);
    }
    if let Some(branch) = &git.default_branch {
        let _ = Command::new("git")
            .args(["config", "--global", "init.defaultBranch", branch])
            .output();
    }
    if git.pull_rebase == Some(true) {
        let _ = Command::new("git")
            .args(["config", "--global", "pull.rebase", "true"])
            .output();
    }
    if git.push_auto_setup_remote == Some(true) {
        let _ = Command::new("git")
            .args(["config", "--global", "push.autoSetupRemote", "true"])
            .output();
    }
    let _ = Command::new("git")
        .args([
            "config",
            "--global",
            "credential.https://github.com.helper",
            "!gh auth git-credential",
        ])
        .output();
    Ok(())
}

/// Apply macOS system defaults.
fn apply_macos(config: &Config, macos: &ProfileMacos, dry_run: bool) -> Result<()> {
    if dry_run {
        output::dry_run("would apply macOS preferences");
        return Ok(());
    }

    // ── Boolean NSGlobalDomain defaults ──────────────────────────────────
    let bool_defaults = [
        (
            "NSGlobalDomain",
            "AppleShowAllExtensions",
            macos.show_all_extensions,
        ),
        (
            "NSGlobalDomain",
            "AppleShowAllFiles",
            macos.show_hidden_files,
        ),
        (
            "NSGlobalDomain",
            "NSAutomaticCapitalizationEnabled",
            macos.auto_capitalize,
        ),
        (
            "NSGlobalDomain",
            "NSAutomaticSpellingCorrectionEnabled",
            macos.auto_correct,
        ),
        (
            "NSGlobalDomain",
            "com.apple.swipescrolldirection",
            macos.natural_scroll,
        ),
    ];

    for (domain, key, value) in &bool_defaults {
        if let Some(v) = value {
            defaults_write_bool(domain, key, *v);
        }
    }

    // ── Legacy dock booleans (top-level) ─────────────────────────────────
    if let Some(v) = macos.dock_autohide {
        defaults_write_bool("com.apple.dock", "autohide", v);
    }
    if let Some(v) = macos.dock_show_recents {
        defaults_write_bool("com.apple.dock", "show-recents", v);
    }

    // ── Trackpad ─────────────────────────────────────────────────────────
    if let Some(true) = macos.tap_to_click {
        defaults_write_bool("com.apple.AppleMultitouchTrackpad", "Clicking", true);
        defaults_write_bool(
            "com.apple.driver.AppleBluetoothMultitouch.trackpad",
            "Clicking",
            true,
        );
    }
    if let Some(true) = macos.three_finger_drag {
        defaults_write_bool(
            "com.apple.AppleMultitouchTrackpad",
            "TrackpadThreeFingerDrag",
            true,
        );
        defaults_write_bool(
            "com.apple.driver.AppleBluetoothMultitouch.trackpad",
            "TrackpadThreeFingerDrag",
            true,
        );
    }

    // ── Dock settings ────────────────────────────────────────────────────
    if let Some(dock) = &macos.dock {
        if let Some(size) = dock.tile_size {
            defaults_write_int("com.apple.dock", "tilesize", size);
        }
        if let Some(ref pos) = dock.position {
            defaults_write_string("com.apple.dock", "orientation", pos);
        }
        if let Some(ref effect) = dock.minimize_effect {
            defaults_write_string("com.apple.dock", "mineffect", effect);
        }
        if let Some(v) = dock.magnification {
            defaults_write_bool("com.apple.dock", "magnification", v);
        }
        if let Some(size) = dock.magnification_size {
            defaults_write_int("com.apple.dock", "largesize", size);
        }
        if let Some(v) = dock.launchanim {
            defaults_write_bool("com.apple.dock", "launchanim", v);
        }
        if let Some(v) = dock.show_process_indicators {
            defaults_write_bool("com.apple.dock", "show-process-indicators", v);
        }

        // Dock persistent apps via dockutil
        if let Some(ref apps) = dock.persistent_apps {
            apply_dock_apps(apps);
        }
    }

    // ── Appearance ───────────────────────────────────────────────────────
    if let Some(appearance) = &macos.appearance {
        if let Some(true) = appearance.dark_mode {
            defaults_write_string("NSGlobalDomain", "AppleInterfaceStyle", "Dark");
        } else if appearance.dark_mode == Some(false) {
            let _ = Command::new("defaults")
                .args(["delete", "NSGlobalDomain", "AppleInterfaceStyle"])
                .output();
        }
        if let Some(ref color) = appearance.accent_color {
            // macOS accent colors: Blue=-1(default), Purple=5, Pink=6, Red=0,
            // Orange=1, Yellow=2, Green=3, Graphite=-2
            let lowered = color.to_lowercase();
            let val = match lowered.as_str() {
                "blue" => "-1",
                "purple" => "5",
                "pink" => "6",
                "red" => "0",
                "orange" => "1",
                "yellow" => "2",
                "green" => "3",
                "graphite" => "-2",
                _ => &lowered,
            };
            defaults_write_string("NSGlobalDomain", "AppleAccentColor", val);
        }
        if let Some(ref color) = appearance.highlight_color {
            defaults_write_string("NSGlobalDomain", "AppleHighlightColor", color);
        }
        if let Some(v) = appearance.reduce_transparency {
            defaults_write_bool("com.apple.universalaccess", "reduceTransparency", v);
        }
        if let Some(size) = appearance.sidebar_icon_size {
            defaults_write_int("NSGlobalDomain", "NSTableViewDefaultSizeMode", size);
        }
    }

    // ── Input ────────────────────────────────────────────────────────────
    if let Some(input) = &macos.input {
        if let Some(rate) = input.key_repeat {
            defaults_write_int("NSGlobalDomain", "KeyRepeat", rate);
        }
        if let Some(delay) = input.initial_key_repeat {
            defaults_write_int("NSGlobalDomain", "InitialKeyRepeat", delay);
        }
        if let Some(v) = input.fn_as_standard {
            defaults_write_bool("NSGlobalDomain", "com.apple.keyboard.fnState", v);
        }
        if let Some(v) = input.press_and_hold {
            defaults_write_bool("NSGlobalDomain", "ApplePressAndHoldEnabled", v);
        }
    }

    // ── Finder ───────────────────────────────────────────────────────────
    if let Some(finder) = &macos.finder {
        if let Some(ref view) = finder.default_view {
            // Nlsv=list, icnv=icon, clmv=column, glyv=gallery
            let code = match view.as_str() {
                "list" => "Nlsv",
                "icon" => "icnv",
                "column" => "clmv",
                "gallery" => "glyv",
                other => other,
            };
            defaults_write_string("com.apple.finder", "FXPreferredViewStyle", code);
        }
        if let Some(v) = finder.show_path_bar {
            defaults_write_bool("com.apple.finder", "ShowPathbar", v);
        }
        if let Some(v) = finder.show_status_bar {
            defaults_write_bool("com.apple.finder", "ShowStatusBar", v);
        }
        if let Some(v) = finder.show_tab_bar {
            defaults_write_bool("com.apple.finder", "ShowTabView", v);
        }
        if let Some(ref path) = finder.new_window_path {
            // PfHm=home, PfDe=desktop, PfLo=custom path
            defaults_write_string("com.apple.finder", "NewWindowTarget", "PfLo");
            defaults_write_string("com.apple.finder", "NewWindowTargetPath", path);
        }
        if let Some(ref scope) = finder.search_scope {
            // SCcf=current folder, SCsp=previous scope, SCev=entire mac
            let val = match scope.as_str() {
                "current" => "SCcf",
                "previous" => "SCsp",
                "computer" => "SCev",
                other => other,
            };
            defaults_write_string("com.apple.finder", "FXDefaultSearchScope", val);
        }
        if let Some(v) = finder.show_extensions {
            defaults_write_bool("NSGlobalDomain", "AppleShowAllExtensions", v);
        }
        if let Some(v) = finder.warn_on_extension_change {
            defaults_write_bool("com.apple.finder", "FXEnableExtensionChangeWarning", v);
        }
    }

    // ── Screenshots ──────────────────────────────────────────────────────
    if let Some(ss) = &macos.screenshots {
        if let Some(ref loc) = ss.location {
            // Expand ~ for the defaults command
            let expanded = loc.replace(
                '~',
                &dirs::home_dir()
                    .map(|h| h.display().to_string())
                    .unwrap_or_default(),
            );
            defaults_write_string("com.apple.screencapture", "location", &expanded);
        }
        if let Some(ref fmt) = ss.format {
            defaults_write_string("com.apple.screencapture", "type", fmt);
        }
        if let Some(v) = ss.disable_shadow {
            defaults_write_bool("com.apple.screencapture", "disable-shadow", v);
        }
    }

    // ── Default apps ─────────────────────────────────────────────────────
    if let Some(apps) = &macos.default_apps {
        if let Some(ref browser) = apps.browser {
            // Resolve app name to bundle ID if needed
            let bundle_id = resolve_bundle_id(browser);
            let bid = bundle_id.as_deref().unwrap_or(browser);

            // Set default browser via open -a (works with app names)
            let _ = Command::new("open")
                .args(["-a", browser, "--args", "--make-default-browser"])
                .output();

            // Write the LSHandler with the proper bundle ID
            defaults_write_string(
                "com.apple.LaunchServices/com.apple.launchservices.secure",
                "LSHandlerURLSchemeHTTP",
                bid,
            );
        }
    }

    // ── Restart affected services ────────────────────────────────────────
    let needs_dock_restart =
        macos.dock.is_some() || macos.dock_autohide.is_some() || macos.dock_show_recents.is_some();
    let needs_finder_restart = macos.finder.is_some();

    if needs_dock_restart {
        let _ = Command::new("killall").arg("Dock").output();
    }
    if needs_finder_restart {
        let _ = Command::new("killall").arg("Finder").output();
    }
    if macos.screenshots.is_some() {
        let _ = Command::new("killall").arg("SystemUIServer").output();
    }

    // ── Write system.defaults to base.nix for declarative management ────
    write_system_defaults(config, macos)?;

    println!("  {} macOS preferences applied", style("✓").green());

    Ok(())
}

// ── Helper functions for defaults write ──────────────────────────────────

/// Resolve an app name (e.g. "Safari") to its macOS bundle ID (e.g. "com.apple.Safari").
/// Returns None if resolution fails (caller should use the original string).
fn resolve_bundle_id(app_name: &str) -> Option<String> {
    // If it already looks like a bundle ID, return as-is
    if app_name.contains('.') {
        return Some(app_name.to_string());
    }

    // Use mdls to query the bundle identifier from the app
    let app_path = format!("/Applications/{app_name}.app");
    let output = Command::new("mdls")
        .args(["-name", "kMDItemCFBundleIdentifier", "-raw", &app_path])
        .output()
        .ok()?;

    if output.status.success() {
        let bid = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !bid.is_empty() && bid != "(null)" {
            return Some(bid);
        }
    }

    // Well-known fallbacks
    match app_name {
        "Safari" => Some("com.apple.Safari".to_string()),
        "Firefox" => Some("org.mozilla.firefox".to_string()),
        "Chrome" | "Google Chrome" => Some("com.google.Chrome".to_string()),
        "Arc" => Some("company.thebrowser.Browser".to_string()),
        "Brave" | "Brave Browser" => Some("com.brave.Browser".to_string()),
        _ => None,
    }
}

fn defaults_write_bool(domain: &str, key: &str, value: bool) {
    let val = if value { "true" } else { "false" };
    let _ = Command::new("defaults")
        .args(["write", domain, key, "-bool", val])
        .output();
}

fn defaults_write_int(domain: &str, key: &str, value: u32) {
    let _ = Command::new("defaults")
        .args(["write", domain, key, "-int", &value.to_string()])
        .output();
}

fn defaults_write_string(domain: &str, key: &str, value: &str) {
    let _ = Command::new("defaults")
        .args(["write", domain, key, "-string", value])
        .output();
}

/// Set dock persistent apps using dockutil (if available) or direct plist manipulation.
fn apply_dock_apps(apps: &[String]) {
    // Check for dockutil
    let has_dockutil = Command::new("which")
        .arg("dockutil")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !has_dockutil {
        println!(
            "  {} install dockutil to manage dock apps: {}",
            style("!").yellow(),
            style("brew install dockutil").bold()
        );
        return;
    }

    // Remove all existing items
    let _ = Command::new("dockutil")
        .args(["--remove", "all", "--no-restart"])
        .output();

    // Add each app
    for app in apps {
        let path = if app.starts_with('/') {
            app.clone()
        } else {
            format!("/Applications/{app}.app")
        };
        let _ = Command::new("dockutil")
            .args(["--add", &path, "--no-restart"])
            .output();
        println!("    {} {}", style("+").green(), style(&path).dim());
    }

    // Restart dock once at the end
    let _ = Command::new("killall").arg("Dock").output();
}

/// Write a system.defaults block into darwin/base.nix for nix-darwin declarative management.
fn write_system_defaults(config: &Config, macos: &ProfileMacos) -> Result<()> {
    let base_nix = config.repo.join("nix/modules/darwin/base.nix");
    let content = std::fs::read_to_string(&base_nix)
        .with_context(|| format!("reading {}", base_nix.display()))?;

    // Build system.defaults block
    let mut defaults_lines: Vec<String> = Vec::new();

    // NSGlobalDomain
    let mut nsg: Vec<String> = Vec::new();
    if let Some(v) = macos.show_all_extensions {
        nsg.push(format!("    AppleShowAllExtensions = {};", v));
    }
    if let Some(v) = macos.show_hidden_files {
        nsg.push(format!("    AppleShowAllFiles = {};", v));
    }
    if let Some(v) = macos.auto_capitalize {
        nsg.push(format!("    NSAutomaticCapitalizationEnabled = {};", v));
    }
    if let Some(v) = macos.auto_correct {
        nsg.push(format!("    NSAutomaticSpellingCorrectionEnabled = {};", v));
    }
    if let Some(v) = macos.natural_scroll {
        nsg.push(format!("    \"com.apple.swipescrolldirection\" = {};", v));
    }
    if let Some(appearance) = &macos.appearance {
        if let Some(true) = appearance.dark_mode {
            nsg.push("    AppleInterfaceStyle = \"Dark\";".to_string());
        }
        if let Some(size) = appearance.sidebar_icon_size {
            nsg.push(format!("    NSTableViewDefaultSizeMode = {};", size));
        }
    }
    if let Some(input) = &macos.input {
        if let Some(rate) = input.key_repeat {
            nsg.push(format!("    KeyRepeat = {};", rate));
        }
        if let Some(delay) = input.initial_key_repeat {
            nsg.push(format!("    InitialKeyRepeat = {};", delay));
        }
        if let Some(v) = input.press_and_hold {
            nsg.push(format!("    ApplePressAndHoldEnabled = {};", v));
        }
    }
    if !nsg.is_empty() {
        defaults_lines.push("  NSGlobalDomain = {".to_string());
        defaults_lines.extend(nsg);
        defaults_lines.push("  };".to_string());
    }

    // dock
    let mut dock: Vec<String> = Vec::new();
    if let Some(v) = macos.dock_autohide {
        dock.push(format!("    autohide = {};", v));
    }
    if let Some(v) = macos.dock_show_recents {
        dock.push(format!("    show-recents = {};", v));
    }
    if let Some(d) = &macos.dock {
        if let Some(size) = d.tile_size {
            dock.push(format!("    tilesize = {};", size));
        }
        if let Some(ref pos) = d.position {
            dock.push(format!("    orientation = \"{}\";", pos));
        }
        if let Some(ref effect) = d.minimize_effect {
            dock.push(format!("    mineffect = \"{}\";", effect));
        }
        if let Some(v) = d.magnification {
            dock.push(format!("    magnification = {};", v));
        }
        if let Some(size) = d.magnification_size {
            dock.push(format!("    largesize = {};", size));
        }
        if let Some(v) = d.launchanim {
            dock.push(format!("    launchanim = {};", v));
        }
        if let Some(v) = d.show_process_indicators {
            dock.push(format!("    show-process-indicators = {};", v));
        }
    }
    if !dock.is_empty() {
        defaults_lines.push("  dock = {".to_string());
        defaults_lines.extend(dock);
        defaults_lines.push("  };".to_string());
    }

    // finder
    let mut finder: Vec<String> = Vec::new();
    if let Some(f) = &macos.finder {
        if let Some(ref view) = f.default_view {
            let code = match view.as_str() {
                "list" => "Nlsv",
                "icon" => "icnv",
                "column" => "clmv",
                "gallery" => "glyv",
                other => other,
            };
            finder.push(format!("    FXPreferredViewStyle = \"{}\";", code));
        }
        if let Some(v) = f.show_path_bar {
            finder.push(format!("    ShowPathbar = {};", v));
        }
        if let Some(v) = f.show_status_bar {
            finder.push(format!("    ShowStatusBar = {};", v));
        }
        if let Some(ref scope) = f.search_scope {
            let val = match scope.as_str() {
                "current" => "SCcf",
                "previous" => "SCsp",
                "computer" => "SCev",
                other => other,
            };
            finder.push(format!("    FXDefaultSearchScope = \"{}\";", val));
        }
        if let Some(v) = f.warn_on_extension_change {
            finder.push(format!("    FXEnableExtensionChangeWarning = {};", v));
        }
    }
    if !finder.is_empty() {
        defaults_lines.push("  finder = {".to_string());
        defaults_lines.extend(finder);
        defaults_lines.push("  };".to_string());
    }

    // screencapture
    let mut screencap: Vec<String> = Vec::new();
    if let Some(ss) = &macos.screenshots {
        if let Some(ref loc) = ss.location {
            screencap.push(format!("    location = \"{}\";", loc));
        }
        if let Some(ref fmt) = ss.format {
            screencap.push(format!("    type = \"{}\";", fmt));
        }
        if let Some(v) = ss.disable_shadow {
            screencap.push(format!("    disable-shadow = {};", v));
        }
    }
    if !screencap.is_empty() {
        defaults_lines.push("  screencapture = {".to_string());
        defaults_lines.extend(screencap);
        defaults_lines.push("  };".to_string());
    }

    // trackpad
    let mut trackpad: Vec<String> = Vec::new();
    if let Some(true) = macos.tap_to_click {
        trackpad.push("    Clicking = true;".to_string());
    }
    if let Some(true) = macos.three_finger_drag {
        trackpad.push("    TrackpadThreeFingerDrag = true;".to_string());
    }
    if !trackpad.is_empty() {
        defaults_lines.push("  trackpad = {".to_string());
        defaults_lines.extend(trackpad);
        defaults_lines.push("  };".to_string());
    }

    if defaults_lines.is_empty() {
        return Ok(());
    }

    let defaults_block = format!(
        "\n  system.defaults = {{\n{}\n  }};\n",
        defaults_lines.join("\n")
    );

    // Insert or replace the system.defaults block
    if content.contains("system.defaults = {") {
        // Replace existing block — find from "system.defaults = {" to its closing "};"
        let start = match content.find("  system.defaults = {") {
            Some(pos) => pos,
            None => {
                // Shouldn't happen given the contains() check, but be safe
                let insert_pos = content.rfind('}').unwrap_or(content.len());
                let mut patched = content[..insert_pos].to_string();
                patched.push_str(&defaults_block);
                patched.push('}');
                if content.ends_with('\n') {
                    patched.push('\n');
                }
                std::fs::write(&base_nix, patched)?;
                return Ok(());
            }
        };

        // Find the matching closing brace by counting depth from the first '{'
        let after_start = &content[start..];
        let mut depth: i32 = 0;
        let mut end = content.len(); // fallback: end of file
        let mut found = false;
        for (i, c) in after_start.char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        // Consume trailing `;` and newline if present
                        let mut consume_end = start + i + 1;
                        let remaining = &content[consume_end..];
                        if remaining.starts_with(";\n") {
                            consume_end += 2;
                        } else if remaining.starts_with(';') || remaining.starts_with('\n') {
                            consume_end += 1;
                        }
                        end = consume_end;
                        found = true;
                        break;
                    }
                }
                _ => {}
            }
        }

        if !found {
            // Malformed: unmatched braces — append instead of replacing
            let insert_pos = content.rfind('}').unwrap_or(content.len());
            let mut patched = content[..insert_pos].to_string();
            patched.push_str(&defaults_block);
            patched.push('}');
            if content.ends_with('\n') {
                patched.push('\n');
            }
            std::fs::write(&base_nix, patched)?;
            return Ok(());
        }

        let mut patched = content[..start].to_string();
        patched.push_str(&format!(
            "  system.defaults = {{\n{}\n  }};\n",
            defaults_lines.join("\n")
        ));
        patched.push_str(&content[end..]);
        std::fs::write(&base_nix, patched)?;
    } else {
        // Insert before the closing "}" of the module
        let insert_pos = content.rfind('}').unwrap_or(content.len());
        let mut patched = content[..insert_pos].to_string();
        patched.push_str(&defaults_block);
        patched.push('}');
        if content.ends_with('\n') {
            patched.push('\n');
        }
        std::fs::write(&base_nix, patched)?;
    }

    Ok(())
}

/// Apply security settings.
fn apply_security(_config: &Config, _security: &ProfileSecurity, dry_run: bool) -> Result<()> {
    if dry_run {
        output::dry_run("would configure security settings");
        return Ok(());
    }
    // TouchID sudo is handled by the nix-darwin module (security.nix)
    // which the scaffold already includes.
    Ok(())
}

/// Apply Linux / NixOS system configuration.
/// Writes a NixOS module to nix/modules/nixos/desktop.nix that configures
/// the desktop environment, GPU drivers, gaming stack, and audio.
fn apply_linux(config: &Config, linux: &ProfileLinux, dry_run: bool) -> Result<()> {
    if dry_run {
        output::dry_run("would apply Linux desktop configuration");
        return Ok(());
    }

    let mut lines: Vec<String> = Vec::new();
    lines.push("{ pkgs, lib, ... }:".to_string());
    lines.push(String::new());
    lines.push("{".to_string());

    // ── Desktop environment ──────────────────────────────────────────
    if let Some(ref de) = linux.desktop {
        match de.as_str() {
            "gnome" => {
                lines.push("  services.xserver.enable = true;".to_string());
                lines.push("  services.xserver.displayManager.gdm.enable = true;".to_string());
                lines.push("  services.xserver.desktopManager.gnome.enable = true;".to_string());
            }
            "kde" | "plasma" => {
                lines.push("  services.desktopManager.plasma6.enable = true;".to_string());
                lines.push("  services.displayManager.sddm.enable = true;".to_string());
                lines.push("  services.displayManager.sddm.wayland.enable = true;".to_string());
            }
            "cosmic" => {
                lines.push("  services.desktopManager.cosmic.enable = true;".to_string());
                lines.push("  services.displayManager.cosmic-greeter.enable = true;".to_string());
            }
            _ => {}
        }
    }

    // Override display manager if explicitly set
    if let Some(ref dm) = linux.display_manager {
        match dm.as_str() {
            "gdm" => {
                lines.push(
                    "  services.xserver.displayManager.gdm.enable = lib.mkForce true;".to_string(),
                );
            }
            "sddm" => {
                lines.push("  services.displayManager.sddm.enable = lib.mkForce true;".to_string());
            }
            "greetd" => {
                lines.push("  services.greetd.enable = true;".to_string());
            }
            _ => {}
        }
    }

    // ── GPU drivers ──────────────────────────────────────────────────
    if let Some(ref gpu) = linux.gpu {
        if let Some(ref driver) = gpu.driver {
            lines.push(String::new());
            lines.push("  hardware.graphics.enable = true;".to_string());
            if gpu.lib32 == Some(true) {
                lines.push("  hardware.graphics.enable32Bit = true;".to_string());
            }

            // Support comma-separated multi-GPU: "amdgpu,nvidia"
            let drivers: Vec<&str> = driver.split(',').map(|d| d.trim()).collect();
            let mut video_drivers: Vec<&str> = Vec::new();
            let mut extra_packages: Vec<&str> = Vec::new();

            for drv in &drivers {
                match *drv {
                    "amdgpu" => {
                        lines.push("  # GPU: AMD".to_string());
                        lines.push("  hardware.amdgpu.initrd.enable = true;".to_string());
                        if gpu.opencl == Some(true) {
                            lines.push("  hardware.amdgpu.opencl.enable = true;".to_string());
                        }
                        if gpu.vaapi == Some(true) {
                            extra_packages.push("libva-vdpau-driver");
                        }
                    }
                    "nvidia" => {
                        lines.push("  # GPU: NVIDIA".to_string());
                        video_drivers.push("nvidia");
                        lines.push("  hardware.nvidia.modesetting.enable = true;".to_string());
                        // nvidia.open = true only works on Turing+ (RTX 2000+)
                        // Default true for modern cards; set nvidia_open = false in profile for older
                        let open = gpu.nvidia_open.unwrap_or(true);
                        lines.push(format!("  hardware.nvidia.open = {};", open));
                    }
                    "nouveau" => {
                        lines.push("  # GPU: NVIDIA (nouveau)".to_string());
                        video_drivers.push("nouveau");
                    }
                    "intel" => {
                        lines.push("  # GPU: Intel".to_string());
                        if gpu.vaapi == Some(true) {
                            extra_packages.push("intel-media-driver");
                        }
                    }
                    _ => {}
                }
            }

            if !video_drivers.is_empty() {
                let vd = video_drivers
                    .iter()
                    .map(|d| format!("\"{d}\""))
                    .collect::<Vec<_>>()
                    .join(" ");
                lines.push(format!("  services.xserver.videoDrivers = [ {vd} ];"));
            }
            if !extra_packages.is_empty() {
                lines.push("  hardware.graphics.extraPackages = with pkgs; [".to_string());
                for pkg in &extra_packages {
                    lines.push(format!("    {pkg}"));
                }
                lines.push("  ];".to_string());
            }
        }
    }

    // ── Audio ────────────────────────────────────────────────────────
    if let Some(ref audio) = linux.audio {
        lines.push(String::new());
        lines.push("  # Audio".to_string());
        match audio.backend.as_deref() {
            Some("pipewire") | None => {
                lines.push("  services.pipewire = {".to_string());
                lines.push("    enable = true;".to_string());
                lines.push("    alsa.enable = true;".to_string());
                lines.push("    alsa.support32Bit = true;".to_string());
                lines.push("    pulse.enable = true;".to_string());
                if audio.low_latency == Some(true) {
                    lines.push("    extraConfig.pipewire.\"92-low-latency\" = {".to_string());
                    lines.push("      \"context.properties\" = { \"default.clock.rate\" = 48000; \"default.clock.quantum\" = 64; };".to_string());
                    lines.push("    };".to_string());
                }
                lines.push("  };".to_string());
            }
            Some("pulseaudio") => {
                lines.push("  hardware.pulseaudio.enable = true;".to_string());
            }
            _ => {}
        }
        if audio.bluetooth == Some(true) {
            lines.push("  hardware.bluetooth.enable = true;".to_string());
            lines.push("  hardware.bluetooth.powerOnBoot = true;".to_string());
        }
    }

    // ── Gaming ───────────────────────────────────────────────────────
    if let Some(ref gaming) = linux.gaming {
        lines.push(String::new());
        lines.push("  # Gaming".to_string());
        if gaming.steam == Some(true) {
            lines.push("  programs.steam = {".to_string());
            lines.push("    enable = true;".to_string());
            lines.push(
                "    gamescopeSession.enable = {};"
                    .replace(
                        "{}",
                        if gaming.gamescope == Some(true) {
                            "true"
                        } else {
                            "false"
                        },
                    )
                    .to_string(),
            );
            lines.push("  };".to_string());
        }
        if gaming.gamemode == Some(true) {
            lines.push("  programs.gamemode.enable = true;".to_string());
        }
        if gaming.controllers == Some(true) {
            lines.push("  hardware.steam-hardware.enable = true;".to_string());
        }

        // Gaming packages
        let mut gaming_pkgs: Vec<&str> = Vec::new();
        if gaming.mangohud == Some(true) {
            gaming_pkgs.push("mangohud");
        }
        if gaming.proton_ge == Some(true) {
            // proton-ge is installed via Steam compatibility tools, not as a system package
        }
        if !gaming_pkgs.is_empty() {
            lines.push("  environment.systemPackages = with pkgs; [".to_string());
            for pkg in &gaming_pkgs {
                lines.push(format!("    {pkg}"));
            }
            lines.push("  ];".to_string());
        }
    }

    // ── Extra services ───────────────────────────────────────────────
    if let Some(ref services) = linux.services {
        lines.push(String::new());
        for svc in services {
            lines.push(format!("  services.{svc}.enable = true;"));
        }
    }

    // ── Kernel parameters ────────────────────────────────────────────
    if let Some(ref params) = linux.kernel_params {
        lines.push(String::new());
        let params_str = params
            .iter()
            .map(|p| format!("\"{p}\""))
            .collect::<Vec<_>>()
            .join(" ");
        lines.push(format!("  boot.kernelParams = [ {params_str} ];"));
    }

    lines.push("}".to_string());
    lines.push(String::new());

    // Detect layout: scaffolded (nix/modules/nixos/) vs flat (/etc/nixos/)
    let scaffolded =
        config.repo.join("nix/modules/nixos").exists() || config.repo.join("nix/hosts").exists();

    let desktop_nix = if scaffolded {
        config.repo.join("nix/modules/nixos/desktop.nix")
    } else {
        // Flat layout (e.g., /etc/nixos from polymerize)
        config.repo.join("desktop.nix")
    };

    if let Some(parent) = desktop_nix.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&desktop_nix, lines.join("\n"))?;

    // Ensure the desktop module is imported by the main config
    if scaffolded {
        // Scaffolded layout: patch nix/hosts/{hostname}/default.nix
        let host_default = config
            .repo
            .join(format!("nix/hosts/{}/default.nix", config.hostname));
        if host_default.exists() {
            let content = std::fs::read_to_string(&host_default)?;
            if !content.contains("desktop.nix") {
                let patched = content.replace(
                    "../../modules/nixos/base.nix",
                    "../../modules/nixos/base.nix\n    ../../modules/nixos/desktop.nix",
                );
                std::fs::write(&host_default, patched)?;
            }
        }
    } else {
        // Flat layout: patch configuration.nix to import ./desktop.nix
        let config_nix = config.repo.join("configuration.nix");
        if config_nix.exists() {
            let content = std::fs::read_to_string(&config_nix)?;
            if !content.contains("desktop.nix") {
                // Insert import after the opening "{"
                if let Some(brace_pos) = content.find('{') {
                    let mut patched = content[..=brace_pos].to_string();
                    patched.push_str("\n  imports = [ ./desktop.nix ];");
                    patched.push_str(&content[brace_pos + 1..]);
                    std::fs::write(&config_nix, patched)?;
                }
            }
        }
    }

    println!(
        "  {} Linux desktop configuration written",
        style("✓").green()
    );
    println!("    {}", style(desktop_nix.display()).dim());

    Ok(())
}
