use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

#[derive(Debug, Clone)]
pub struct MaterializationCheck {
    pub workspace: PathBuf,
    pub hostname: String,
}

impl MaterializationCheck {
    pub fn eval_attr(&self) -> String {
        nixos_toplevel_attr(&self.hostname)
    }

    pub fn command(&self) -> Result<Command> {
        validate_hostname(&self.hostname)?;
        validate_workspace(&self.workspace)?;

        let mut command = Command::new(find_nix());
        command
            .args(["--extra-experimental-features", "nix-command flakes"])
            .arg("eval")
            .arg(self.eval_attr())
            .current_dir(&self.workspace);
        Ok(command)
    }

    pub fn run(&self) -> Result<()> {
        let output = self
            .command()?
            .output()
            .with_context(|| format!("running nix eval in {}", self.workspace.display()))?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        bail!(
            "materialization check failed for {}\n{}{}",
            self.eval_attr(),
            stdout,
            stderr
        );
    }
}

pub fn nixos_toplevel_attr(hostname: &str) -> String {
    format!(".#nixosConfigurations.{hostname}.config.system.build.toplevel")
}

pub fn validate_hostname(hostname: &str) -> Result<()> {
    if hostname.is_empty() || hostname.len() > 63 {
        bail!("hostname must be 1-63 characters");
    }
    if hostname.starts_with('-') || hostname.ends_with('-') {
        bail!("hostname cannot start or end with a hyphen");
    }
    if !hostname
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        bail!("hostname must contain only ASCII letters, digits, and hyphens");
    }
    Ok(())
}

pub fn validate_workspace(workspace: &Path) -> Result<()> {
    if !workspace.is_dir() {
        bail!("materialization workspace does not exist: {}", workspace.display());
    }
    let flake = workspace.join("flake.nix");
    if !flake.is_file() {
        bail!(
            "materialization workspace {} does not contain flake.nix",
            workspace.display()
        );
    }
    Ok(())
}

pub fn find_nix() -> String {
    let candidates = [
        "/nix/var/nix/profiles/default/bin/nix",
        "/run/current-system/sw/bin/nix",
        "/etc/profiles/per-user/default/bin/nix",
    ];
    for path in candidates {
        if Path::new(path).exists() {
            return path.to_string();
        }
    }
    "nix".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_nixos_toplevel_attr() {
        assert_eq!(
            nixos_toplevel_attr("test-host"),
            ".#nixosConfigurations.test-host.config.system.build.toplevel"
        );
    }

    #[test]
    fn command_uses_eval_attr_and_workspace() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("flake.nix"), "{}").expect("write flake");
        let check = MaterializationCheck {
            workspace: dir.path().to_path_buf(),
            hostname: "test-host".to_string(),
        };
        let command = check.command().expect("valid command");
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>();

        assert_eq!(
            args,
            vec![
                "--extra-experimental-features".to_string(),
                "nix-command flakes".to_string(),
                "eval".to_string(),
                nixos_toplevel_attr("test-host"),
            ]
        );
        assert_eq!(command.get_current_dir(), Some(dir.path()));
    }

    #[test]
    fn rejects_invalid_hostname() {
        let error = validate_hostname("bad/host").expect_err("invalid hostname rejected");
        assert!(format!("{error:#}").contains("hostname must contain only"));
    }

    #[test]
    fn rejects_workspace_without_flake() {
        let dir = tempfile::tempdir().expect("tempdir");
        let error = validate_workspace(dir.path()).expect_err("missing flake rejected");
        assert!(format!("{error:#}").contains("does not contain flake.nix"));
    }
}
