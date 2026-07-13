use std::path::{Path, PathBuf};

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
    pub managed_by_nix_homebrew: bool,
}

impl ExistingHomebrew {
    pub fn is_conflict(&self) -> bool {
        (self.repository.exists() || self.brew_binary.is_some())
            && !self.auto_migrate_configured
            && !self.managed_by_nix_homebrew
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HomebrewBootstrapChoice {
    Migrate,
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

    let managed_by_nix_homebrew =
        managed_by_nix_homebrew(prefix, &repository, brew_binary.as_deref());

    Ok(Some(ExistingHomebrew {
        prefix: prefix.to_path_buf(),
        repository,
        brew_binary,
        auto_migrate_configured: nix_homebrew_auto_migrate_configured(&config.homebrew_file)?,
        managed_by_nix_homebrew,
    }))
}

fn managed_by_nix_homebrew(prefix: &Path, repository: &Path, brew_binary: Option<&Path>) -> bool {
    let marker = repository.join(".homebrew-is-managed-by-nix");
    if marker.exists() {
        return true;
    }

    if path_resolves_into_nix_store(repository) {
        return true;
    }

    if brew_binary.is_some_and(path_resolves_into_nix_store) {
        return true;
    }

    let taps = prefix.join("Homebrew/Library/Taps");
    if path_resolves_into_nix_store(&taps) {
        return true;
    }

    false
}

fn path_resolves_into_nix_store(path: &Path) -> bool {
    path.starts_with("/nix/store")
        || path
            .canonicalize()
            .map(|resolved| resolved.starts_with("/nix/store"))
            .unwrap_or(false)
}

pub(crate) fn preflight(config: &Config, dry_run: bool) -> Result<()> {
    if std::env::var_os("NEX_SKIP_HOMEBREW_PREFLIGHT").is_some() {
        return Ok(());
    }

    let Some(existing) = detect_existing(config)? else {
        return Ok(());
    };
    if !existing.is_conflict() {
        return Ok(());
    }

    let supports_auto_migrate = nix_homebrew_auto_migrate_supported(config);
    print_existing_homebrew_warning(&existing, supports_auto_migrate);
    if dry_run {
        return Ok(());
    }

    match prompt_choice(supports_auto_migrate)? {
        HomebrewBootstrapChoice::Migrate => {
            enable_auto_migrate(config)?;
            eprintln!(
                "  {} enabled nix-homebrew.autoMigrate; rerun switch/activation",
                style("✓").green().bold()
            );
            bail!("Homebrew migration configured; rerun the activation command");
        }
        HomebrewBootstrapChoice::Abort => bail!(
            "cannot safely reset this Homebrew installation; enable nix-homebrew.autoMigrate or leave it unchanged"
        ),
    }
}

pub(crate) fn print_existing_homebrew_warning(
    existing: &ExistingHomebrew,
    supports_auto_migrate: bool,
) {
    eprintln!();
    eprintln!(
        "  {} existing unmanaged Homebrew detected at {}",
        style("!").yellow().bold(),
        existing.prefix.display()
    );
    let repair = if supports_auto_migrate {
        "nix-homebrew will reject activation until this installation is migrated."
    } else {
        "nix-homebrew will reject activation, and this configuration cannot safely migrate the existing installation."
    };
    eprintln!("    {}", style(repair).dim());
    if supports_auto_migrate {
        eprintln!(
            "    {}",
            style(
                "Run `nex doctor --fix homebrew-bootstrap` to enable migration before switching."
            )
            .dim()
        );
    } else {
        eprintln!(
            "    {}",
            style("Update nix-homebrew to expose autoMigrate; Nex will not remove or disable the existing installation.").dim()
        );
    }
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

    let supports_auto_migrate = nix_homebrew_auto_migrate_supported(config);
    print_existing_homebrew_warning(&existing, supports_auto_migrate);
    if fix {
        match prompt_choice(supports_auto_migrate)? {
            HomebrewBootstrapChoice::Migrate => {
                enable_auto_migrate(config)?;
            }
            HomebrewBootstrapChoice::Abort => bail!(
                "cannot safely reset this Homebrew installation; enable nix-homebrew.autoMigrate or leave it unchanged"
            ),
        }
    }
    Ok(())
}

fn prompt_choice(supports_auto_migrate: bool) -> Result<HomebrewBootstrapChoice> {
    let mut items = Vec::new();
    if supports_auto_migrate {
        items.push(
            "migrate: set nix-homebrew.autoMigrate = true and preserve installed packages"
                .to_string(),
        );
    }
    items.push("abort: leave Homebrew unchanged".to_string());

    let choice = crate::input::input().select("Existing Homebrew detected", &items, 0)?;
    Ok(choice_from_index(supports_auto_migrate, choice))
}

fn choice_from_index(supports_auto_migrate: bool, choice: usize) -> HomebrewBootstrapChoice {
    if supports_auto_migrate && choice == 0 {
        HomebrewBootstrapChoice::Migrate
    } else {
        HomebrewBootstrapChoice::Abort
    }
}

pub(crate) fn enable_auto_migrate(config: &Config) -> Result<bool> {
    let path = &config.homebrew_file;
    let content =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let patched = add_auto_migrate_to_nix_homebrew_module(&content)
        .context("could not find nix-homebrew block to patch autoMigrate")?;
    if patched == content {
        return Ok(false);
    }
    crate::edit::atomic_write_bytes(path, patched.as_bytes())?;
    crate::exec::git_commit(&config.repo, "nex doctor: enable nix-homebrew autoMigrate");
    Ok(true)
}

pub(crate) fn nix_homebrew_auto_migrate_supported(config: &Config) -> bool {
    if config.repo.join("flake.nix").exists() {
        return nix_homebrew_auto_migrate_supported_by_flake(&config.repo).unwrap_or(false);
    }
    config
        .homebrew_file
        .exists()
        .then(|| std::fs::read_to_string(&config.homebrew_file).ok())
        .flatten()
        .is_some_and(|content| content.contains("nix-homebrew = {"))
}

fn nix_homebrew_auto_migrate_supported_by_flake(repo: &Path) -> Result<bool> {
    if !repo.join("flake.nix").exists() {
        return Ok(false);
    }
    let expr = r#"let
  flake = builtins.getFlake (toString ./.);
  module = flake.inputs.nix-homebrew.darwinModules.nix-homebrew;
  eval = flake.inputs.nix-darwin.lib.darwinSystem {
    system = builtins.currentSystem;
    modules = [ module { nix-homebrew.enable = false; } ];
  };
in eval.options ? "nix-homebrew" && eval.options."nix-homebrew" ? autoMigrate"#;
    let output = crate::exec::nix_command()
        .args(["eval", "--impure", "--expr", expr, "--raw"])
        .current_dir(repo)
        .stderr(std::process::Stdio::null())
        .output()
        .context("failed to evaluate nix-homebrew.autoMigrate support")?;
    if !output.status.success() {
        return Ok(false);
    }
    Ok(crate::exec::captured_text(&output.stdout).trim() == "true")
}

fn add_auto_migrate_to_nix_homebrew_module(content: &str) -> Option<String> {
    if nix_homebrew_auto_migrate_configured_in_content(content) {
        return Some(content.to_string());
    }

    let start = content.find("nix-homebrew = {")?;
    let relative_enable = content[start..].find("    enable = true;\n")?;
    let idx = start + relative_enable + "    enable = true;\n".len();
    let mut patched = String::with_capacity(content.len() + 32);
    patched.push_str(&content[..idx]);
    patched.push_str("    autoMigrate = true;\n");
    patched.push_str(&content[idx..]);
    Some(patched)
}

fn nix_homebrew_auto_migrate_configured(path: &Path) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let content =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(nix_homebrew_auto_migrate_configured_in_content(&content))
}

fn nix_homebrew_auto_migrate_configured_in_content(content: &str) -> bool {
    let Some(block) = nix_homebrew_block(content) else {
        return false;
    };
    block.contains("autoMigrate = true;")
}

fn nix_homebrew_block(content: &str) -> Option<&str> {
    let start = content.find("nix-homebrew = {")?;
    let bytes = content.as_bytes();
    let mut depth = 0usize;
    let mut entered = false;
    for (idx, byte) in bytes.iter().enumerate().skip(start) {
        match *byte {
            b'{' => {
                depth += 1;
                entered = true;
            }
            b'}' if entered => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return content.get(start..=idx);
                }
            }
            _ => {}
        }
    }
    content.get(start..)
}

pub(crate) fn expected_brew_binary_exists() -> bool {
    expected_homebrew_prefix_for_host()
        .join("bin/brew")
        .exists()
}

fn expected_homebrew_prefix_for_host() -> PathBuf {
    if cfg!(target_arch = "aarch64") {
        PathBuf::from("/opt/homebrew")
    } else {
        PathBuf::from("/usr/local")
    }
}

fn homebrew_prefixes_for_host() -> Vec<PathBuf> {
    if cfg!(target_arch = "aarch64") {
        vec![PathBuf::from("/opt/homebrew"), PathBuf::from("/usr/local")]
    } else {
        vec![PathBuf::from("/usr/local"), PathBuf::from("/opt/homebrew")]
    }
}

#[cfg(test)]
mod tests {
    use super::{
        add_auto_migrate_to_nix_homebrew_module, managed_by_nix_homebrew,
        nix_homebrew_auto_migrate_configured_in_content, ExistingHomebrew,
    };
    use std::path::PathBuf;

    #[test]
    fn inserts_auto_migrate_only_in_nix_homebrew_block() {
        let input = "homebrew = {\n    enable = true;\n};\n\nnix-homebrew = {\n    enable = true;\n    user = username;\n};\n";
        let output = add_auto_migrate_to_nix_homebrew_module(input).expect("patchable");

        assert!(output.contains("nix-homebrew = {\n    enable = true;\n    autoMigrate = true;\n"));
        let homebrew_block = output
            .split("nix-homebrew = {")
            .next()
            .expect("homebrew block");
        assert!(homebrew_block.contains("homebrew = {\n    enable = true;\n};"));
        assert!(!homebrew_block.contains("autoMigrate = true;"));
    }

    #[test]
    fn auto_migrate_patch_is_idempotent() {
        let input = "nix-homebrew = {\n    enable = true;\n    autoMigrate = true;\n};\n";
        assert_eq!(
            add_auto_migrate_to_nix_homebrew_module(input).as_deref(),
            Some(input)
        );
    }

    #[test]
    fn auto_migrate_detection_is_scoped_to_nix_homebrew() {
        let wrong_block = "homebrew = {\n    enable = true;\n    autoMigrate = true;\n};\n\nnix-homebrew = {\n    enable = true;\n};\n";
        assert!(!nix_homebrew_auto_migrate_configured_in_content(
            wrong_block
        ));

        let right_block = "homebrew = {\n    enable = true;\n};\n\nnix-homebrew = {\n    enable = true;\n    autoMigrate = true;\n};\n";
        assert!(nix_homebrew_auto_migrate_configured_in_content(right_block));
    }

    #[test]
    fn auto_migrate_support_eval_returns_false_without_flake() {
        let dir = tempfile::tempdir().expect("temp dir");

        assert!(
            !super::nix_homebrew_auto_migrate_supported_by_flake(dir.path())
                .expect("support check")
        );
    }

    #[test]
    fn auto_migrate_detection_stops_at_nested_nix_homebrew_block_end() {
        let content = r#"nix-homebrew = {
    enable = true;
    taps = {
      "homebrew/homebrew-core" = homebrew-core;
    };
};

homebrew = {
    enable = true;
    autoMigrate = true;
};
"#;

        assert!(!nix_homebrew_auto_migrate_configured_in_content(content));
    }

    #[test]
    fn marker_file_classifies_prefix_as_managed() {
        let dir = tempfile::tempdir().expect("temp dir");
        let repo = dir.path().join("Homebrew/Library/Homebrew");
        std::fs::create_dir_all(&repo).expect("repo dir");
        std::fs::write(repo.join(".homebrew-is-managed-by-nix"), "").expect("marker");

        assert!(managed_by_nix_homebrew(dir.path(), &repo, None));
    }

    #[test]
    fn nix_store_brew_symlink_classifies_prefix_as_managed() {
        let dir = tempfile::tempdir().expect("temp dir");
        let repo = dir.path().join("Homebrew/Library/Homebrew");
        std::fs::create_dir_all(&repo).expect("repo dir");
        let brew = PathBuf::from("/nix/store/nex-test-brew");

        assert!(managed_by_nix_homebrew(dir.path(), &repo, Some(&brew)));
    }

    #[test]
    fn managed_homebrew_is_not_a_conflict() {
        let existing = ExistingHomebrew {
            prefix: PathBuf::from("/usr/local"),
            repository: PathBuf::from("/usr/local/Homebrew/Library/Homebrew"),
            brew_binary: Some(PathBuf::from("/usr/local/bin/brew")),
            auto_migrate_configured: false,
            managed_by_nix_homebrew: true,
        };

        assert!(!existing.is_conflict());
    }

    #[test]
    fn unmanaged_homebrew_is_a_conflict() {
        let dir = tempfile::tempdir().expect("temp dir");
        let repo = dir.path().join("Homebrew/Library/Homebrew");
        std::fs::create_dir_all(&repo).expect("repo dir");
        let existing = ExistingHomebrew {
            prefix: dir.path().to_path_buf(),
            repository: repo,
            brew_binary: None,
            auto_migrate_configured: false,
            managed_by_nix_homebrew: false,
        };

        assert!(existing.is_conflict());
    }

    #[test]
    fn unsupported_migration_can_only_abort() {
        assert_eq!(
            super::choice_from_index(false, 0),
            super::HomebrewBootstrapChoice::Abort
        );
        assert_eq!(
            super::choice_from_index(false, 1),
            super::HomebrewBootstrapChoice::Abort
        );
    }

    #[test]
    fn supported_migration_is_the_only_mutating_choice() {
        assert_eq!(
            super::choice_from_index(true, 0),
            super::HomebrewBootstrapChoice::Migrate
        );
        assert_eq!(
            super::choice_from_index(true, 1),
            super::HomebrewBootstrapChoice::Abort
        );
    }
}
