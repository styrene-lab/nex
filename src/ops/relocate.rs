use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use console::style;

use crate::config;
use crate::discover;
use crate::output;

/// Move a system-owned nix config (typically /etc/nixos) into a user-writable
/// directory and update ~/.config/nex/config.toml to point at the new path.
///
/// Why: NixOS installers drop the config in /etc/nixos, which is root-owned.
/// nex then needs sudo for every read and edit. After relocation, only the
/// final `nixos-rebuild switch` needs root.
pub fn run(target_arg: Option<&Path>, dry_run: bool) -> Result<()> {
    let source = resolve_source()?;
    let target = resolve_target(target_arg)?;

    println!();
    println!(
        "  {} — move config out of root-owned location",
        style("nex relocate").bold()
    );
    println!();
    println!("  source: {}", style(source.display()).cyan());
    println!("  target: {}", style(target.display()).cyan());
    println!();

    let needs_sudo = !is_writable_by_user(&source);

    // If the source is already user-writable AND the user didn't ask for a
    // specific target, there's nothing to fix. Avoid silently moving an
    // already-fine config to ~/nix-config.
    if !needs_sudo && target_arg.is_none() {
        println!(
            "  {} {} is already user-writable — nothing to do.",
            style("✓").green().bold(),
            source.display()
        );
        println!();
        println!("  Pass --to <path> to force a move to a specific location.");
        println!();
        return Ok(());
    }

    validate_move(&source, &target)?;
    let user = std::env::var("USER").unwrap_or_else(|_| "user".to_string());

    if dry_run {
        if needs_sudo {
            output::dry_run(&format!(
                "would run: sudo mv {} {}",
                source.display(),
                target.display()
            ));
            output::dry_run(&format!(
                "would run: sudo chown -R {user}:users {}",
                target.display()
            ));
        } else {
            output::dry_run(&format!(
                "would run: mv {} {}",
                source.display(),
                target.display()
            ));
        }
        output::dry_run(&format!(
            "would update ~/.config/nex/config.toml: repo_path = {}",
            target.display()
        ));
        return Ok(());
    }

    move_repo(&source, &target, needs_sudo)?;
    if needs_sudo {
        chown_repo(&target, &user)?;
    }
    write_config(&target)?;

    println!();
    println!("  {} relocation complete", style("✓").green().bold());
    println!();
    println!("  Verify the new location works:");
    println!(
        "    {}",
        style(format!(
            "sudo nixos-rebuild switch --flake {}#$(hostname)",
            target.display()
        ))
        .cyan()
    );
    println!();
    println!("  Once that succeeds, /etc/nixos is no longer used.");
    println!();

    Ok(())
}

/// Find the existing repo by consulting the nex config / discovery.
/// Falls back to /etc/nixos directly so this command works even when the
/// stored config is stale or never existed.
fn resolve_source() -> Result<PathBuf> {
    if let Ok(repo) = discover::find_repo() {
        return Ok(repo);
    }

    let etc = PathBuf::from("/etc/nixos");
    if etc.join("flake.nix").exists() || etc.join("configuration.nix").exists() {
        return Ok(etc);
    }

    bail!(
        "no nix config found to relocate. Run `nex init` to scaffold a fresh \
         user-owned config, or pass --repo to override discovery."
    )
}

fn resolve_target(arg: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = arg {
        return Ok(p.to_path_buf());
    }
    let home = dirs::home_dir().context("no home directory")?;
    Ok(home.join(discover::default_repo_name()))
}

fn validate_move(source: &Path, target: &Path) -> Result<()> {
    if !source.exists() {
        bail!("source {} does not exist", source.display());
    }
    if source == target {
        bail!("source and target are the same path");
    }
    if target.exists() {
        let is_empty = target
            .read_dir()
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false);
        if !is_empty {
            bail!(
                "target {} already exists and is not empty. \
                 Pick a different --to path or remove the existing directory.",
                target.display()
            );
        }
    }
    if let Some(parent) = target.parent() {
        if !parent.exists() {
            bail!("target parent {} does not exist", parent.display());
        }
    }
    Ok(())
}

/// Best-effort check: can the current process write to this directory?
fn is_writable_by_user(path: &Path) -> bool {
    let probe = path.join(".nex-write-probe");
    let result = std::fs::File::create(&probe).is_ok();
    let _ = std::fs::remove_file(&probe);
    result
}

fn move_repo(source: &Path, target: &Path, needs_sudo: bool) -> Result<()> {
    if needs_sudo {
        output::status(&format!(
            "moving {} → {} (sudo required)",
            source.display(),
            target.display()
        ));
        let status = Command::new("sudo")
            .args([
                "mv",
                &source.display().to_string(),
                &target.display().to_string(),
            ])
            .status()
            .context("failed to invoke sudo mv")?;
        if !status.success() {
            bail!("sudo mv failed");
        }
    } else {
        output::status(&format!(
            "moving {} → {}",
            source.display(),
            target.display()
        ));
        std::fs::rename(source, target)
            .with_context(|| format!("rename {} -> {}", source.display(), target.display()))?;
    }
    Ok(())
}

fn chown_repo(target: &Path, user: &str) -> Result<()> {
    output::status(&format!("chowning {} to {user}:users", target.display()));
    let status = Command::new("sudo")
        .args([
            "chown",
            "-R",
            &format!("{user}:users"),
            &target.display().to_string(),
        ])
        .status()
        .context("failed to invoke sudo chown")?;
    if !status.success() {
        bail!("sudo chown failed");
    }
    Ok(())
}

fn write_config(target: &Path) -> Result<()> {
    let dir = config::config_dir()?;
    std::fs::create_dir_all(&dir)?;
    config::set_preference("repo_path", &format!("\"{}\"", target.display()))?;
    output::status(&format!(
        "updated {} → repo_path = {}",
        dir.join("config.toml").display(),
        target.display()
    ));
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn validate_rejects_same_path() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_path_buf();
        let err = validate_move(&path, &path).unwrap_err();
        assert!(err.to_string().contains("same path"));
    }

    #[test]
    fn validate_rejects_nonexistent_source() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("missing");
        let target = dir.path().join("target");
        let err = validate_move(&source, &target).unwrap_err();
        assert!(err.to_string().contains("does not exist"));
    }

    #[test]
    fn validate_rejects_nonempty_target() {
        let src_dir = TempDir::new().unwrap();
        let tgt_dir = TempDir::new().unwrap();
        std::fs::write(tgt_dir.path().join("file"), "hi").unwrap();
        let err = validate_move(src_dir.path(), tgt_dir.path()).unwrap_err();
        assert!(err.to_string().contains("not empty"));
    }

    #[test]
    fn validate_accepts_empty_target() {
        let src_dir = TempDir::new().unwrap();
        let tgt_dir = TempDir::new().unwrap();
        validate_move(src_dir.path(), tgt_dir.path()).unwrap();
    }

    #[test]
    fn validate_accepts_missing_target() {
        let src_dir = TempDir::new().unwrap();
        let parent = TempDir::new().unwrap();
        let target = parent.path().join("does-not-exist");
        validate_move(src_dir.path(), &target).unwrap();
    }
}
