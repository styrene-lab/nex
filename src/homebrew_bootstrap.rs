use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use console::style;

use crate::config::Config;
use crate::discover::Platform;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingHomebrew {
    pub prefix: PathBuf,
    pub repository: PathBuf,
    pub brew_binary: Option<PathBuf>,
    pub auto_migrate_configured: bool,
}

impl ExistingHomebrew {
    pub fn is_conflict(&self) -> bool {
        (self.repository.exists() || self.brew_binary.is_some()) && !self.auto_migrate_configured
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HomebrewBootstrapChoice {
    Migrate,
    Reset,
    Abort,
}

pub(crate) fn detect_existing(config: &Config) -> Result<Option<ExistingHomebrew>> {
    if config.platform != Platform::Darwin {
        return Ok(None);
    }
    let prefixes = homebrew_prefixes_for_host();
    for prefix in prefixes {
        if let Some(existing) = detect_existing_at(config, &prefix)? {
            return Ok(Some(existing));
        }
    }
    Ok(None)
}

fn detect_existing_at(config: &Config, prefix: &Path) -> Result<Option<ExistingHomebrew>> {
    let repository = prefix.join("Homebrew/Library/Homebrew");
    let brew_binary = [prefix.join("bin/brew"), prefix.join("Homebrew/bin/brew")]
        .into_iter()
        .find(|path| path.exists());

    if !repository.exists() && brew_binary.is_none() {
        return Ok(None);
    }

    Ok(Some(ExistingHomebrew {
        prefix: prefix.to_path_buf(),
        repository,
        brew_binary,
        auto_migrate_configured: homebrew_auto_migrate_configured(&config.homebrew_file)?,
    }))
}

pub(crate) fn preflight(config: &Config, dry_run: bool) -> Result<()> {
    let Some(existing) = detect_existing(config)? else {
        return Ok(());
    };
    if !existing.is_conflict() {
        return Ok(());
    }

    print_existing_homebrew_warning(&existing);
    if dry_run {
        return Ok(());
    }

    match prompt_choice()? {
        HomebrewBootstrapChoice::Migrate => {
            enable_auto_migrate(config)?;
            eprintln!(
                "  {} enabled nix-homebrew.autoMigrate; rerun switch/activation",
                style("✓").green().bold()
            );
            bail!("Homebrew migration configured; rerun the activation command");
        }
        HomebrewBootstrapChoice::Reset => {
            let inventory = inventory_existing(&existing)?;
            quarantine_existing(&existing)?;
            eprintln!(
                "  {} inventory kept at {}",
                style("i").cyan(),
                inventory.display()
            );
        }
        HomebrewBootstrapChoice::Abort => bail!("aborted to leave existing Homebrew unchanged"),
    }

    Ok(())
}

pub(crate) fn print_existing_homebrew_warning(existing: &ExistingHomebrew) {
    eprintln!();
    eprintln!(
        "  {} existing unmanaged Homebrew detected at {}",
        style("!").yellow().bold(),
        existing.prefix.display()
    );
    eprintln!(
        "    {}",
        style("nix-homebrew will reject activation until this is migrated or reset.").dim()
    );
    eprintln!(
        "    {}",
        style("Run `nex doctor --fix homebrew-bootstrap` to repair before switching.").dim()
    );
}

pub(crate) fn doctor(config: &Config, fix: bool) -> Result<()> {
    let Some(existing) = detect_existing(config)? else {
        eprintln!("  {} homebrew bootstrap ready", style("✓").green().bold());
        return Ok(());
    };
    if !existing.is_conflict() {
        eprintln!("  {} homebrew bootstrap ready", style("✓").green().bold());
        return Ok(());
    }

    print_existing_homebrew_warning(&existing);
    if fix {
        match prompt_choice()? {
            HomebrewBootstrapChoice::Migrate => {
                enable_auto_migrate(config)?;
            }
            HomebrewBootstrapChoice::Reset => {
                let inventory = inventory_existing(&existing)?;
                quarantine_existing(&existing)?;
                eprintln!(
                    "  {} inventory kept at {}",
                    style("i").cyan(),
                    inventory.display()
                );
            }
            HomebrewBootstrapChoice::Abort => bail!("aborted to leave existing Homebrew unchanged"),
        }
    }
    Ok(())
}

fn prompt_choice() -> Result<HomebrewBootstrapChoice> {
    let items = vec![
        "migrate: set nix-homebrew.autoMigrate = true and preserve installed packages".to_string(),
        "reset: inventory packages, move Homebrew aside, and let nix-homebrew install fresh"
            .to_string(),
        "abort: leave Homebrew unchanged".to_string(),
    ];
    match crate::input::input().select("Existing Homebrew detected", &items, 0)? {
        0 => Ok(HomebrewBootstrapChoice::Migrate),
        1 => confirm_reset_choice(),
        _ => Ok(HomebrewBootstrapChoice::Abort),
    }
}

fn confirm_reset_choice() -> Result<HomebrewBootstrapChoice> {
    eprintln!();
    eprintln!(
        "  {} reset moves the Homebrew repository and brew shim aside; it does not delete package payloads.",
        style("!").yellow().bold()
    );
    eprintln!("  Type {} to continue.", style("RESET HOMEBREW").bold());
    let typed = crate::input::input().input_text("Confirmation", None)?;
    Ok(reset_choice_from_confirmation(&typed))
}

fn reset_choice_from_confirmation(typed: &str) -> HomebrewBootstrapChoice {
    if typed == "RESET HOMEBREW" {
        HomebrewBootstrapChoice::Reset
    } else {
        HomebrewBootstrapChoice::Abort
    }
}

pub(crate) fn enable_auto_migrate(config: &Config) -> Result<bool> {
    let path = &config.homebrew_file;
    let content =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let patched = add_auto_migrate_to_homebrew_module(&content)
        .context("could not find nix-homebrew block to patch autoMigrate")?;
    if patched == content {
        return Ok(false);
    }
    crate::edit::atomic_write_bytes(path, patched.as_bytes())?;
    crate::exec::git_commit(&config.repo, "nex doctor: enable nix-homebrew autoMigrate");
    Ok(true)
}

fn add_auto_migrate_to_homebrew_module(content: &str) -> Option<String> {
    if content.contains("autoMigrate = true;") {
        return Some(content.to_string());
    }
    let marker = "    enable = true;\n";
    let idx = content.find(marker)? + marker.len();
    let mut patched = String::with_capacity(content.len() + 32);
    patched.push_str(&content[..idx]);
    patched.push_str("    autoMigrate = true;\n");
    patched.push_str(&content[idx..]);
    Some(patched)
}

fn inventory_existing(existing: &ExistingHomebrew) -> Result<PathBuf> {
    let state_dir = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".local/state/nex");
    std::fs::create_dir_all(&state_dir)?;
    let stamp = current_timestamp();
    let prefix = state_dir.join(format!("homebrew-before-reset-{stamp}"));
    std::fs::create_dir_all(&prefix)?;

    if let Some(brew) = &existing.brew_binary {
        capture_brew_list(brew, &["list", "--formula"], &prefix.join("formulae.txt"))?;
        capture_brew_list(brew, &["list", "--cask"], &prefix.join("casks.txt"))?;
        capture_brew_list(brew, &["leaves"], &prefix.join("leaves.txt"))?;
        capture_brew_list(brew, &["tap"], &prefix.join("taps.txt"))?;
        capture_brew_list(brew, &["config"], &prefix.join("config.txt"))?;
        let brewfile = prefix.join("Brewfile");
        let _ = Command::new(brew)
            .args(["bundle", "dump", "--file"])
            .arg(&brewfile)
            .arg("--force")
            .output();
    }

    eprintln!(
        "  {} wrote Homebrew inventory to {}",
        style("✓").green(),
        prefix.display()
    );
    Ok(prefix)
}

fn capture_brew_list(brew: &Path, args: &[&str], output_path: &Path) -> Result<()> {
    let output = Command::new(brew)
        .args(args)
        .output()
        .with_context(|| format!("running {} {}", brew.display(), args.join(" ")))?;
    if !output.status.success() {
        bail!(
            "failed to inventory Homebrew with `{} {}`: {}",
            brew.display(),
            args.join(" "),
            crate::exec::captured_text(&output.stderr).trim()
        );
    }
    std::fs::write(output_path, crate::exec::captured_text(&output.stdout))?;
    Ok(())
}

fn quarantine_existing(existing: &ExistingHomebrew) -> Result<()> {
    let prefix = existing.prefix.to_string_lossy().to_string();
    if prefix != "/usr/local" && prefix != "/opt/homebrew" {
        bail!("refusing to reset unexpected Homebrew prefix {prefix}");
    }

    if existing.repository.exists() {
        let homebrew_dir = existing.prefix.join("Homebrew");
        ensure_safe_quarantine_source(&homebrew_dir, &existing.prefix)?;
        let quarantine = next_quarantine_path(&homebrew_dir);
        run_sudo_mv(&homebrew_dir, &quarantine)?;
        eprintln!(
            "  {} moved {} to {}",
            style("✓").green(),
            homebrew_dir.display(),
            quarantine.display()
        );
    }

    if let Some(brew) = &existing.brew_binary {
        if brew.exists() && brew.starts_with(&existing.prefix) {
            ensure_safe_quarantine_source(brew, &existing.prefix)?;
            let quarantine = next_quarantine_path(brew);
            run_sudo_mv(brew, &quarantine)?;
            eprintln!(
                "  {} moved {} to {}",
                style("✓").green(),
                brew.display(),
                quarantine.display()
            );
        }
    }

    eprintln!(
        "  {} moved unmanaged Homebrew aside at {}",
        style("✓").green(),
        existing.prefix.display()
    );
    Ok(())
}

fn ensure_safe_quarantine_source(path: &Path, prefix: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)
        .with_context(|| format!("inspecting {}", path.display()))?;
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    let canonical_path = path
        .canonicalize()
        .with_context(|| format!("canonicalizing {}", path.display()))?;
    let canonical_prefix = prefix
        .canonicalize()
        .with_context(|| format!("canonicalizing {}", prefix.display()))?;
    if !canonical_path.starts_with(&canonical_prefix) {
        bail!(
            "refusing to move {} because it resolves outside {}",
            path.display(),
            prefix.display()
        );
    }
    Ok(())
}

fn run_sudo_mv(from: &Path, to: &Path) -> Result<()> {
    let status = Command::new("sudo")
        .arg("mv")
        .arg(from)
        .arg(to)
        .status()
        .with_context(|| format!("failed to move {} to {}", from.display(), to.display()))?;
    if !status.success() {
        bail!("failed to move {} to {}", from.display(), to.display());
    }
    Ok(())
}

fn next_quarantine_path(path: &Path) -> PathBuf {
    let candidate = PathBuf::from(format!(
        "{}.before-nex-reset-{}",
        path.display(),
        current_timestamp()
    ));
    if !candidate.exists() {
        return candidate;
    }
    for i in 1.. {
        let candidate = PathBuf::from(format!(
            "{}.before-nex-reset-{}.{}",
            path.display(),
            current_timestamp(),
            i
        ));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

fn homebrew_auto_migrate_configured(path: &Path) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let content =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(content.contains("autoMigrate = true;"))
}

fn homebrew_prefixes_for_host() -> Vec<PathBuf> {
    if cfg!(target_arch = "aarch64") {
        vec![PathBuf::from("/opt/homebrew"), PathBuf::from("/usr/local")]
    } else {
        vec![PathBuf::from("/usr/local"), PathBuf::from("/opt/homebrew")]
    }
}

fn current_timestamp() -> String {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => duration.as_secs().to_string(),
        Err(_) => "0".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{add_auto_migrate_to_homebrew_module, next_quarantine_path};
    use std::path::Path;

    #[test]
    fn inserts_auto_migrate_after_enable() {
        let input = "nix-homebrew = {\n    enable = true;\n    user = username;\n};\n";
        let output = add_auto_migrate_to_homebrew_module(input).expect("patchable");
        assert!(output.contains("    enable = true;\n    autoMigrate = true;\n"));
    }

    #[test]
    fn auto_migrate_patch_is_idempotent() {
        let input = "nix-homebrew = {\n    enable = true;\n    autoMigrate = true;\n};\n";
        assert_eq!(
            add_auto_migrate_to_homebrew_module(input).as_deref(),
            Some(input)
        );
    }

    #[test]
    fn quarantine_path_moves_aside_instead_of_deleting() {
        let path = next_quarantine_path(Path::new("/usr/local/Homebrew"));
        assert!(path
            .to_string_lossy()
            .starts_with("/usr/local/Homebrew.before-nex-reset-"));
    }

    #[test]
    fn reset_confirmation_accepts_only_exact_phrase() {
        assert_eq!(
            super::reset_choice_from_confirmation("no"),
            super::HomebrewBootstrapChoice::Abort
        );
        assert_eq!(
            super::reset_choice_from_confirmation("RESET HOMEBREW"),
            super::HomebrewBootstrapChoice::Reset
        );
    }
}
