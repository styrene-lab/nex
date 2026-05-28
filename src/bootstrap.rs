use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use console::style;

use crate::discover::Platform;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapScope {
    DarwinBootstrap,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapFinding {
    pub id: &'static str,
    pub message: String,
    pub repair: Option<BootstrapRepair>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapRepair {
    pub description: String,
    pub command_preview: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapReport {
    pub scope: BootstrapScope,
    pub findings: Vec<BootstrapFinding>,
}

impl BootstrapReport {
    pub fn has_blockers(&self) -> bool {
        !self.findings.is_empty()
    }
}

pub fn check(platform: Platform) -> Result<Option<BootstrapReport>> {
    match platform {
        Platform::Darwin => Ok(Some(check_darwin_bootstrap()?)),
        Platform::Linux => Ok(None),
    }
}

pub fn print_recommendations(report: &BootstrapReport) {
    if !report.has_blockers() {
        return;
    }
    eprintln!();
    eprintln!(
        "  {} Darwin bootstrap blockers detected:",
        style("!").yellow().bold()
    );
    for finding in &report.findings {
        eprintln!("    {} {}", style("!").yellow(), finding.message);
    }
    eprintln!();
    eprintln!(
        "  Run {} before activating this machine.",
        style("nex doctor --fix darwin-bootstrap").bold()
    );
}

pub fn ensure_switch_ready(platform: Platform) -> Result<()> {
    if let Some(report) = check(platform)? {
        if report.has_blockers() {
            print_recommendations(&report);
            bail!("Darwin bootstrap blockers must be fixed before activation");
        }
    }
    Ok(())
}

pub fn maybe_repair_for_init(platform: Platform, dry_run: bool) -> Result<()> {
    let Some(report) = check(platform)? else {
        return Ok(());
    };
    if !report.has_blockers() {
        return Ok(());
    }
    print_recommendations(&report);
    if dry_run {
        return Ok(());
    }
    let confirm = crate::input::input().confirm("  Repair Darwin bootstrap blockers now?", true)?;
    if confirm {
        repair(&report)?;
    }
    Ok(())
}

pub fn repair(report: &BootstrapReport) -> Result<()> {
    match report.scope {
        BootstrapScope::DarwinBootstrap => repair_darwin_bootstrap(report),
    }
}

fn check_darwin_bootstrap() -> Result<BootstrapReport> {
    let etc = etc_root();
    let mut findings = Vec::new();

    for name in ["bashrc", "zshrc"] {
        let path = etc.join(name);
        if shell_rc_blocks_activation(&path)? {
            let backup = next_backup_path(&path, "before-nix-darwin");
            findings.push(BootstrapFinding {
                id: "darwin.shellrc.unmanaged",
                message: format!(
                    "{} has unmanaged content and may block nix-darwin activation",
                    path.display()
                ),
                repair: Some(BootstrapRepair {
                    description: format!("move {} to {}", path.display(), backup.display()),
                    command_preview: vec![
                        "sudo".to_string(),
                        "mv".to_string(),
                        path.display().to_string(),
                        backup.display().to_string(),
                    ],
                }),
            });
        }
    }

    let synthetic = etc.join("synthetic.conf");
    if !synthetic.exists() {
        findings.push(BootstrapFinding {
            id: "darwin.synthetic-conf.missing",
            message: format!("{} is missing", synthetic.display()),
            repair: Some(BootstrapRepair {
                description: format!("create {} with root:wheel 0644", synthetic.display()),
                command_preview: vec![
                    "sudo touch /etc/synthetic.conf".to_string(),
                    "sudo chown root:wheel /etc/synthetic.conf".to_string(),
                    "sudo chmod 0644 /etc/synthetic.conf".to_string(),
                ],
            }),
        });
    }

    Ok(BootstrapReport {
        scope: BootstrapScope::DarwinBootstrap,
        findings,
    })
}

fn shell_rc_blocks_activation(path: &Path) -> Result<bool> {
    if !path.exists() || path.is_symlink() {
        return Ok(false);
    }
    if !path.is_file() {
        return Ok(false);
    }
    let content =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(!content.contains("nix-darwin") && !content.contains("NIX_DARWIN"))
}

fn repair_darwin_bootstrap(report: &BootstrapReport) -> Result<()> {
    eprintln!();
    eprintln!("  {} fixing Darwin bootstrap", style(">>>").cyan().bold());
    for finding in &report.findings {
        match finding.id {
            "darwin.shellrc.unmanaged" => {
                let Some(repair) = &finding.repair else {
                    continue;
                };
                let args = &repair.command_preview;
                if args.len() == 4 {
                    run_sudo(&args[1], &args[2..])?;
                    eprintln!("  {} {}", style("✓").green(), repair.description);
                }
            }
            "darwin.synthetic-conf.missing" => {
                run_sudo("touch", &["/etc/synthetic.conf".to_string()])?;
                run_sudo(
                    "chown",
                    &["root:wheel".to_string(), "/etc/synthetic.conf".to_string()],
                )?;
                run_sudo(
                    "chmod",
                    &["0644".to_string(), "/etc/synthetic.conf".to_string()],
                )?;
                eprintln!(
                    "  {} ensured /etc/synthetic.conf root:wheel 0644",
                    style("✓").green()
                );
            }
            _ => {}
        }
    }
    Ok(())
}

fn run_sudo(program: &str, args: &[String]) -> Result<()> {
    let status = Command::new("sudo")
        .arg(program)
        .args(args)
        .status()
        .with_context(|| format!("failed to run sudo {program}"))?;
    if !status.success() {
        bail!(
            "sudo {program} failed with exit code {}",
            status.code().unwrap_or(-1)
        );
    }
    Ok(())
}

fn etc_root() -> PathBuf {
    std::env::var_os("NEX_TEST_ETC_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/etc"))
}

fn next_backup_path(path: &Path, suffix: &str) -> PathBuf {
    let base = PathBuf::from(format!("{}.{}", path.display(), suffix));
    if !base.exists() {
        return base;
    }
    for i in 1.. {
        let candidate = PathBuf::from(format!("{}.{}.{}", path.display(), suffix, i));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::{check, next_backup_path, shell_rc_blocks_activation};
    use crate::discover::Platform;
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    #[test]
    fn backup_path_uses_suffix_when_free() {
        assert_eq!(
            next_backup_path(
                Path::new("/tmp/nex-bootstrap-test-file"),
                "before-nix-darwin"
            ),
            Path::new("/tmp/nex-bootstrap-test-file.before-nix-darwin")
        );
    }

    #[test]
    fn unmanaged_shell_rc_blocks_activation() {
        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("bashrc");
        fs::write(&path, "export PATH=/usr/local/bin:$PATH\n").expect("write shell rc");
        assert!(shell_rc_blocks_activation(&path).expect("classify shell rc"));
    }

    #[test]
    fn nix_darwin_shell_rc_does_not_block_activation() {
        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("zshrc");
        fs::write(&path, "# nix-darwin managed\n").expect("write shell rc");
        assert!(!shell_rc_blocks_activation(&path).expect("classify shell rc"));
    }

    #[test]
    fn darwin_check_reports_shellrc_and_synthetic_conf_blockers() {
        let dir = tempdir().expect("temp dir");
        fs::write(dir.path().join("bashrc"), "legacy bashrc\n").expect("write bashrc");
        std::env::set_var("NEX_TEST_ETC_ROOT", dir.path());
        let report = check(Platform::Darwin)
            .expect("check bootstrap")
            .expect("darwin report");
        std::env::remove_var("NEX_TEST_ETC_ROOT");

        assert_eq!(report.findings.len(), 2);
        assert!(report
            .findings
            .iter()
            .any(|f| f.id == "darwin.shellrc.unmanaged"));
        assert!(report
            .findings
            .iter()
            .any(|f| f.id == "darwin.synthetic-conf.missing"));
    }
}
