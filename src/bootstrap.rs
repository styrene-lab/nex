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
    pub kind: BootstrapRepairKind,
}

impl BootstrapRepair {
    pub fn command_preview(&self) -> Vec<String> {
        match &self.kind {
            BootstrapRepairKind::MoveShellRc { from } => {
                let to = next_backup_path(from, "before-nix-darwin");
                vec![
                    "sudo".to_string(),
                    "mv".to_string(),
                    from.display().to_string(),
                    to.display().to_string(),
                ]
            }
            BootstrapRepairKind::EnsureSyntheticConf { path } => vec![
                format!("sudo touch {}", path.display()),
                format!("sudo chown root:wheel {}", path.display()),
                format!("sudo chmod 0644 {}", path.display()),
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BootstrapRepairKind {
    MoveShellRc { from: PathBuf },
    EnsureSyntheticConf { path: PathBuf },
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
        if let Some(repair) = &finding.repair {
            for command in repair.command_preview() {
                eprintln!("      {}", style(command).dim());
            }
        }
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
    check_darwin_bootstrap_at(&etc_root())
}

fn check_darwin_bootstrap_at(etc: &Path) -> Result<BootstrapReport> {
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
                    kind: BootstrapRepairKind::MoveShellRc { from: path },
                }),
            });
        }
    }

    add_synthetic_conf_findings(&etc.join("synthetic.conf"), &mut findings)?;

    Ok(BootstrapReport {
        scope: BootstrapScope::DarwinBootstrap,
        findings,
    })
}

fn add_synthetic_conf_findings(path: &Path, findings: &mut Vec<BootstrapFinding>) -> Result<()> {
    if !path.exists() {
        findings.push(synthetic_conf_finding(
            "darwin.synthetic-conf.missing",
            format!("{} is missing", path.display()),
            path,
        ));
        return Ok(());
    }

    let metadata =
        std::fs::metadata(path).with_context(|| format!("reading {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let mode = metadata.permissions().mode() & 0o777;
        if metadata.uid() != 0 || metadata.gid() != 0 {
            findings.push(synthetic_conf_finding(
                "darwin.synthetic-conf.owner",
                format!("{} is not owned by root:wheel", path.display()),
                path,
            ));
        }
        if mode != 0o644 {
            findings.push(synthetic_conf_finding(
                "darwin.synthetic-conf.mode",
                format!("{} has mode {:03o}; expected 644", path.display(), mode),
                path,
            ));
        }
    }
    Ok(())
}

fn synthetic_conf_finding(id: &'static str, message: String, path: &Path) -> BootstrapFinding {
    BootstrapFinding {
        id,
        message,
        repair: Some(BootstrapRepair {
            description: format!("ensure {} exists with root:wheel 0644", path.display()),
            kind: BootstrapRepairKind::EnsureSyntheticConf {
                path: path.to_path_buf(),
            },
        }),
    }
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
        let Some(repair) = &finding.repair else {
            continue;
        };
        match &repair.kind {
            BootstrapRepairKind::MoveShellRc { from } => {
                let to = next_backup_path(from, "before-nix-darwin");
                run_sudo(
                    "mv",
                    &[from.display().to_string(), to.display().to_string()],
                )?;
                eprintln!(
                    "  {} moved {} to {}",
                    style("✓").green(),
                    from.display(),
                    to.display()
                );
            }
            BootstrapRepairKind::EnsureSyntheticConf { path } => {
                run_sudo("touch", &[path.display().to_string()])?;
                run_sudo(
                    "chown",
                    &["root:wheel".to_string(), path.display().to_string()],
                )?;
                run_sudo("chmod", &["0644".to_string(), path.display().to_string()])?;
                eprintln!(
                    "  {} ensured {} root:wheel 0644",
                    style("✓").green(),
                    path.display()
                );
            }
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
    if std::env::var_os("NEX_TESTING").is_some() {
        if let Some(root) = std::env::var_os("NEX_TEST_ETC_ROOT") {
            return PathBuf::from(root);
        }
    }
    PathBuf::from("/etc")
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
    use super::{
        check_darwin_bootstrap_at, etc_root, next_backup_path, shell_rc_blocks_activation,
    };
    use std::fs;
    use std::path::{Path, PathBuf};
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
    fn test_etc_root_requires_testing_guard() {
        let dir = tempdir().expect("temp dir");
        let previous_testing = std::env::var_os("NEX_TESTING");
        let previous_root = std::env::var_os("NEX_TEST_ETC_ROOT");
        std::env::remove_var("NEX_TESTING");
        std::env::remove_var("NEX_TEST_ETC_ROOT");
        std::env::set_var("NEX_TEST_ETC_ROOT", dir.path());
        assert_eq!(etc_root(), PathBuf::from("/etc"));
        std::env::remove_var("NEX_TEST_ETC_ROOT");
        if let Some(value) = previous_testing {
            std::env::set_var("NEX_TESTING", value);
        }
        if let Some(value) = previous_root {
            std::env::set_var("NEX_TEST_ETC_ROOT", value);
        }
    }

    #[test]
    fn darwin_check_reports_shellrc_and_synthetic_conf_blockers() {
        let dir = tempdir().expect("temp dir");
        fs::write(dir.path().join("bashrc"), "legacy bashrc\n").expect("write bashrc");
        let report = check_darwin_bootstrap_at(dir.path()).expect("check bootstrap");

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

    #[test]
    fn darwin_check_reports_synthetic_conf_mode_blocker() {
        let dir = tempdir().expect("temp dir");
        let synthetic = dir.path().join("synthetic.conf");
        fs::write(&synthetic, "").expect("write synthetic conf");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&synthetic).expect("metadata").permissions();
            permissions.set_mode(0o600);
            fs::set_permissions(&synthetic, permissions).expect("set mode");
        }
        let report = check_darwin_bootstrap_at(dir.path()).expect("check bootstrap");

        assert!(report
            .findings
            .iter()
            .any(|f| f.id == "darwin.synthetic-conf.mode"));
    }
}
