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
    let upgraded = ensure_nix_homebrew_integration(config)?;
    let path = &config.homebrew_file;
    let content =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let patched = add_auto_migrate_to_nix_homebrew_module(&content)
        .context("could not find nix-homebrew block to patch autoMigrate")?;
    if patched == content {
        if upgraded {
            crate::exec::git_commit(&config.repo, "nex doctor: integrate nix-homebrew migration");
        }
        return Ok(upgraded);
    }
    crate::edit::atomic_write_bytes(path, patched.as_bytes())?;
    crate::exec::git_commit(&config.repo, "nex doctor: enable nix-homebrew autoMigrate");
    Ok(true)
}

pub(crate) fn nix_homebrew_auto_migrate_supported(config: &Config) -> bool {
    nix_homebrew_integration_present(config)
        || can_upgrade_nix_homebrew_integration(config)
        || (!config.repo.join("flake.nix").exists()
            && config
                .homebrew_file
                .exists()
                .then(|| std::fs::read_to_string(&config.homebrew_file).ok())
                .flatten()
                .is_some_and(|content| content.contains("nix-homebrew = {")))
}

fn nix_homebrew_integration_present(config: &Config) -> bool {
    let flake = std::fs::read_to_string(config.repo.join("flake.nix")).unwrap_or_default();
    let mk_host =
        std::fs::read_to_string(config.repo.join("nix/lib/mkHost.nix")).unwrap_or_default();
    flake.contains("nix-homebrew.url")
        && mk_host.contains("nix-homebrew.darwinModules.nix-homebrew")
        && config
            .homebrew_file
            .exists()
            .then(|| std::fs::read_to_string(&config.homebrew_file).ok())
            .flatten()
            .is_some_and(|content| content.contains("nix-homebrew = {"))
}

fn can_upgrade_nix_homebrew_integration(config: &Config) -> bool {
    if config.platform != Platform::Darwin {
        return false;
    }
    let flake = std::fs::read_to_string(config.repo.join("flake.nix")).ok();
    let mk_host = std::fs::read_to_string(config.repo.join("nix/lib/mkHost.nix")).ok();
    let homebrew = std::fs::read_to_string(&config.homebrew_file).ok();
    match (flake, mk_host, homebrew) {
        (Some(flake), Some(mk_host), Some(homebrew)) => {
            upgrade_legacy_flake(&flake).is_some()
                && upgrade_legacy_mk_host(&mk_host).is_some()
                && upgrade_legacy_homebrew_module(&homebrew).is_some()
        }
        _ => false,
    }
}

fn ensure_nix_homebrew_integration(config: &Config) -> Result<bool> {
    if nix_homebrew_integration_present(config) {
        return Ok(false);
    }
    if !can_upgrade_nix_homebrew_integration(config) {
        bail!("cannot upgrade this repository to nix-homebrew automatically");
    }

    let flake_path = config.repo.join("flake.nix");
    let mk_host_path = config.repo.join("nix/lib/mkHost.nix");
    let original_flake = std::fs::read_to_string(&flake_path)
        .with_context(|| format!("reading {}", flake_path.display()))?;
    let original_mk_host = std::fs::read_to_string(&mk_host_path)
        .with_context(|| format!("reading {}", mk_host_path.display()))?;
    let original_homebrew = std::fs::read_to_string(&config.homebrew_file)
        .with_context(|| format!("reading {}", config.homebrew_file.display()))?;

    let flake = upgrade_legacy_flake(&original_flake)
        .context("legacy flake shape is not safe to upgrade automatically")?;
    let mk_host = upgrade_legacy_mk_host(&original_mk_host)
        .context("legacy mkHost.nix shape is not safe to upgrade automatically")?;
    let homebrew = upgrade_legacy_homebrew_module(&original_homebrew)
        .context("legacy homebrew.nix shape is not safe to upgrade automatically")?;

    let lock_path = config.repo.join("flake.lock");
    let original_lock = std::fs::read(&lock_path).ok();
    let lock_existed = lock_path.exists();

    crate::edit::atomic_write_bytes(&flake_path, flake.as_bytes())?;
    if let Err(error) = (|| -> Result<()> {
        crate::edit::atomic_write_bytes(&mk_host_path, mk_host.as_bytes())?;
        crate::edit::atomic_write_bytes(&config.homebrew_file, homebrew.as_bytes())?;
        update_flake_lock_for_nix_homebrew(&config.repo)
    })() {
        let _ = crate::edit::atomic_write_bytes(&flake_path, original_flake.as_bytes());
        let _ = crate::edit::atomic_write_bytes(&mk_host_path, original_mk_host.as_bytes());
        let _ =
            crate::edit::atomic_write_bytes(&config.homebrew_file, original_homebrew.as_bytes());
        match (lock_existed, original_lock) {
            (true, Some(bytes)) => {
                let _ = crate::edit::atomic_write_bytes(&lock_path, &bytes);
            }
            (false, _) => {
                let _ = std::fs::remove_file(&lock_path);
            }
            _ => {}
        }
        return Err(error).context("rolled back nix-homebrew repository upgrade");
    }
    Ok(true)
}

fn update_flake_lock_for_nix_homebrew(repo: &Path) -> Result<()> {
    let output = crate::exec::nix_command()
        .args(["flake", "lock", "--update-input", "nix-homebrew"])
        .current_dir(repo)
        .output()
        .context("updating flake.lock for nix-homebrew")?;
    if !output.status.success() {
        bail!(
            "failed to add nix-homebrew to flake.lock: {}",
            crate::exec::captured_text(&output.stderr).trim()
        );
    }
    Ok(())
}

fn upgrade_legacy_flake(content: &str) -> Option<String> {
    if content.contains("nix-homebrew.url") {
        return Some(content.to_string());
    }
    let input_anchor = "    mac-app-util.url = \"github:hraban/mac-app-util\";";
    let output_anchor = "mac-app-util }";
    let inherit_anchor = "inherit nixpkgs nix-darwin home-manager mac-app-util;";
    if !content.contains(input_anchor)
        || !content.contains(output_anchor)
        || !content.contains(inherit_anchor)
    {
        return None;
    }
    Some(
        content
            .replace(
                input_anchor,
                &format!(
                    "{input_anchor}\n    nix-homebrew.url = \"github:zhaofengli/nix-homebrew\";"
                ),
            )
            .replace(output_anchor, "mac-app-util, nix-homebrew }")
            .replace(
                inherit_anchor,
                "inherit nixpkgs nix-darwin home-manager mac-app-util nix-homebrew;",
            ),
    )
}

fn upgrade_legacy_mk_host(content: &str) -> Option<String> {
    if content.contains("nix-homebrew.darwinModules.nix-homebrew") {
        return Some(content.to_string());
    }
    let args_anchor = "{ nixpkgs, nix-darwin, home-manager, mac-app-util }:";
    let module_anchor = "    mac-app-util.darwinModules.default";
    if !content.contains(args_anchor) || !content.contains(module_anchor) {
        return None;
    }
    Some(
        content
            .replace(
                args_anchor,
                "{ nixpkgs, nix-darwin, home-manager, mac-app-util, nix-homebrew }:",
            )
            .replace(
                module_anchor,
                "    mac-app-util.darwinModules.default\n    nix-homebrew.darwinModules.nix-homebrew",
            ),
    )
}

fn upgrade_legacy_homebrew_module(content: &str) -> Option<String> {
    if content.contains("nix-homebrew = {") {
        return add_auto_migrate_to_nix_homebrew_module(content);
    }
    let args_end = content.find(':')?;
    if content.get(..args_end)?.trim() != "{ ... }" {
        return None;
    }
    let body = content.get(args_end + 1..)?.trim_start();
    if !body.starts_with("{") || !body.contains("homebrew = {") {
        return None;
    }
    let mut upgraded = String::from("{ pkgs, username, ... }:\n\n{\n  nix-homebrew = {\n    enable = true;\n    enableRosetta = pkgs.stdenv.hostPlatform.isAarch64;\n    user = username;\n    autoMigrate = true;\n  };\n\n");
    upgraded.push_str(body.strip_prefix("{\n").unwrap_or(body));
    Some(upgraded)
}

fn add_auto_migrate_to_nix_homebrew_module(content: &str) -> Option<String> {
    if nix_homebrew_auto_migrate_configured_in_content(content) {
        return Some(content.to_string());
    }

    let start = content.find("nix-homebrew = {")?;
    let block = nix_homebrew_block(content)?;
    let relative_enable = block.find("    enable = true;\n")?;
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
        nix_homebrew_auto_migrate_configured_in_content, upgrade_legacy_flake,
        upgrade_legacy_homebrew_module, upgrade_legacy_mk_host, ExistingHomebrew,
    };
    use std::path::PathBuf;

    #[test]
    fn upgrades_legacy_darwin_repository_fragments() {
        let flake = r#"{
  inputs = {
    mac-app-util.url = "github:hraban/mac-app-util";
  };
  outputs = { self, nixpkgs, nix-darwin, home-manager, mac-app-util }:
    let mkHost = import ./nix/lib/mkHost.nix { inherit nixpkgs nix-darwin home-manager mac-app-util; };
}"#;
        let upgraded_flake = upgrade_legacy_flake(flake).expect("upgrade flake");
        assert!(upgraded_flake.contains("nix-homebrew.url"));
        assert!(upgraded_flake.contains("mac-app-util, nix-homebrew }"));
        assert!(upgraded_flake.contains("mac-app-util nix-homebrew;"));

        let mk_host = r#"{ nixpkgs, nix-darwin, home-manager, mac-app-util }:
nix-darwin.lib.darwinSystem {
  modules = [
    mac-app-util.darwinModules.default
  ];
}"#;
        let upgraded_host = upgrade_legacy_mk_host(mk_host).expect("upgrade mkHost");
        assert!(upgraded_host.contains("mac-app-util, nix-homebrew }:"));
        assert!(upgraded_host.contains("nix-homebrew.darwinModules.nix-homebrew"));

        let homebrew = "{ ... }:\n\n{\n  homebrew = {\n    enable = true;\n  };\n}\n";
        let upgraded_homebrew = upgrade_legacy_homebrew_module(homebrew).expect("upgrade homebrew");
        assert!(upgraded_homebrew.contains("{ pkgs, username, ... }:"));
        assert!(upgraded_homebrew.contains("enableRosetta = pkgs.stdenv.hostPlatform.isAarch64;"));
        assert!(upgraded_homebrew.contains("nix-homebrew = {"));
        assert!(upgraded_homebrew.contains("autoMigrate = true;"));
        assert!(upgraded_homebrew.contains("homebrew = {"));
    }

    #[test]
    fn rejects_legacy_homebrew_with_named_arguments() {
        let input = "{ lib, ... }:\n{ homebrew.enable = lib.mkDefault true; }\n";
        assert!(upgrade_legacy_homebrew_module(input).is_none());
    }

    #[test]
    fn legacy_upgrade_is_idempotent() {
        let flake = "    nix-homebrew.url = \"github:zhaofengli/nix-homebrew\";\n";
        assert_eq!(upgrade_legacy_flake(flake).as_deref(), Some(flake));
        let mk_host = "    nix-homebrew.darwinModules.nix-homebrew\n";
        assert_eq!(upgrade_legacy_mk_host(mk_host).as_deref(), Some(mk_host));
        let homebrew = "nix-homebrew = {\n    enable = true;\n    autoMigrate = true;\n};\n";
        assert_eq!(
            upgrade_legacy_homebrew_module(homebrew).as_deref(),
            Some(homebrew)
        );
    }

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
    fn auto_migrate_patch_never_crosses_nix_homebrew_block() {
        let input =
            "nix-homebrew = {\n    enable = false;\n};\n\nhomebrew = {\n    enable = true;\n};\n";
        assert!(add_auto_migrate_to_nix_homebrew_module(input).is_none());
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
