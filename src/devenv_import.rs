use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const DEVENVP_IMPORT_SCHEMA_V1: &str = "io.styrene.nex.devenv-import-report.v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DevenvImportReport {
    pub schema: String,
    pub root: PathBuf,
    pub mode: DevenvImportMode,
    pub detected: DevenvDetectedFiles,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub source_hashes: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<DevenvImportItem>,
    pub summary: DevenvImportSummary,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<DevenvImportMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DevenvImportMode {
    Static,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DevenvDetectedFiles {
    pub devenv_nix: bool,
    pub devenv_yaml: bool,
    pub devenv_lock: bool,
    pub devenv_local_nix: bool,
    pub devenv_local_yaml: bool,
    pub envrc: bool,
    pub secretspec_toml: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DevenvImportSummary {
    pub portable: usize,
    pub project_scoped: usize,
    pub machine_scoped_candidate: usize,
    pub requires_review: usize,
    pub unsupported: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DevenvImportItem {
    pub id: String,
    pub kind: DevenvImportKind,
    pub bucket: DevenvImportBucket,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub safety: Vec<DevenvSafetyTag>,
    pub source: DevenvImportSource,
    pub devenv: DevenvSourceSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nex_candidate: Option<NexCandidateSummary>,
    pub review: DevenvReviewState,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<DevenvImportMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DevenvImportKind {
    Package,
    Language,
    Service,
    Process,
    Task,
    ShellHook,
    Test,
    Output,
    Container,
    SecretContract,
    DotenvProvider,
    GitHook,
    Overlay,
    Import,
    BinaryCache,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DevenvImportBucket {
    Portable,
    ProjectScoped,
    MachineScopedCandidate,
    RequiresReview,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DevenvSafetyTag {
    ReadOnly,
    LocalFileWrite,
    NetworkRead,
    NetworkWrite,
    Build,
    UserConfigMutation,
    SystemConfigMutation,
    PrivilegedMutation,
    HardwareDriverMutation,
    DestructiveDiskOperation,
    SecretContract,
    SecretValueRuntime,
    IdentitySigning,
    ArbitraryCommand,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DevenvImportSource {
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DevenvSourceSummary {
    pub option: String,
    pub value_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NexCandidateSummary {
    pub target: String,
    pub value_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DevenvReviewState {
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub resolved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DevenvImportMessage {
    pub code: String,
    pub message: String,
}

pub fn inspect_devenv_project(root: &Path) -> Result<DevenvImportReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let detected = detect_files(&root);
    let mut report = DevenvImportReport {
        schema: DEVENVP_IMPORT_SCHEMA_V1.to_string(),
        root: root.clone(),
        mode: DevenvImportMode::Static,
        detected,
        source_hashes: BTreeMap::new(),
        items: Vec::new(),
        summary: DevenvImportSummary::default(),
        warnings: Vec::new(),
    };

    collect_hashes(&root, &report.detected, &mut report.source_hashes)?;
    inspect_devenv_nix(&root, &mut report)?;
    inspect_devenv_yaml(&root, &mut report)?;
    inspect_secretspec(&root, &mut report)?;
    inspect_local_files(&mut report);
    report.summary = summarize(&report.items);
    Ok(report)
}

fn detect_files(root: &Path) -> DevenvDetectedFiles {
    DevenvDetectedFiles {
        devenv_nix: root.join("devenv.nix").exists(),
        devenv_yaml: root.join("devenv.yaml").exists(),
        devenv_lock: root.join("devenv.lock").exists(),
        devenv_local_nix: root.join("devenv.local.nix").exists(),
        devenv_local_yaml: root.join("devenv.local.yaml").exists(),
        envrc: root.join(".envrc").exists(),
        secretspec_toml: root.join("secretspec.toml").exists(),
    }
}

fn collect_hashes(
    root: &Path,
    detected: &DevenvDetectedFiles,
    hashes: &mut BTreeMap<String, String>,
) -> Result<()> {
    for (enabled, file) in [
        (detected.devenv_nix, "devenv.nix"),
        (detected.devenv_yaml, "devenv.yaml"),
        (detected.devenv_lock, "devenv.lock"),
        (detected.devenv_local_nix, "devenv.local.nix"),
        (detected.devenv_local_yaml, "devenv.local.yaml"),
        (detected.envrc, ".envrc"),
        (detected.secretspec_toml, "secretspec.toml"),
    ] {
        if enabled {
            let bytes =
                std::fs::read(root.join(file)).with_context(|| format!("reading {file}"))?;
            hashes.insert(
                file.to_string(),
                format!("sha256:{:x}", Sha256::digest(bytes)),
            );
        }
    }
    Ok(())
}

fn inspect_devenv_nix(root: &Path, report: &mut DevenvImportReport) -> Result<()> {
    let path = root.join("devenv.nix");
    if !path.exists() {
        return Ok(());
    }
    let content = std::fs::read_to_string(&path).context("reading devenv.nix")?;
    for (needle, kind, bucket, target, safety) in [
        (
            "packages",
            DevenvImportKind::Package,
            DevenvImportBucket::Portable,
            "profile.packages",
            vec![DevenvSafetyTag::Build],
        ),
        (
            "languages.",
            DevenvImportKind::Language,
            DevenvImportBucket::Portable,
            "profile.fragments.dev",
            vec![DevenvSafetyTag::Build],
        ),
        (
            "services.",
            DevenvImportKind::Service,
            DevenvImportBucket::MachineScopedCandidate,
            "profile.services",
            vec![DevenvSafetyTag::SystemConfigMutation],
        ),
        (
            "processes",
            DevenvImportKind::Process,
            DevenvImportBucket::ProjectScoped,
            "profile.processes",
            vec![DevenvSafetyTag::ArbitraryCommand],
        ),
        (
            "tasks.",
            DevenvImportKind::Task,
            DevenvImportBucket::RequiresReview,
            "profile.tasks",
            vec![DevenvSafetyTag::ArbitraryCommand],
        ),
        (
            "enterShell",
            DevenvImportKind::ShellHook,
            DevenvImportBucket::RequiresReview,
            "profile.shellHooks",
            vec![DevenvSafetyTag::ArbitraryCommand],
        ),
        (
            "enterTest",
            DevenvImportKind::Test,
            DevenvImportBucket::Portable,
            "profile.tests",
            vec![DevenvSafetyTag::Build],
        ),
        (
            "outputs",
            DevenvImportKind::Output,
            DevenvImportBucket::Portable,
            "profile.outputs",
            vec![DevenvSafetyTag::Build],
        ),
        (
            "containers",
            DevenvImportKind::Container,
            DevenvImportBucket::Portable,
            "profile.outputs.container",
            vec![DevenvSafetyTag::Build],
        ),
    ] {
        if content.contains(needle) {
            report.items.push(item(
                format!("devenv.nix:{needle}"),
                kind,
                bucket,
                "devenv.nix",
                Some(needle.to_string()),
                needle.to_string(),
                format!("detected {needle}"),
                Some((
                    target.to_string(),
                    format!("candidate mapping for {needle}"),
                )),
                safety,
                review_for_bucket(bucket, None),
            ));
        }
    }
    if content.contains("imports") {
        report.items.push(item(
            "devenv.nix:imports".to_string(),
            DevenvImportKind::Import,
            DevenvImportBucket::RequiresReview,
            "devenv.nix",
            Some("imports".to_string()),
            "imports".to_string(),
            "devenv imports require review before profile migration".to_string(),
            Some((
                "profile.imports".to_string(),
                "candidate imports".to_string(),
            )),
            vec![DevenvSafetyTag::Build],
            review_for_bucket(
                DevenvImportBucket::RequiresReview,
                Some("devenv imports can execute arbitrary Nix or pull project-local modules"),
            ),
        ));
    }
    Ok(())
}

fn inspect_devenv_yaml(root: &Path, report: &mut DevenvImportReport) -> Result<()> {
    let path = root.join("devenv.yaml");
    if !path.exists() {
        return Ok(());
    }
    let content = std::fs::read_to_string(&path).context("reading devenv.yaml")?;
    let yaml: serde_yaml::Value = serde_yaml::from_str(&content).context("parsing devenv.yaml")?;
    if yaml.get("secretspec").is_some() {
        report.items.push(item(
            "devenv.yaml:secretspec".to_string(),
            DevenvImportKind::SecretContract,
            DevenvImportBucket::Portable,
            "devenv.yaml",
            Some("secretspec".to_string()),
            "secretspec".to_string(),
            "SecretSpec integration configured".to_string(),
            Some((
                "profile.secrets.provider".to_string(),
                "provider/profile hint".to_string(),
            )),
            vec![DevenvSafetyTag::SecretContract],
            review_for_bucket(DevenvImportBucket::Portable, None),
        ));
    }
    Ok(())
}

fn inspect_secretspec(root: &Path, report: &mut DevenvImportReport) -> Result<()> {
    let path = root.join("secretspec.toml");
    if !path.exists() {
        return Ok(());
    }
    let content = std::fs::read_to_string(&path).context("reading secretspec.toml")?;
    let value: toml::Value = toml::from_str(&content).context("parsing secretspec.toml")?;
    if let Some(profiles) = value.get("profiles").and_then(toml::Value::as_table) {
        for (profile_name, profile) in profiles {
            let Some(secrets) = profile.as_table() else {
                continue;
            };
            for secret_name in secrets.keys() {
                report.items.push(item(
                    format!("secretspec:{profile_name}:{secret_name}"),
                    DevenvImportKind::SecretContract,
                    DevenvImportBucket::Portable,
                    "secretspec.toml",
                    Some(format!("profiles.{profile_name}.{secret_name}")),
                    format!("profiles.{profile_name}.{secret_name}"),
                    format!("secret contract {secret_name}"),
                    Some(("profile.secrets.required".to_string(), secret_name.clone())),
                    vec![DevenvSafetyTag::SecretContract],
                    review_for_bucket(DevenvImportBucket::Portable, None),
                ));
            }
        }
    }
    Ok(())
}

fn inspect_local_files(report: &mut DevenvImportReport) {
    for (enabled, file) in [
        (report.detected.devenv_local_nix, "devenv.local.nix"),
        (report.detected.devenv_local_yaml, "devenv.local.yaml"),
        (report.detected.envrc, ".envrc"),
    ] {
        if enabled {
            report.items.push(item(
                format!("local:{file}"),
                DevenvImportKind::Unknown,
                DevenvImportBucket::ProjectScoped,
                file,
                None,
                file.to_string(),
                "local project-only configuration detected".to_string(),
                None,
                vec![DevenvSafetyTag::LocalFileWrite],
                review_for_bucket(DevenvImportBucket::ProjectScoped, None),
            ));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn item(
    id: String,
    kind: DevenvImportKind,
    bucket: DevenvImportBucket,
    file: &str,
    source_path: Option<String>,
    option: String,
    value_summary: String,
    nex_candidate: Option<(String, String)>,
    safety: Vec<DevenvSafetyTag>,
    review: DevenvReviewState,
) -> DevenvImportItem {
    DevenvImportItem {
        id,
        kind,
        bucket,
        safety,
        source: DevenvImportSource {
            file: file.to_string(),
            path: source_path,
            line: None,
        },
        devenv: DevenvSourceSummary {
            option,
            value_summary,
        },
        nex_candidate: nex_candidate.map(|(target, value_summary)| NexCandidateSummary {
            target,
            value_summary,
        }),
        review,
        messages: Vec::new(),
    }
}

fn review_for_bucket(bucket: DevenvImportBucket, reason: Option<&str>) -> DevenvReviewState {
    DevenvReviewState {
        required: bucket == DevenvImportBucket::RequiresReview,
        reason: reason.map(ToString::to_string).or_else(|| {
            (bucket == DevenvImportBucket::RequiresReview)
                .then(|| "requires operator review before migration".to_string())
        }),
        resolved: false,
        resolution: None,
    }
}

fn summarize(items: &[DevenvImportItem]) -> DevenvImportSummary {
    let mut summary = DevenvImportSummary::default();
    for item in items {
        match item.bucket {
            DevenvImportBucket::Portable => summary.portable += 1,
            DevenvImportBucket::ProjectScoped => summary.project_scoped += 1,
            DevenvImportBucket::MachineScopedCandidate => summary.machine_scoped_candidate += 1,
            DevenvImportBucket::RequiresReview => summary.requires_review += 1,
            DevenvImportBucket::Unsupported => summary.unsupported += 1,
        }
    }
    summary
}

pub const DEVENVP_MIGRATION_PLAN_SCHEMA_V1: &str = "io.styrene.nex.devenv-migration-plan.v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DevenvMigrationPlan {
    pub schema: String,
    pub root: PathBuf,
    pub import_schema: String,
    pub summary: DevenvImportSummary,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<DevenvMigrationAction>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocked: Vec<DevenvMigrationAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DevenvMigrationAction {
    pub id: String,
    pub kind: DevenvImportKind,
    pub bucket: DevenvImportBucket,
    pub source: DevenvImportSource,
    pub action: DevenvMigrationActionKind,
    pub target: String,
    pub value_summary: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub safety: Vec<DevenvSafetyTag>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blockers: Vec<DevenvImportMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DevenvMigrationActionKind {
    GenerateProfileFragment,
    GenerateSecretContract,
    GenerateProfileOutput,
    ManualReview,
    SkipLocalOnly,
}

pub fn plan_devenv_migration(root: &Path) -> Result<DevenvMigrationPlan> {
    let report = inspect_devenv_project(root)?;
    let mut actions = Vec::new();
    let mut blocked = Vec::new();
    for item in &report.items {
        let target = item
            .nex_candidate
            .as_ref()
            .map(|candidate| candidate.target.clone())
            .unwrap_or_else(|| "manual-review".to_string());
        let action = action_for_item(item);
        let mut migration = DevenvMigrationAction {
            id: item.id.clone(),
            kind: item.kind.clone(),
            bucket: item.bucket,
            source: item.source.clone(),
            action,
            target,
            value_summary: item.devenv.value_summary.clone(),
            safety: item.safety.clone(),
            blockers: Vec::new(),
        };
        if item.review.required || item.bucket == DevenvImportBucket::MachineScopedCandidate {
            migration.blockers.push(DevenvImportMessage {
                code: "REVIEW_REQUIRED".to_string(),
                message: item
                    .review
                    .reason
                    .clone()
                    .unwrap_or_else(|| "operator review required before migration".to_string()),
            });
        }
        if migration.blockers.is_empty() {
            actions.push(migration);
        } else {
            blocked.push(migration);
        }
    }
    Ok(DevenvMigrationPlan {
        schema: DEVENVP_MIGRATION_PLAN_SCHEMA_V1.to_string(),
        root: report.root,
        import_schema: report.schema,
        summary: report.summary,
        actions,
        blocked,
    })
}

fn action_for_item(item: &DevenvImportItem) -> DevenvMigrationActionKind {
    match item.bucket {
        DevenvImportBucket::ProjectScoped => return DevenvMigrationActionKind::SkipLocalOnly,
        DevenvImportBucket::RequiresReview
        | DevenvImportBucket::MachineScopedCandidate
        | DevenvImportBucket::Unsupported => return DevenvMigrationActionKind::ManualReview,
        DevenvImportBucket::Portable => {}
    }
    match item.kind {
        DevenvImportKind::SecretContract | DevenvImportKind::DotenvProvider => {
            DevenvMigrationActionKind::GenerateSecretContract
        }
        DevenvImportKind::Output | DevenvImportKind::Container => {
            DevenvMigrationActionKind::GenerateProfileOutput
        }
        _ => DevenvMigrationActionKind::GenerateProfileFragment,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statically_inspects_devenv_project_with_secretspec() -> Result<()> {
        let dir = tempfile::tempdir()?;
        std::fs::write(
            dir.path().join("devenv.nix"),
            r#"{ pkgs, ... }: {
              packages = [ pkgs.git ];
              languages.rust.enable = true;
              services.postgres.enable = true;
              enterShell = ''echo hello'';
              containers.shell.copyToRoot = null;
            }"#,
        )?;
        std::fs::write(
            dir.path().join("devenv.yaml"),
            "secretspec:\n  enable: true\n  provider: keyring\n  profile: default\n",
        )?;
        std::fs::write(
            dir.path().join("secretspec.toml"),
            r#"[project]
name = "example"
revision = "1.0"

[profiles.default]
DATABASE_URL = { description = "Database URL", required = true }
API_TOKEN = { description = "API token", required = false }
"#,
        )?;

        let report = inspect_devenv_project(dir.path())?;

        assert!(report.detected.devenv_nix);
        assert!(report.detected.devenv_yaml);
        assert!(report.detected.secretspec_toml);
        assert!(report.summary.portable >= 5);
        assert_eq!(report.summary.machine_scoped_candidate, 1);
        assert_eq!(report.summary.requires_review, 1);
        assert!(report
            .items
            .iter()
            .any(|item| item.id == "secretspec:default:DATABASE_URL"));
        Ok(())
    }
}
