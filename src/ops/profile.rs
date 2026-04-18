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
    macos: Option<ProfileMacos>,
    security: Option<ProfileSecurity>,
}

#[derive(serde::Deserialize)]
struct ProfileMeta {
    name: Option<String>,
    description: Option<String>,
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

    if let Some(meta) = &profile.meta {
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
fn fetch_profile(repo_ref: &str) -> Result<Profile> {
    // Accept: user/repo, github.com/user/repo, or full URL
    let raw_url = if repo_ref.starts_with("http") {
        // Full URL — assume it points to profile.toml
        repo_ref.to_string()
    } else {
        // user/repo shorthand — fetch from GitHub raw
        let repo = repo_ref
            .trim_start_matches("github.com/")
            .trim_start_matches("https://github.com/");
        format!("https://raw.githubusercontent.com/{repo}/main/profile.toml")
    };

    output::status(&format!("fetching profile from {repo_ref}..."));

    let output = Command::new("curl")
        .args(["-fsSL", &raw_url])
        .output()
        .context("failed to fetch profile")?;

    if !output.status.success() {
        bail!(
            "could not fetch profile from {repo_ref}\n\
             tried: {raw_url}"
        );
    }

    let content = String::from_utf8_lossy(&output.stdout);
    let profile: Profile = toml::from_str(&content)
        .with_context(|| format!("invalid profile.toml from {repo_ref}"))?;

    Ok(profile)
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
