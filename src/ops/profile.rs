use std::collections::HashSet;
use std::process::Command;

use anyhow::{bail, Context, Result};
use console::style;

use crate::config::Config;
use crate::edit::{self, EditSession};
use crate::nixfile;
use crate::output;

/// Profile data parsed from a profile.toml
#[derive(serde::Deserialize)]
struct Profile {
    meta: Option<ProfileMeta>,
    packages: Option<ProfilePackages>,
    shell: Option<ProfileShell>,
    git: Option<ProfileGit>,
    kitty: Option<ProfileKitty>,
    macos: Option<ProfileMacos>,
    security: Option<ProfileSecurity>,
}

#[derive(serde::Deserialize)]
struct ProfileMeta {
    name: Option<String>,
    description: Option<String>,
    extends: Option<String>,
}

#[derive(serde::Deserialize)]
struct ProfileKitty {
    font: Option<String>,
    font_size: Option<f64>,
    theme: Option<String>,
    window_padding: Option<u32>,
    scrollback_lines: Option<u32>,
    macos_option_as_alt: Option<bool>,
    macos_quit_when_last_window_closed: Option<bool>,
}

#[derive(serde::Deserialize)]
struct ProfilePackages {
    nix: Option<Vec<String>>,
    brews: Option<Vec<String>>,
    casks: Option<Vec<String>>,
    taps: Option<Vec<String>>,
}

#[derive(serde::Deserialize)]
struct ProfileShell {
    default: Option<String>,
    aliases: Option<std::collections::HashMap<String, String>>,
    env: Option<std::collections::HashMap<String, String>>,
}

#[derive(serde::Deserialize)]
struct ProfileGit {
    name: Option<String>,
    email: Option<String>,
    default_branch: Option<String>,
    pull_rebase: Option<bool>,
    push_auto_setup_remote: Option<bool>,
}

#[derive(serde::Deserialize)]
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
}

#[derive(serde::Deserialize)]
struct ProfileFonts {
    nerd: Option<Vec<String>>,
    families: Option<Vec<String>>,
}

#[derive(serde::Deserialize)]
struct ProfileSecurity {
    touchid_sudo: Option<bool>,
}

pub fn run(config: &Config, repo_ref: &str, dry_run: bool) -> Result<()> {
    let profile = fetch_profile(repo_ref)?;

    // Handle extends — apply the base profile first
    if let Some(base_ref) = profile.meta.as_ref().and_then(|m| m.extends.as_deref()) {
        println!("  {} extends {}", style("i").cyan(), style(base_ref).bold());
        run(config, base_ref, dry_run)?;
        println!();
        println!(
            "  {} applying overlay {}",
            style("nex profile").bold(),
            style(
                profile
                    .meta
                    .as_ref()
                    .and_then(|m| m.name.as_deref())
                    .unwrap_or(repo_ref)
            )
            .cyan()
        );
        println!();
    } else if let Some(meta) = &profile.meta {
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

    let mut session = EditSession::new();
    let mut changes = 0;

    // Apply packages
    if let Some(pkgs) = &profile.packages {
        changes += apply_nix_packages(config, &mut session, pkgs, dry_run)?;
        changes += apply_brew_packages(config, &mut session, pkgs, dry_run)?;
        apply_taps(config, pkgs, dry_run)?;
    }

    // Apply kitty config and files
    if profile.kitty.is_some() {
        apply_kitty(config, repo_ref, &profile.kitty, dry_run)?;
    }

    // Apply shell config
    if let Some(shell) = &profile.shell {
        apply_shell(config, shell, dry_run)?;
    }

    // Apply git config
    if let Some(git) = &profile.git {
        apply_git(config, git, dry_run)?;
    }

    // Apply macOS preferences
    if let Some(macos) = &profile.macos {
        apply_macos(config, macos, dry_run)?;
    }

    // Apply security
    if let Some(security) = &profile.security {
        apply_security(config, security, dry_run)?;
    }

    if dry_run {
        println!();
        output::dry_run(&format!("{changes} package(s) would be added"));
        return Ok(());
    }

    if changes > 0 {
        session.commit_all()?;

        // Commit changes to the nix-darwin repo
        let _ = Command::new("git")
            .args(["add", "-A"])
            .current_dir(&config.repo)
            .output();
        let _ = Command::new("git")
            .args(["commit", "-m", &format!("nex profile apply: {repo_ref}")])
            .current_dir(&config.repo)
            .output();
    }

    // Save the profile reference for future updates
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

/// Fetch profile.toml from a GitHub repo.
/// Tries `gh api` first (handles private repos), falls back to raw.githubusercontent.
fn fetch_profile(repo_ref: &str) -> Result<Profile> {
    let repo = if repo_ref.starts_with("http") {
        repo_ref.to_string()
    } else {
        repo_ref
            .trim_start_matches("github.com/")
            .trim_start_matches("https://github.com/")
            .to_string()
    };

    output::status(&format!("fetching profile from {repo}..."));

    // Try gh CLI first (handles auth for private repos)
    let content = fetch_via_gh(&repo)
        .or_else(|_| fetch_via_curl(&repo))
        .with_context(|| format!("could not fetch profile.toml from {repo}"))?;

    let profile: Profile =
        toml::from_str(&content).with_context(|| format!("invalid profile.toml from {repo}"))?;

    Ok(profile)
}

fn fetch_via_gh(repo: &str) -> Result<String> {
    let output = Command::new("gh")
        .args([
            "api",
            &format!("repos/{repo}/contents/profile.toml"),
            "-H",
            "Accept: application/vnd.github.raw+json",
        ])
        .output()
        .context("gh not available")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let hint = if stderr.contains("404") {
            format!("repo {repo} not found (check the name, or run `gh auth refresh -s repo`)")
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

fn fetch_via_curl(repo: &str) -> Result<String> {
    let url = format!("https://raw.githubusercontent.com/{repo}/main/profile.toml");
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

/// Apply shell configuration via git commands (non-destructive).
fn apply_shell(config: &Config, shell: &ProfileShell, dry_run: bool) -> Result<()> {
    if dry_run {
        if shell.default.is_some() || shell.aliases.is_some() || shell.env.is_some() {
            output::dry_run("would apply shell configuration");
        }
        return Ok(());
    }

    // Shell config is baked into the scaffold's shell.nix — the profile.toml
    // is the portable record. The actual nix module is generated by nex init.
    // For now, we just note that shell prefs are in the profile for reference.
    if shell.aliases.is_some() || shell.env.is_some() {
        println!(
            "  {} shell aliases and env vars are in the profile",
            style("i").cyan()
        );
        println!(
            "    edit {} to customize",
            style(config.repo.join("nix/modules/home/shell.nix").display()).dim()
        );
    }

    Ok(())
}

/// Apply git config via git commands.
fn apply_git(_config: &Config, git: &ProfileGit, dry_run: bool) -> Result<()> {
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

    // Set up GitHub credential helper
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
fn apply_macos(_config: &Config, macos: &ProfileMacos, dry_run: bool) -> Result<()> {
    if dry_run {
        output::dry_run("would apply macOS preferences");
        return Ok(());
    }

    // These are applied via the nix-darwin config (system.defaults).
    // The profile.toml is the portable record — actual settings are in
    // darwin/base.nix which the scaffold generates.
    // For immediate effect without switch, apply via defaults(1):
    let defaults = [
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
    ];

    for (domain, key, value) in &defaults {
        if let Some(v) = value {
            let val_str = if *v { "true" } else { "false" };
            let _ = Command::new("defaults")
                .args(["write", domain, key, "-bool", val_str])
                .output();
        }
    }

    if let Some(false) = macos.natural_scroll {
        let _ = Command::new("defaults")
            .args([
                "write",
                "NSGlobalDomain",
                "com.apple.swipescrolldirection",
                "-bool",
                "false",
            ])
            .output();
    }

    if macos.dock_autohide == Some(true) {
        let _ = Command::new("defaults")
            .args(["write", "com.apple.dock", "autohide", "-bool", "true"])
            .output();
    }
    if macos.dock_show_recents == Some(false) {
        let _ = Command::new("defaults")
            .args(["write", "com.apple.dock", "show-recents", "-bool", "false"])
            .output();
    }

    println!("  {} macOS preferences applied", style("✓").green());

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
