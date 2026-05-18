//! Shared forge request, plan, and event model.
//!
//! This module is deliberately data-first. The existing CLI can translate flags
//! into these structs, and richer TUIs can render the same plan without parsing
//! human output.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

pub const FORGE_SCHEMA_VERSION: u16 = 1;
pub const CANONICAL_DEFINITION_FORMAT: &str = "pkl";
pub const CANONICAL_REQUEST_EXTENSION: &str = "pkl";
pub const CANONICAL_MANIFEST_EXTENSION: &str = "pkl";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForgeRequest {
    #[serde(default = "default_schema_version")]
    pub schema_version: u16,
    pub operation: ForgeOperation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    pub hostname: String,
    pub arch: ForgeArch,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_dir: Option<PathBuf>,
    pub target: ForgeTarget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub polymerize_defaults: Option<PolymerizeDefaults>,
    #[serde(default)]
    pub network: NetworkPolicy,
    #[serde(default)]
    pub safety: SafetyPolicy,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
}

impl ForgeRequest {
    pub fn new(
        operation: ForgeOperation,
        hostname: impl Into<String>,
        arch: ForgeArch,
        target: ForgeTarget,
    ) -> Self {
        Self {
            schema_version: FORGE_SCHEMA_VERSION,
            operation,
            profile: None,
            hostname: hostname.into(),
            arch,
            output_dir: None,
            target,
            polymerize_defaults: None,
            network: NetworkPolicy::default(),
            safety: SafetyPolicy::default(),
            labels: Vec::new(),
        }
    }

    pub fn profile(mut self, profile: impl Into<String>) -> Self {
        self.profile = Some(profile.into());
        self
    }

    pub fn output_dir(mut self, output_dir: impl Into<PathBuf>) -> Self {
        self.output_dir = Some(output_dir.into());
        self
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ForgeOperation {
    Bundle,
    UsbInstall,
    Image,
    Netboot,
    RemotePolymerize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ForgeArch {
    X86_64,
    Aarch64,
}

impl ForgeArch {
    pub fn label(self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64",
            Self::Aarch64 => "aarch64",
        }
    }

    pub fn iso_url(self) -> &'static str {
        match self {
            Self::X86_64 => {
                "https://channels.nixos.org/nixos-24.11/latest-nixos-minimal-x86_64-linux.iso"
            }
            Self::Aarch64 => {
                "https://channels.nixos.org/nixos-24.11/latest-nixos-minimal-aarch64-linux.iso"
            }
        }
    }

    pub fn iso_filename(self) -> String {
        format!("nixos-minimal-{}.iso", self.label())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForgeTarget {
    pub kind: TargetKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk: Option<String>,
}

impl ForgeTarget {
    pub fn bundle() -> Self {
        Self {
            kind: TargetKind::Bundle,
            disk: None,
        }
    }

    pub fn usb(disk: Option<impl Into<String>>) -> Self {
        Self {
            kind: TargetKind::Usb,
            disk: disk.map(Into::into),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TargetKind {
    Bundle,
    Usb,
    Image,
    Netboot,
    Remote,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolymerizeDefaults {
    pub username: String,
    pub timezone: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkPolicy {
    #[serde(default)]
    pub require_wired: bool,
    #[serde(default = "default_wifi_allowed")]
    pub wifi_allowed: bool,
}

impl Default for NetworkPolicy {
    fn default() -> Self {
        Self {
            require_wired: false,
            wifi_allowed: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SafetyPolicy {
    #[serde(default)]
    pub allow_destructive_flash: bool,
    #[serde(default)]
    pub allow_internal_disk_selection: bool,
    #[serde(default = "default_require_operator_confirmation")]
    pub require_operator_confirmation: bool,
}

impl Default for SafetyPolicy {
    fn default() -> Self {
        Self {
            allow_destructive_flash: false,
            allow_internal_disk_selection: false,
            require_operator_confirmation: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForgePlan {
    pub schema_version: u16,
    pub operation: ForgeOperation,
    pub hostname: String,
    pub arch: ForgeArch,
    pub output_dir: PathBuf,
    pub iso: Option<ForgeIsoPlan>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub destructive_actions: Vec<DestructiveAction>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<ForgeDiagnostic>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blockers: Vec<ForgeDiagnostic>,
}

impl ForgePlan {
    pub fn is_blocked(&self) -> bool {
        !self.blockers.is_empty()
    }
}

pub fn load_request(path: impl Into<PathBuf>) -> Result<ForgeRequest> {
    let path = path.into();
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("json") => {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            serde_json::from_str(&content)
                .with_context(|| format!("decoding JSON forge request {}", path.display()))
        }
        Some("pkl") => evaluate_pkl_request(&path)
            .with_context(|| format!("evaluating Pkl forge request {}", path.display())),
        Some(other) => {
            bail!("unsupported forge request extension .{other}; canonical requests use .pkl")
        }
        None => bail!("forge request path must have an extension; canonical requests use .pkl"),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForgeIsoPlan {
    pub url: String,
    pub filename: String,
    pub cache_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DestructiveAction {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForgeDiagnostic {
    pub code: String,
    pub message: String,
}

impl ForgeDiagnostic {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ForgeEvent {
    PhaseStarted {
        schema_version: u16,
        phase: String,
    },
    PhaseCompleted {
        schema_version: u16,
        phase: String,
    },
    Warning {
        schema_version: u16,
        code: String,
        message: String,
    },
    Blocker {
        schema_version: u16,
        code: String,
        message: String,
    },
    ArtifactCreated {
        schema_version: u16,
        path: PathBuf,
    },
    RunCompleted {
        schema_version: u16,
    },
    RunFailed {
        schema_version: u16,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForgeCheckReport {
    pub schema: String,
    pub evaluator: String,
    pub valid: bool,
    pub template: ForgeCheckTemplate,
    pub capabilities: ForgeCheckCapabilities,
    pub inputs: Vec<ForgeCheckInput>,
    pub outputs: Vec<ForgeCheckOutput>,
    pub warnings: Vec<ForgeDiagnostic>,
    pub errors: Vec<ForgeDiagnostic>,
}

impl ForgeCheckReport {
    pub fn exit_code(&self) -> i32 {
        if self.valid {
            return 0;
        }
        if self.errors.iter().any(|error| {
            matches!(
                error.code.as_str(),
                "UNSAFE_RAW_DISK_TARGET"
                    | "UNSAFE_CLUSTER_INIT"
                    | "UNSAFE_JOIN_TOKEN"
                    | "UNSAFE_EXECUTION_HOOK"
                    | "UNSAFE_PRIVATE_IMPORT"
                    | "UNDECLARED_DESTRUCTIVE_CAPABILITY"
                    | "PLAN_ONLY_EXECUTE_CAPABLE"
            )
        }) {
            3
        } else {
            1
        }
    }

    pub fn evaluator_error(message: String) -> Self {
        Self {
            schema: FORGE_TEMPLATE_SCHEMA.to_string(),
            evaluator: CANONICAL_DEFINITION_FORMAT.to_string(),
            valid: false,
            template: ForgeCheckTemplate::default(),
            capabilities: ForgeCheckCapabilities::default(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            warnings: Vec::new(),
            errors: vec![ForgeDiagnostic::new("EVALUATOR_ERROR", message)],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForgeCheckTemplate {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(rename = "evaluationMode")]
    pub evaluation_mode: String,
    #[serde(rename = "safetyClass")]
    pub safety_class: String,
    #[serde(rename = "canonicalFormat")]
    pub canonical_format: String,
}

impl Default for ForgeCheckTemplate {
    fn default() -> Self {
        Self {
            id: "unknown".to_string(),
            version: None,
            evaluation_mode: "plan-only".to_string(),
            safety_class: "unknown".to_string(),
            canonical_format: CANONICAL_DEFINITION_FORMAT.to_string(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForgeCheckCapabilities {
    pub destructive: Vec<String>,
    pub network: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForgeCheckInput {
    pub name: String,
    pub kind: String,
    pub required: bool,
    pub sensitive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForgeCheckOutput {
    pub kind: String,
}

const FORGE_TEMPLATE_SCHEMA: &str = "dev.styrene.nex.forge-template.v1";

pub fn check_template(
    template_path: &Path,
    metadata_path: Option<&Path>,
    _no_execute: bool,
) -> Result<ForgeCheckReport> {
    if template_path.extension().and_then(|ext| ext.to_str()) != Some("pkl") {
        bail!("forge template check requires a canonical .pkl payload");
    }

    let source = std::fs::read_to_string(template_path)
        .with_context(|| format!("reading {}", template_path.display()))?;
    let metadata = metadata_path
        .map(load_forge_template_metadata)
        .transpose()?;
    let public_by_metadata = metadata
        .as_ref()
        .and_then(|metadata| metadata.visibility.as_deref())
        == Some("public");
    let mut source_errors = Vec::new();
    if public_by_metadata {
        validate_public_template_source_safety(&source, &mut source_errors);
    }

    let evaluated = evaluate_pkl_json(template_path)?;
    let request = request_from_evaluated_pkl(evaluated.clone())?;
    if !public_by_metadata && evaluated_visibility_is_public(&evaluated) {
        validate_public_template_source_safety(&source, &mut source_errors);
    }
    let mut report = build_check_report(&evaluated, &request, metadata.as_ref());
    report.errors.extend(source_errors);
    report.valid = report.errors.is_empty();
    Ok(report)
}

fn build_check_report(
    evaluated: &serde_json::Value,
    request: &ForgeRequest,
    metadata: Option<&ForgeTemplateMetadata>,
) -> ForgeCheckReport {
    let object = evaluated.as_object();
    let template_id = object
        .and_then(|object| value_string(object, &["id", "name"]))
        .or_else(|| metadata.and_then(|metadata| metadata.id.clone()))
        .unwrap_or_else(|| "unknown".to_string());
    let evaluation_mode = metadata
        .and_then(|metadata| metadata.evaluation_mode.clone())
        .unwrap_or_else(|| "plan-only".to_string());
    let safety_class = metadata
        .and_then(|metadata| metadata.safety_class.clone())
        .unwrap_or_else(|| safety_class_for_operation(request.operation).to_string());
    let destructive = destructive_capabilities(request);
    let network = network_capabilities(evaluated, request);
    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    let template = ForgeCheckTemplate {
        id: template_id,
        version: metadata.and_then(|metadata| metadata.version.clone()),
        evaluation_mode,
        safety_class,
        canonical_format: CANONICAL_DEFINITION_FORMAT.to_string(),
    };

    if let Some(metadata) = metadata {
        validate_metadata_agreement(
            metadata,
            evaluated,
            &template,
            &destructive,
            &network,
            &mut errors,
        );
    } else {
        warnings.push(ForgeDiagnostic::new(
            "METADATA_NOT_PROVIDED",
            "No forge.toml metadata was provided; metadata agreement checks were skipped.",
        ));
    }

    let evaluated_visibility = object.and_then(|object| value_string(object, &["visibility"]));
    let is_public = metadata
        .and_then(|metadata| metadata.visibility.as_deref())
        .or(evaluated_visibility.as_deref())
        == Some("public");
    if is_public {
        validate_public_template_safety(evaluated, &mut errors);
    }
    validate_evaluation_mode(&template, request, &destructive, &mut errors);

    ForgeCheckReport {
        schema: metadata
            .and_then(|metadata| metadata.schema.clone())
            .unwrap_or_else(|| FORGE_TEMPLATE_SCHEMA.to_string()),
        evaluator: metadata
            .and_then(|metadata| metadata.evaluator.clone())
            .unwrap_or_else(|| CANONICAL_DEFINITION_FORMAT.to_string()),
        valid: errors.is_empty(),
        template,
        capabilities: ForgeCheckCapabilities {
            destructive,
            network,
        },
        inputs: check_inputs(evaluated),
        outputs: check_outputs(request),
        warnings,
        errors,
    }
}

fn evaluated_visibility_is_public(evaluated: &serde_json::Value) -> bool {
    evaluated
        .as_object()
        .and_then(|object| value_string(object, &["visibility"]))
        .as_deref()
        == Some("public")
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ForgeTemplateMetadataFile {
    #[serde(default)]
    forge_template: ForgeTemplateMetadata,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ForgeTemplateMetadata {
    id: Option<String>,
    version: Option<String>,
    schema: Option<String>,
    evaluator: Option<String>,
    evaluation_mode: Option<String>,
    safety_class: Option<String>,
    canonical_format: Option<String>,
    visibility: Option<String>,
    profile_class: Option<String>,
    destructive_capabilities: Option<Vec<String>>,
    network_requirements: Option<Vec<String>>,
}

fn load_forge_template_metadata(path: &Path) -> Result<ForgeTemplateMetadata> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let parsed: ForgeTemplateMetadataFile = toml::from_str(&content)
        .with_context(|| format!("decoding forge template metadata {}", path.display()))?;
    Ok(parsed.forge_template)
}

fn validate_metadata_agreement(
    metadata: &ForgeTemplateMetadata,
    evaluated: &serde_json::Value,
    template: &ForgeCheckTemplate,
    destructive: &[String],
    network: &[String],
    errors: &mut Vec<ForgeDiagnostic>,
) {
    let object = evaluated.as_object();
    compare_optional(
        "id",
        metadata.id.as_deref(),
        Some(template.id.as_str()),
        "METADATA_ID_MISMATCH",
        errors,
    );
    compare_optional(
        "canonical_format",
        metadata.canonical_format.as_deref(),
        Some(CANONICAL_DEFINITION_FORMAT),
        "METADATA_CANONICAL_FORMAT_MISMATCH",
        errors,
    );
    compare_optional(
        "evaluator",
        metadata.evaluator.as_deref(),
        Some(CANONICAL_DEFINITION_FORMAT),
        "METADATA_EVALUATOR_MISMATCH",
        errors,
    );
    compare_optional(
        "evaluation_mode",
        metadata.evaluation_mode.as_deref(),
        Some(template.evaluation_mode.as_str()),
        "METADATA_EVALUATION_MODE_MISMATCH",
        errors,
    );
    compare_optional(
        "safety_class",
        metadata.safety_class.as_deref(),
        Some(template.safety_class.as_str()),
        "METADATA_SAFETY_CLASS_MISMATCH",
        errors,
    );
    if let Some(profile_class) = metadata.profile_class.as_deref() {
        let evaluated_profile_class =
            object.and_then(|object| value_string(object, &["profileClass", "profile_class"]));
        compare_optional(
            "profile_class",
            Some(profile_class),
            evaluated_profile_class.as_deref(),
            "METADATA_PROFILE_CLASS_MISMATCH",
            errors,
        );
    }
    compare_string_sets(
        "destructive_capabilities",
        metadata.destructive_capabilities.as_deref(),
        destructive,
        "METADATA_DESTRUCTIVE_CAPABILITIES_MISMATCH",
        errors,
    );
    compare_string_sets(
        "network_requirements",
        metadata.network_requirements.as_deref(),
        network,
        "METADATA_NETWORK_REQUIREMENTS_MISMATCH",
        errors,
    );
}

fn compare_optional(
    field: &str,
    declared: Option<&str>,
    detected: Option<&str>,
    code: &str,
    errors: &mut Vec<ForgeDiagnostic>,
) {
    let Some(declared) = declared else {
        return;
    };
    if detected != Some(declared) {
        errors.push(ForgeDiagnostic::new(
            code,
            format!(
                "Metadata field {field} declares {declared:?}, but Nex detected {:?}.",
                detected.unwrap_or("<missing>")
            ),
        ));
    }
}

fn compare_string_sets(
    field: &str,
    declared: Option<&[String]>,
    detected: &[String],
    code: &str,
    errors: &mut Vec<ForgeDiagnostic>,
) {
    let Some(declared) = declared else {
        return;
    };
    let declared: BTreeSet<&str> = declared.iter().map(String::as_str).collect();
    let detected: BTreeSet<&str> = detected.iter().map(String::as_str).collect();
    if declared != detected {
        errors.push(ForgeDiagnostic::new(
            code,
            format!(
                "Metadata field {field} declares {:?}, but Nex detected {:?}.",
                declared, detected
            ),
        ));
    }
}

fn validate_evaluation_mode(
    template: &ForgeCheckTemplate,
    request: &ForgeRequest,
    destructive: &[String],
    errors: &mut Vec<ForgeDiagnostic>,
) {
    if template.evaluation_mode != "plan-only" {
        errors.push(ForgeDiagnostic::new(
            "UNSUPPORTED_EVALUATION_MODE",
            format!(
                "Evaluation mode {:?} is not supported by forge check.",
                template.evaluation_mode
            ),
        ));
    }
    if template.evaluation_mode == "plan-only"
        && (request.operation == ForgeOperation::UsbInstall
            || destructive.iter().any(|cap| cap == "usb-flash"))
    {
        errors.push(ForgeDiagnostic::new(
            "PLAN_ONLY_EXECUTE_CAPABLE",
            "Plan-only templates cannot imply USB flashing or direct execution.",
        ));
    }
    for capability in destructive {
        if template.safety_class != *capability {
            errors.push(ForgeDiagnostic::new(
                "UNDECLARED_DESTRUCTIVE_CAPABILITY",
                format!(
                    "Safety class {:?} does not match detected capability {:?}.",
                    template.safety_class, capability
                ),
            ));
        }
    }
}

fn validate_public_template_safety(value: &serde_json::Value, errors: &mut Vec<ForgeDiagnostic>) {
    walk_json(value, &mut Vec::new(), &mut |path, value| {
        let key = path.last().map(String::as_str).unwrap_or_default();
        match value {
            serde_json::Value::String(text) => {
                if is_raw_disk_target(text) {
                    errors.push(ForgeDiagnostic::new(
                        "UNSAFE_RAW_DISK_TARGET",
                        format!(
                            "Public forge template contains fixed raw disk target at {path:?}."
                        ),
                    ));
                }
                if is_private_import(text) {
                    errors.push(ForgeDiagnostic::new(
                        "UNSAFE_PRIVATE_IMPORT",
                        format!("Public forge template contains private/local import at {path:?}."),
                    ));
                }
                if key.to_ascii_lowercase().contains("token") || text.contains("join_token") {
                    errors.push(ForgeDiagnostic::new(
                        "UNSAFE_JOIN_TOKEN",
                        format!("Public forge template contains token-like field at {path:?}."),
                    ));
                }
            }
            serde_json::Value::Bool(true) => {
                let key = key.to_ascii_lowercase();
                if key == "clusterinit" || key == "cluster_init" {
                    errors.push(ForgeDiagnostic::new(
                        "UNSAFE_CLUSTER_INIT",
                        format!(
                            "Public forge template enables cluster initialization at {path:?}."
                        ),
                    ));
                }
            }
            _ => {}
        }
        let key = key.to_ascii_lowercase();
        if matches!(
            key.as_str(),
            "hook" | "hooks" | "command" | "commands" | "script" | "scripts"
        ) || key.ends_with("hook")
            || key.ends_with("command")
            || key.ends_with("script")
        {
            errors.push(ForgeDiagnostic::new(
                "UNSAFE_EXECUTION_HOOK",
                format!("Public forge template contains execution hook field at {path:?}."),
            ));
        }
    });
}

fn validate_public_template_source_safety(source: &str, errors: &mut Vec<ForgeDiagnostic>) {
    for (line_index, line) in source.lines().enumerate() {
        let line_no = line_index + 1;
        let trimmed = line.trim();
        if trimmed.starts_with("import") && contains_private_import_text(trimmed) {
            errors.push(ForgeDiagnostic::new(
                "UNSAFE_PRIVATE_IMPORT",
                format!("Public forge template contains private/local import on line {line_no}."),
            ));
        }
        if contains_token_text(trimmed) {
            errors.push(ForgeDiagnostic::new(
                "UNSAFE_JOIN_TOKEN",
                format!("Public forge template contains token-like source text on line {line_no}."),
            ));
        }
        if contains_cluster_init_text(trimmed) {
            errors.push(ForgeDiagnostic::new(
                "UNSAFE_CLUSTER_INIT",
                format!(
                    "Public forge template contains cluster-init source text on line {line_no}."
                ),
            ));
        }
        if contains_execution_hook_text(trimmed) {
            errors.push(ForgeDiagnostic::new(
                "UNSAFE_EXECUTION_HOOK",
                format!(
                    "Public forge template contains execution-hook source text on line {line_no}."
                ),
            ));
        }
        if contains_raw_disk_text(trimmed) {
            errors.push(ForgeDiagnostic::new(
                "UNSAFE_RAW_DISK_TARGET",
                format!("Public forge template contains fixed raw disk target on line {line_no}."),
            ));
        }
    }
}

fn contains_private_import_text(line: &str) -> bool {
    line.contains("file:")
        || line.contains("import(\"/")
        || line.contains("import(\"../")
        || line.contains("import(\"./private")
        || line.contains("ssh://")
}

fn contains_token_text(line: &str) -> bool {
    let line = line.to_ascii_lowercase();
    line.contains("join_token") || line.contains("bootstrap_token") || line.contains("token_file")
}

fn contains_cluster_init_text(line: &str) -> bool {
    let line = line.to_ascii_lowercase();
    line.contains("clusterinit") || line.contains("cluster_init")
}

fn contains_execution_hook_text(line: &str) -> bool {
    let line = line.to_ascii_lowercase();
    line.contains("postinstall")
        || line.contains("preinstall")
        || line.contains("exechook")
        || line.contains("exec_hook")
        || line.contains("shellhook")
        || line.contains("shell_hook")
}

fn contains_raw_disk_text(line: &str) -> bool {
    line.contains("/dev/sda")
        || line.contains("/dev/vda")
        || line.contains("/dev/xvda")
        || line.contains("/dev/nvme")
        || line.contains("/dev/disk/")
}

fn walk_json<F>(value: &serde_json::Value, path: &mut Vec<String>, visit: &mut F)
where
    F: FnMut(&[String], &serde_json::Value),
{
    visit(path, value);
    match value {
        serde_json::Value::Object(object) => {
            for (key, child) in object {
                path.push(key.clone());
                walk_json(child, path, visit);
                path.pop();
            }
        }
        serde_json::Value::Array(values) => {
            for (index, child) in values.iter().enumerate() {
                path.push(index.to_string());
                walk_json(child, path, visit);
                path.pop();
            }
        }
        _ => {}
    }
}

fn is_raw_disk_target(value: &str) -> bool {
    value == "/dev/sda"
        || value == "/dev/vda"
        || value == "/dev/xvda"
        || value.starts_with("/dev/nvme")
        || value.starts_with("/dev/disk/")
}

fn is_private_import(value: &str) -> bool {
    value.starts_with("file:")
        || (value.starts_with('/') && !value.starts_with("/dev/"))
        || value.starts_with("../")
        || value.starts_with("./private")
        || value.contains("ssh://")
}

fn destructive_capabilities(request: &ForgeRequest) -> Vec<String> {
    match request.operation {
        ForgeOperation::Image => vec!["image-build".to_string()],
        ForgeOperation::UsbInstall => vec!["usb-flash".to_string()],
        _ => Vec::new(),
    }
}

fn network_capabilities(evaluated: &serde_json::Value, request: &ForgeRequest) -> Vec<String> {
    let mut network = BTreeSet::new();
    if request.network.require_wired {
        network.insert("wired-network".to_string());
    }
    if evaluated
        .pointer("/plan/requiresNetwork")
        .and_then(|value| value.as_bool())
        == Some(true)
    {
        network.insert("package-download".to_string());
    }
    network.into_iter().collect()
}

fn safety_class_for_operation(operation: ForgeOperation) -> &'static str {
    match operation {
        ForgeOperation::Image => "image-build",
        ForgeOperation::UsbInstall => "usb-flash",
        ForgeOperation::Bundle => "bundle",
        ForgeOperation::Netboot => "netboot",
        ForgeOperation::RemotePolymerize => "remote-polymerize",
    }
}

fn check_inputs(evaluated: &serde_json::Value) -> Vec<ForgeCheckInput> {
    if evaluated
        .pointer("/plan/target")
        .and_then(|value| value.as_str())
        == Some("operator-selected")
    {
        return vec![ForgeCheckInput {
            name: "target".to_string(),
            kind: "operator-selected".to_string(),
            required: true,
            sensitive: true,
        }];
    }
    Vec::new()
}

fn check_outputs(request: &ForgeRequest) -> Vec<ForgeCheckOutput> {
    let kind = match request.operation {
        ForgeOperation::Image => "image-plan",
        ForgeOperation::UsbInstall => "usb-install-plan",
        ForgeOperation::Bundle => "bundle-plan",
        ForgeOperation::Netboot => "netboot-plan",
        ForgeOperation::RemotePolymerize => "remote-polymerize-plan",
    };
    vec![ForgeCheckOutput {
        kind: kind.to_string(),
    }]
}

pub fn plan_request(request: &ForgeRequest) -> Result<ForgePlan> {
    if request.schema_version != FORGE_SCHEMA_VERSION {
        bail!(
            "unsupported forge request schema_version {}",
            request.schema_version
        );
    }

    let mut warnings = Vec::new();
    let mut blockers = Vec::new();
    validate_hostname(&request.hostname, &mut blockers);
    validate_operation_target(request, &mut blockers);

    if request.network.require_wired {
        warnings.push(ForgeDiagnostic::new(
            "WIRED_NETWORK_REQUIRED",
            "This request requires wired network during install/polymerize.",
        ));
    }

    if request.operation == ForgeOperation::RemotePolymerize {
        blockers.push(ForgeDiagnostic::new(
            "REMOTE_POLYMERIZE_NOT_IMPLEMENTED",
            "Remote polymerize is a later forge target and is not implemented yet.",
        ));
    }

    let output_dir = request.output_dir.clone().unwrap_or_else(|| {
        std::env::temp_dir()
            .join("nex-forge")
            .join(bundle_name(request.profile.as_deref()))
    });
    let iso = uses_installer_iso(request.operation).then(|| ForgeIsoPlan {
        url: request.arch.iso_url().to_string(),
        filename: request.arch.iso_filename(),
        cache_path: output_dir.join(request.arch.iso_filename()),
    });
    let destructive_actions = destructive_actions(request);

    if !destructive_actions.is_empty() && !request.safety.allow_destructive_flash {
        blockers.push(ForgeDiagnostic::new(
            "DESTRUCTIVE_FLASH_NOT_ALLOWED",
            "Request selects a destructive USB flash but safety.allow_destructive_flash is false.",
        ));
    }

    Ok(ForgePlan {
        schema_version: FORGE_SCHEMA_VERSION,
        operation: request.operation,
        hostname: request.hostname.clone(),
        arch: request.arch,
        output_dir,
        iso,
        destructive_actions,
        warnings,
        blockers,
    })
}

fn default_schema_version() -> u16 {
    FORGE_SCHEMA_VERSION
}

fn default_wifi_allowed() -> bool {
    true
}

fn default_require_operator_confirmation() -> bool {
    true
}

fn uses_installer_iso(operation: ForgeOperation) -> bool {
    matches!(
        operation,
        ForgeOperation::Bundle | ForgeOperation::UsbInstall
    )
}

fn bundle_name(profile: Option<&str>) -> String {
    profile
        .map(|r| r.replace('/', "_"))
        .unwrap_or_else(|| "styx".to_string())
}

fn validate_hostname(hostname: &str, blockers: &mut Vec<ForgeDiagnostic>) {
    if hostname.is_empty() {
        blockers.push(ForgeDiagnostic::new(
            "INVALID_HOSTNAME",
            "Hostname cannot be empty.",
        ));
        return;
    }
    if hostname.starts_with('-') || hostname.ends_with('-') {
        blockers.push(ForgeDiagnostic::new(
            "INVALID_HOSTNAME",
            "Hostname cannot start or end with a hyphen.",
        ));
    }
    if !hostname
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        blockers.push(ForgeDiagnostic::new(
            "INVALID_HOSTNAME",
            "Hostname must be alphanumeric with hyphens only.",
        ));
    }
}

fn validate_operation_target(request: &ForgeRequest, blockers: &mut Vec<ForgeDiagnostic>) {
    let valid = matches!(
        (request.operation, request.target.kind),
        (ForgeOperation::Bundle, TargetKind::Bundle)
            | (ForgeOperation::UsbInstall, TargetKind::Usb)
            | (ForgeOperation::Image, TargetKind::Image)
            | (ForgeOperation::Netboot, TargetKind::Netboot)
            | (ForgeOperation::RemotePolymerize, TargetKind::Remote)
    );

    if !valid {
        blockers.push(ForgeDiagnostic::new(
            "TARGET_OPERATION_MISMATCH",
            "Forge operation does not match target kind.",
        ));
    }
}

fn destructive_actions(request: &ForgeRequest) -> Vec<DestructiveAction> {
    if request.operation != ForgeOperation::UsbInstall {
        return Vec::new();
    }

    let Some(disk) = request.target.disk.as_ref() else {
        return Vec::new();
    };

    vec![DestructiveAction {
        code: "FLASH_USB".to_string(),
        message: format!("Flash installer media to {disk}."),
        target: Some(disk.clone()),
    }]
}

#[derive(Default)]
struct PklRequestBuilder {
    schema_version: Option<u16>,
    operation: Option<ForgeOperation>,
    profile: Option<String>,
    hostname: Option<String>,
    arch: Option<ForgeArch>,
    output_dir: Option<PathBuf>,
    target_kind: Option<TargetKind>,
    disk: Option<String>,
    polymerize_defaults: Option<PolymerizeDefaults>,
    network: NetworkPolicy,
    safety: SafetyPolicy,
    labels: Vec<String>,
}

impl PklRequestBuilder {
    fn finish(mut self) -> Result<ForgeRequest> {
        let operation = self.operation.unwrap_or(ForgeOperation::Bundle);
        let target_kind = self
            .target_kind
            .unwrap_or_else(|| default_target_kind(operation));
        let mut request = ForgeRequest::new(
            operation,
            self.hostname.unwrap_or_else(|| "nixos".to_string()),
            self.arch.unwrap_or(ForgeArch::X86_64),
            ForgeTarget {
                kind: target_kind,
                disk: self.disk.take(),
            },
        );
        request.schema_version = self.schema_version.unwrap_or(FORGE_SCHEMA_VERSION);
        request.profile = self.profile.take();
        request.output_dir = self.output_dir.take();
        request.polymerize_defaults = self.polymerize_defaults.take().map(|mut defaults| {
            if defaults.username.is_empty() {
                defaults.username = "user".to_string();
            }
            if defaults.timezone.is_empty() {
                defaults.timezone = "UTC".to_string();
            }
            defaults
        });
        request.network = self.network;
        request.safety = self.safety;
        request.labels = self.labels;
        Ok(request)
    }
}

fn evaluate_pkl_request(path: &Path) -> Result<ForgeRequest> {
    let value = evaluate_pkl_json(path)?;
    request_from_evaluated_pkl(value)
}

fn evaluate_pkl_json(path: &Path) -> Result<serde_json::Value> {
    let output = run_pkl_eval(path)?;
    serde_json::from_slice(&output)
        .with_context(|| format!("Pkl evaluator did not emit JSON for {}", path.display()))
}

fn run_pkl_eval(path: &Path) -> Result<Vec<u8>> {
    let path_arg = path.to_string_lossy().to_string();
    let mut attempts = Vec::new();

    if let Ok(bin) = std::env::var("NEX_PKL") {
        attempts.push(PklCommand {
            program: bin,
            args: vec![
                "eval".into(),
                "--format".into(),
                "json".into(),
                path_arg.clone(),
            ],
        });
    }
    attempts.push(PklCommand {
        program: "pkl".into(),
        args: vec![
            "eval".into(),
            "--format".into(),
            "json".into(),
            path_arg.clone(),
        ],
    });
    attempts.push(PklCommand {
        program: "nix".into(),
        args: vec![
            "shell".into(),
            "nixpkgs#pkl".into(),
            "-c".into(),
            "pkl".into(),
            "eval".into(),
            "--format".into(),
            "json".into(),
            path_arg,
        ],
    });

    let mut missing = Vec::new();
    for attempt in attempts {
        let output = Command::new(&attempt.program)
            .args(&attempt.args)
            .stdin(Stdio::null())
            .output();
        match output {
            Ok(output) if output.status.success() => return Ok(output.stdout),
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                bail!(
                    "Pkl evaluator command failed: {} {}\n{}",
                    attempt.program,
                    attempt.args.join(" "),
                    stderr.trim()
                );
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                missing.push(attempt.program);
            }
            Err(err) => {
                return Err(err).with_context(|| {
                    format!(
                        "starting Pkl evaluator command: {} {}",
                        attempt.program,
                        attempt.args.join(" ")
                    )
                });
            }
        }
    }

    bail!(
        "Pkl evaluator unavailable; install `pkl`, set NEX_PKL, or provide `nix` so Nex can run nixpkgs#pkl (tried: {})",
        missing.join(", ")
    )
}

struct PklCommand {
    program: String,
    args: Vec<String>,
}

fn request_from_evaluated_pkl(value: serde_json::Value) -> Result<ForgeRequest> {
    let object = value
        .as_object()
        .context("evaluated Pkl forge definition must be an object")?;
    let mut builder = PklRequestBuilder::default();

    builder.schema_version = value_u16(object, &["schemaVersion", "schema_version"])?;
    builder.operation = value_string(object, &["operation"])
        .map(|operation| parse_operation(&operation))
        .transpose()?;
    builder.profile = value_string(object, &["profile"]);
    builder.hostname = value_string(object, &["hostname"]);
    builder.arch = value_string(object, &["arch"])
        .map(|arch| parse_arch(&arch))
        .transpose()?;
    builder.output_dir = value_string(object, &["outputDir", "output_dir"]).map(PathBuf::from);
    builder.labels = value_string_array(object, &["labels"]);

    if let Some(target) = object.get("target").and_then(|value| value.as_object()) {
        builder.target_kind = value_string(target, &["kind"])
            .map(|kind| parse_target_kind(&kind))
            .transpose()?;
        builder.disk = value_string(target, &["disk"]);
    }

    if let Some(network) = object.get("network").and_then(|value| value.as_object()) {
        if let Some(require_wired) = value_bool(network, &["requireWired", "require_wired"]) {
            builder.network.require_wired = require_wired;
        }
        if let Some(wifi_allowed) = value_bool(network, &["wifiAllowed", "wifi_allowed"]) {
            builder.network.wifi_allowed = wifi_allowed;
        }
    }

    if let Some(safety) = object.get("safety").and_then(|value| value.as_object()) {
        if let Some(allow) = value_bool(
            safety,
            &["allowDestructiveFlash", "allow_destructive_flash"],
        ) {
            builder.safety.allow_destructive_flash = allow;
        }
        if let Some(allow) = value_bool(
            safety,
            &[
                "allowInternalDiskSelection",
                "allow_internal_disk_selection",
            ],
        ) {
            builder.safety.allow_internal_disk_selection = allow;
        }
        if let Some(require) = value_bool(
            safety,
            &[
                "requireOperatorConfirmation",
                "require_operator_confirmation",
            ],
        ) {
            builder.safety.require_operator_confirmation = require;
        }
    }

    if let Some(defaults) = object
        .get("polymerizeDefaults")
        .or_else(|| object.get("polymerize_defaults"))
        .and_then(|value| value.as_object())
    {
        builder.polymerize_defaults = Some(PolymerizeDefaults {
            username: value_string(defaults, &["username"]).unwrap_or_default(),
            timezone: value_string(defaults, &["timezone"]).unwrap_or_default(),
            install_mode: value_string(defaults, &["installMode", "install_mode"]),
        });
    }

    if let Some(plan) = object.get("plan").and_then(|value| value.as_object()) {
        if let Some(mode) = value_string(plan, &["mode"]) {
            if mode == "image-build" {
                builder.operation = Some(ForgeOperation::Image);
                builder.target_kind.get_or_insert(TargetKind::Image);
            } else if builder.operation.is_none() {
                builder.operation = Some(parse_operation(&mode)?);
            }
        }
    }

    builder.finish()
}

fn value_string(
    object: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<String> {
    keys.iter()
        .find_map(|key| object.get(*key))
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}

fn value_bool(object: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<bool> {
    keys.iter()
        .find_map(|key| object.get(*key))
        .and_then(|value| value.as_bool())
}

fn value_u16(
    object: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Result<Option<u16>> {
    let Some(value) = keys.iter().find_map(|key| object.get(*key)) else {
        return Ok(None);
    };
    let Some(value) = value.as_u64() else {
        bail!("expected integer for {}", keys[0]);
    };
    Ok(Some(
        u16::try_from(value).context("schema version does not fit in u16")?,
    ))
}

fn value_string_array(
    object: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Vec<String> {
    keys.iter()
        .find_map(|key| object.get(*key))
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str())
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn default_target_kind(operation: ForgeOperation) -> TargetKind {
    match operation {
        ForgeOperation::Bundle => TargetKind::Bundle,
        ForgeOperation::UsbInstall => TargetKind::Usb,
        ForgeOperation::Image => TargetKind::Image,
        ForgeOperation::Netboot => TargetKind::Netboot,
        ForgeOperation::RemotePolymerize => TargetKind::Remote,
    }
}

fn parse_operation(value: &str) -> Result<ForgeOperation> {
    match value {
        "bundle" => Ok(ForgeOperation::Bundle),
        "usb-install" => Ok(ForgeOperation::UsbInstall),
        "image" | "image-build" => Ok(ForgeOperation::Image),
        "netboot" => Ok(ForgeOperation::Netboot),
        "remote-polymerize" => Ok(ForgeOperation::RemotePolymerize),
        other => bail!("unsupported forge operation {other}"),
    }
}

fn parse_arch(value: &str) -> Result<ForgeArch> {
    match value {
        "x86_64" | "x86" | "amd64" => Ok(ForgeArch::X86_64),
        "aarch64" | "arm64" | "arm" => Ok(ForgeArch::Aarch64),
        other => bail!("unsupported forge arch {other}"),
    }
}

fn parse_target_kind(value: &str) -> Result<TargetKind> {
    match value {
        "bundle" => Ok(TargetKind::Bundle),
        "usb" => Ok(TargetKind::Usb),
        "image" | "image-build" => Ok(TargetKind::Image),
        "netboot" => Ok(TargetKind::Netboot),
        "remote" | "remote-polymerize" => Ok(TargetKind::Remote),
        other => bail!("unsupported forge target kind {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_bundle_request_without_destructive_actions() -> Result<()> {
        let request = ForgeRequest::new(
            ForgeOperation::Bundle,
            "seed",
            ForgeArch::X86_64,
            ForgeTarget::bundle(),
        )
        .profile("/Users/wilson/workspace/pig/nex-seed-substrate")
        .output_dir("/tmp/nex-forge/seed");

        let plan = plan_request(&request)?;

        assert!(!plan.is_blocked());
        assert_eq!(plan.output_dir, PathBuf::from("/tmp/nex-forge/seed"));
        assert_eq!(
            plan.iso.as_ref().map(|iso| iso.filename.as_str()),
            Some("nixos-minimal-x86_64.iso")
        );
        assert!(plan.destructive_actions.is_empty());
        Ok(())
    }

    #[test]
    fn blocks_destructive_usb_flash_until_allowed() -> Result<()> {
        let request = ForgeRequest::new(
            ForgeOperation::UsbInstall,
            "seed",
            ForgeArch::X86_64,
            ForgeTarget::usb(Some("/dev/disk9")),
        );

        let plan = plan_request(&request)?;

        assert!(plan.is_blocked());
        assert_eq!(plan.destructive_actions.len(), 1);
        assert!(plan
            .blockers
            .iter()
            .any(|diag| diag.code == "DESTRUCTIVE_FLASH_NOT_ALLOWED"));
        Ok(())
    }

    #[test]
    fn allows_destructive_usb_flash_when_policy_allows_it() -> Result<()> {
        let mut request = ForgeRequest::new(
            ForgeOperation::UsbInstall,
            "seed",
            ForgeArch::X86_64,
            ForgeTarget::usb(Some("/dev/disk9")),
        );
        request.safety.allow_destructive_flash = true;

        let plan = plan_request(&request)?;

        assert!(!plan.is_blocked());
        assert_eq!(
            plan.destructive_actions[0].target.as_deref(),
            Some("/dev/disk9")
        );
        Ok(())
    }

    #[test]
    fn blocks_invalid_hostname() -> Result<()> {
        let request = ForgeRequest::new(
            ForgeOperation::Bundle,
            "bad host",
            ForgeArch::X86_64,
            ForgeTarget::bundle(),
        );

        let plan = plan_request(&request)?;

        assert!(plan.is_blocked());
        assert!(plan
            .blockers
            .iter()
            .any(|diag| diag.code == "INVALID_HOSTNAME"));
        Ok(())
    }

    #[test]
    fn names_pkl_as_canonical_definition_format() {
        assert_eq!(CANONICAL_DEFINITION_FORMAT, "pkl");
        assert_eq!(CANONICAL_REQUEST_EXTENSION, "pkl");
        assert_eq!(CANONICAL_MANIFEST_EXTENSION, "pkl");
    }

    #[test]
    fn serializes_request_as_json_transport_not_canonical_definition() -> Result<()> {
        let request = ForgeRequest::new(
            ForgeOperation::UsbInstall,
            "seed",
            ForgeArch::X86_64,
            ForgeTarget::usb(Some("/dev/disk9")),
        )
        .profile("/Users/wilson/workspace/pig/nex-seed-substrate");

        let encoded = serde_json::to_string(&request)?;
        let decoded: ForgeRequest = serde_json::from_str(&encoded)?;

        assert_eq!(decoded.operation, ForgeOperation::UsbInstall);
        assert_eq!(decoded.target.kind, TargetKind::Usb);
        assert_eq!(decoded.profile.as_deref(), request.profile.as_deref());
        Ok(())
    }

    #[test]
    fn normalizes_evaluated_pkl_usb_request() -> Result<()> {
        let request = request_from_evaluated_pkl(serde_json::json!({
            "schemaVersion": 1,
            "operation": "usb-install",
            "profile": "/Users/wilson/workspace/pig/nex-seed-substrate",
            "hostname": "seed",
            "arch": "x86_64",
            "outputDir": "/tmp/nex-forge/seed",
            "target": {
                "kind": "usb",
                "disk": "/dev/disk9"
            },
            "network": {
                "requireWired": true,
                "wifiAllowed": false
            },
            "safety": {
                "allowDestructiveFlash": true,
                "requireOperatorConfirmation": true
            }
        }))?;

        assert_eq!(request.operation, ForgeOperation::UsbInstall);
        assert_eq!(request.target.kind, TargetKind::Usb);
        assert_eq!(request.target.disk.as_deref(), Some("/dev/disk9"));
        assert_eq!(request.hostname, "seed");
        assert_eq!(request.arch, ForgeArch::X86_64);
        assert_eq!(
            request.profile.as_deref(),
            Some("/Users/wilson/workspace/pig/nex-seed-substrate")
        );
        assert_eq!(
            request.output_dir,
            Some(PathBuf::from("/tmp/nex-forge/seed"))
        );
        assert!(request.network.require_wired);
        assert!(!request.network.wifi_allowed);
        assert!(request.safety.allow_destructive_flash);
        Ok(())
    }

    #[test]
    fn normalizes_armory_style_template_as_image_plan() -> Result<()> {
        let request = request_from_evaluated_pkl(serde_json::json!({
            "name": "minimal-workstation",
            "description": "Build a non-destructive workstation image plan for operator review.",
            "profileClass": "desktop",
            "visibility": "public",
            "plan": {
                "mode": "image-build",
                "target": "operator-selected",
                "requiresNetwork": true
            }
        }))?;

        assert_eq!(request.operation, ForgeOperation::Image);
        assert_eq!(request.target.kind, TargetKind::Image);
        assert_eq!(request.hostname, "nixos");
        assert_eq!(request.arch, ForgeArch::X86_64);
        Ok(())
    }

    #[test]
    fn check_report_validates_armory_metadata_agreement() -> Result<()> {
        let evaluated = serde_json::json!({
            "name": "minimal-workstation",
            "profileClass": "desktop",
            "visibility": "public",
            "plan": {
                "mode": "image-build",
                "target": "operator-selected",
                "requiresNetwork": true
            }
        });
        let request = request_from_evaluated_pkl(evaluated.clone())?;
        let metadata = ForgeTemplateMetadata {
            id: Some("minimal-workstation".to_string()),
            version: Some("1.0.0".to_string()),
            canonical_format: Some("pkl".to_string()),
            visibility: Some("public".to_string()),
            profile_class: Some("desktop".to_string()),
            destructive_capabilities: Some(vec!["image-build".to_string()]),
            network_requirements: Some(vec!["package-download".to_string()]),
            ..ForgeTemplateMetadata::default()
        };

        let report = build_check_report(&evaluated, &request, Some(&metadata));

        assert!(report.valid);
        assert_eq!(report.exit_code(), 0);
        assert_eq!(report.template.id, "minimal-workstation");
        assert_eq!(report.capabilities.destructive, vec!["image-build"]);
        assert_eq!(report.capabilities.network, vec!["package-download"]);
        assert_eq!(report.inputs.len(), 1);
        assert_eq!(report.outputs[0].kind, "image-plan");
        Ok(())
    }

    #[test]
    fn check_report_rejects_public_raw_disk_template() -> Result<()> {
        let evaluated = serde_json::json!({
            "name": "unsafe-usb",
            "visibility": "public",
            "operation": "usb-install",
            "hostname": "seed",
            "target": {
                "kind": "usb",
                "disk": "/dev/sda"
            }
        });
        let request = request_from_evaluated_pkl(evaluated.clone())?;

        let report = build_check_report(&evaluated, &request, None);

        assert!(!report.valid);
        assert_eq!(report.exit_code(), 3);
        assert!(report
            .errors
            .iter()
            .any(|error| error.code == "UNSAFE_RAW_DISK_TARGET"));
        assert!(report
            .errors
            .iter()
            .any(|error| error.code == "PLAN_ONLY_EXECUTE_CAPABLE"));
        Ok(())
    }

    #[test]
    fn source_safety_catches_hidden_public_template_risks() {
        let mut errors = Vec::new();
        validate_public_template_source_safety(
            r#"
            local join_token = "secret"
            import("file:///private/site.pkl")
            local clusterInit = true
            local postInstall = "sh ./run"
            local disk = "/dev/nvme0n1"
            "#,
            &mut errors,
        );

        for code in [
            "UNSAFE_JOIN_TOKEN",
            "UNSAFE_PRIVATE_IMPORT",
            "UNSAFE_CLUSTER_INIT",
            "UNSAFE_EXECUTION_HOOK",
            "UNSAFE_RAW_DISK_TARGET",
        ] {
            assert!(errors.iter().any(|error| error.code == code), "{code}");
        }
    }

    #[test]
    fn evaluates_pkl_with_real_evaluator_when_available() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("request.pkl");
        std::fs::write(
            &path,
            r#"
            local computedHostname = "seed"

            schemaVersion = 1
            operation = "usb-install"
            hostname = computedHostname
            arch = "x86_64"

            target {
              kind = "usb"
              disk = "/dev/disk9"
            }
            "#,
        )?;

        let request = match evaluate_pkl_request(&path) {
            Ok(request) => request,
            Err(error) if error.to_string().contains("Pkl evaluator unavailable") => return Ok(()),
            Err(error) => return Err(error),
        };

        assert_eq!(request.hostname, "seed");
        assert_eq!(request.operation, ForgeOperation::UsbInstall);
        assert_eq!(request.target.disk.as_deref(), Some("/dev/disk9"));
        Ok(())
    }
}
