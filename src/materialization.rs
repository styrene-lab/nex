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

    pub fn command(&self) -> Command {
        let mut command = Command::new(find_nix());
        command
            .arg("eval")
            .arg(self.eval_attr())
            .current_dir(&self.workspace);
        command
    }

    pub fn run(&self) -> Result<()> {
        let output = self
            .command()
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
        let check = MaterializationCheck {
            workspace: PathBuf::from("/tmp/nex-materialization"),
            hostname: "test-host".to_string(),
        };
        let command = check.command();
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>();

        assert_eq!(args, vec!["eval".to_string(), nixos_toplevel_attr("test-host")]);
        assert_eq!(command.get_current_dir(), Some(Path::new("/tmp/nex-materialization")));
    }
}
