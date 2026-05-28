use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::machine_profile::{MachineProfileDocument, MACHINE_PROFILE_SCHEMA_V1};
use crate::materialization::MaterializationPayload;

pub const MATERIALIZATION_PAYLOAD_SCHEMA_V1: &str = "io.styrene.nex.materialization-payload.v1";
pub const MACHINE_PROFILE_ARTIFACT_TYPE_V1: &str = "application/vnd.styrene.nex.machine-profile.v1+tar";
pub const MATERIALIZATION_PAYLOAD_ARTIFACT_TYPE_V1: &str =
    "application/vnd.styrene.nex.materialization-payload.v1+tar";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactKind {
    MachineProfile,
    MaterializationPayload,
}

impl ArtifactKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MachineProfile => "machine-profile",
            Self::MaterializationPayload => "materialization-payload",
        }
    }

    pub fn entrypoint(self) -> &'static str {
        match self {
            Self::MachineProfile => "machine-profile.pkl",
            Self::MaterializationPayload => "payload.pkl",
        }
    }

    pub fn schema(self) -> &'static str {
        match self {
            Self::MachineProfile => MACHINE_PROFILE_SCHEMA_V1,
            Self::MaterializationPayload => MATERIALIZATION_PAYLOAD_SCHEMA_V1,
        }
    }

    pub fn artifact_type(self) -> &'static str {
        match self {
            Self::MachineProfile => MACHINE_PROFILE_ARTIFACT_TYPE_V1,
            Self::MaterializationPayload => MATERIALIZATION_PAYLOAD_ARTIFACT_TYPE_V1,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ArtifactCheckReport {
    pub ok: bool,
    pub path: String,
    pub artifact_kind: Option<ArtifactKind>,
    pub id: Option<String>,
    pub schema: Option<String>,
    pub version: Option<String>,
    pub entrypoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<EvidenceRecord>,
    pub diagnostics: Vec<ArtifactDiagnostic>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvidenceRecord {
    pub tier: String,
    pub result: String,
    pub validated_with: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArtifactRelationshipReport {
    pub ok: bool,
    pub profile: ArtifactRelationshipSide,
    pub payload: ArtifactRelationshipSide,
    pub compatibility: ArtifactCompatibility,
    pub diagnostics: Vec<ArtifactDiagnostic>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArtifactRelationshipSide {
    pub id: Option<String>,
    pub schema: Option<String>,
    pub artifact_kind: Option<ArtifactKind>,
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArtifactCompatibility {
    pub systems: Vec<String>,
    pub targets: Vec<String>,
    pub build_targets: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArtifactDiagnostic {
    pub severity: DiagnosticSeverity,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiagnosticSeverity {
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceTier {
    Evaluates,
    Materializes,
    BuildsImage,
    BootsEmulated,
    BootsHardware,
    Operational,
}

impl EvidenceTier {
    pub fn parse(value: &str) -> std::result::Result<Self, ArtifactDiagnostic> {
        match value {
            "evaluates" => Ok(Self::Evaluates),
            "materializes" => Ok(Self::Materializes),
            "builds-image" => Ok(Self::BuildsImage),
            "boots-emulated" => Ok(Self::BootsEmulated),
            "boots-hardware" => Ok(Self::BootsHardware),
            "operational" => Ok(Self::Operational),
            other => Err(ArtifactDiagnostic::error(
                "unknown-evidence-tier",
                format!("unknown evidence tier '{other}'"),
                Some("evidence".to_string()),
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Evaluates => "evaluates",
            Self::Materializes => "materializes",
            Self::BuildsImage => "builds-image",
            Self::BootsEmulated => "boots-emulated",
            Self::BootsHardware => "boots-hardware",
            Self::Operational => "operational",
        }
    }
}

impl ArtifactDiagnostic {
    fn error(code: impl Into<String>, message: impl Into<String>, path: Option<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::Error,
            code: code.into(),
            message: message.into(),
            path,
        }
    }
}

#[derive(Debug)]
struct DetectedArtifact {
    kind: ArtifactKind,
    entrypoint: PathBuf,
}

pub fn check_artifact_dir(path: &Path) -> ArtifactCheckReport {
    check_artifact_dir_with_evidence(path, "evaluates")
}

pub fn check_artifact_relationship(profile: &Path, payload: &Path) -> ArtifactRelationshipReport {
    let profile_report = check_artifact_dir(profile);
    let payload_report = check_artifact_dir(payload);
    let mut diagnostics = Vec::new();

    if profile_report.artifact_kind != Some(ArtifactKind::MachineProfile) {
        diagnostics.push(ArtifactDiagnostic::error(
            "relationship-profile-kind-invalid",
            "relationship profile path must be a machine-profile artifact",
            Some("profile".to_string()),
        ));
    }
    if payload_report.artifact_kind != Some(ArtifactKind::MaterializationPayload) {
        diagnostics.push(ArtifactDiagnostic::error(
            "relationship-payload-kind-invalid",
            "relationship payload path must be a materialization-payload artifact",
            Some("payload".to_string()),
        ));
    }
    if !profile_report.ok {
        diagnostics.push(ArtifactDiagnostic::error(
            "relationship-profile-invalid",
            "profile artifact failed standalone validation",
            Some("profile".to_string()),
        ));
    }
    if !payload_report.ok {
        diagnostics.push(ArtifactDiagnostic::error(
            "relationship-payload-invalid",
            "payload artifact failed standalone validation",
            Some("payload".to_string()),
        ));
    }

    ArtifactRelationshipReport {
        ok: profile_report.ok && payload_report.ok && diagnostics.is_empty(),
        profile: relationship_side(&profile_report),
        payload: relationship_side(&payload_report),
        compatibility: compatibility_summary(&profile_report, &payload_report),
        diagnostics,
    }
}

pub fn check_artifact_dir_with_evidence(path: &Path, evidence: &str) -> ArtifactCheckReport {
    let mut diagnostics = Vec::new();
    let tier = match EvidenceTier::parse(evidence) {
        Ok(tier) => tier,
        Err(diagnostic) => {
            diagnostics.push(diagnostic);
            return ArtifactCheckReport {
                ok: false,
                path: path.display().to_string(),
                artifact_kind: None,
                id: None,
                schema: None,
                version: None,
                entrypoint: None,
                evidence: Some(evidence_record(evidence, "failed")),
                diagnostics,
            };
        }
    };
    let detected = match detect_artifact(path) {
        Ok(detected) => detected,
        Err(diagnostic) => {
            diagnostics.push(diagnostic);
            return ArtifactCheckReport {
                ok: false,
                path: path.display().to_string(),
                artifact_kind: None,
                id: None,
                schema: None,
                version: None,
                entrypoint: None,
                evidence: Some(evidence_record(tier.as_str(), "failed")),
                diagnostics,
            };
        }
    };

    let json = match crate::pkl::evaluate_json(&detected.entrypoint) {
        Ok(json) => json,
        Err(error) => {
            diagnostics.push(ArtifactDiagnostic::error(
                "pkl-evaluation-failed",
                format!("failed to evaluate {}: {error:#}", detected.entrypoint.display()),
                None,
            ));
            return report(path, &detected, tier, diagnostics);
        }
    };

    diagnostics.extend(validate_boundary_fields(detected.kind, &json));
    if diagnostics.is_empty() {
        diagnostics.extend(validate_typed_semantics(detected.kind, json));
    }
    diagnostics.extend(validate_armory_metadata(path, detected.kind));
    diagnostics.extend(validate_evidence_tier(tier));

    report(path, &detected, tier, diagnostics)
}

fn report(
    path: &Path,
    detected: &DetectedArtifact,
    tier: EvidenceTier,
    diagnostics: Vec<ArtifactDiagnostic>,
) -> ArtifactCheckReport {
    ArtifactCheckReport {
        ok: diagnostics.is_empty(),
        path: path.display().to_string(),
        artifact_kind: Some(detected.kind),
        id: artifact_id(detected.kind, path),
        schema: Some(detected.kind.schema().to_string()),
        version: None,
        entrypoint: Some(detected.kind.entrypoint().to_string()),
        evidence: Some(evidence_record(
            tier.as_str(),
            if diagnostics.is_empty() {
                "passed"
            } else {
                "failed"
            },
        )),
        diagnostics,
    }
}

fn relationship_side(report: &ArtifactCheckReport) -> ArtifactRelationshipSide {
    ArtifactRelationshipSide {
        id: report.id.clone(),
        schema: report.schema.clone(),
        artifact_kind: report.artifact_kind,
        ok: report.ok,
    }
}

fn compatibility_summary(
    profile: &ArtifactCheckReport,
    payload: &ArtifactCheckReport,
) -> ArtifactCompatibility {
    ArtifactCompatibility {
        systems: Vec::new(),
        targets: if profile.artifact_kind == Some(ArtifactKind::MachineProfile) {
            Vec::new()
        } else {
            Vec::new()
        },
        build_targets: if payload.artifact_kind == Some(ArtifactKind::MaterializationPayload) {
            Vec::new()
        } else {
            Vec::new()
        },
    }
}

fn artifact_id(kind: ArtifactKind, path: &Path) -> Option<String> {
    match kind {
        ArtifactKind::MachineProfile | ArtifactKind::MaterializationPayload => path
            .file_name()
            .and_then(|name| name.to_str())
            .map(ToString::to_string),
    }
}

fn evidence_record(tier: &str, result: &str) -> EvidenceRecord {
    EvidenceRecord {
        tier: tier.to_string(),
        result: result.to_string(),
        validated_with: format!("nex {}", env!("CARGO_PKG_VERSION")),
    }
}

fn validate_evidence_tier(tier: EvidenceTier) -> Vec<ArtifactDiagnostic> {
    if tier == EvidenceTier::Evaluates {
        return Vec::new();
    }
    vec![ArtifactDiagnostic::error(
        "unsupported-evidence-tier",
        format!(
            "evidence tier '{}' is recognized but does not have a validator yet",
            tier.as_str()
        ),
        Some("evidence".to_string()),
    )]
}

fn detect_artifact(path: &Path) -> std::result::Result<DetectedArtifact, ArtifactDiagnostic> {
    if !path.is_dir() {
        return Err(ArtifactDiagnostic::error(
            "artifact-path-not-directory",
            format!("artifact path must be a directory: {}", path.display()),
            None,
        ));
    }

    let machine_profile = path.join(ArtifactKind::MachineProfile.entrypoint());
    let materialization_payload = path.join(ArtifactKind::MaterializationPayload.entrypoint());
    match (machine_profile.is_file(), materialization_payload.is_file()) {
        (true, false) => Ok(DetectedArtifact {
            kind: ArtifactKind::MachineProfile,
            entrypoint: machine_profile,
        }),
        (false, true) => Ok(DetectedArtifact {
            kind: ArtifactKind::MaterializationPayload,
            entrypoint: materialization_payload,
        }),
        (true, true) => Err(ArtifactDiagnostic::error(
            "ambiguous-artifact-kind",
            "artifact directory contains both machine-profile.pkl and payload.pkl",
            None,
        )),
        (false, false) => Err(ArtifactDiagnostic::error(
            "missing-artifact-entrypoint",
            "artifact directory must contain machine-profile.pkl or payload.pkl",
            None,
        )),
    }
}

fn validate_boundary_fields(kind: ArtifactKind, json: &Value) -> Vec<ArtifactDiagnostic> {
    let Some(object) = json.as_object() else {
        return vec![ArtifactDiagnostic::error(
            "artifact-root-not-object",
            "evaluated artifact document must be an object",
            None,
        )];
    };

    let forbidden: &[(&str, &str)] = match kind {
        ArtifactKind::MachineProfile => &[
            (
                "flake_inputs",
                "machine-profile artifacts must not declare materialization flake inputs",
            ),
            (
                "nixos_module",
                "machine-profile artifacts must not declare NixOS module material",
            ),
        ],
        ArtifactKind::MaterializationPayload => &[
            (
                "machine_profile",
                "materialization-payload artifacts must not declare machine-profile policy",
            ),
            (
                "safety",
                "materialization-payload artifacts must not declare machine safety policy",
            ),
            (
                "allowed_targets",
                "materialization-payload artifacts must not declare machine target policy",
            ),
            (
                "requires_confirmation",
                "materialization-payload artifacts must not declare confirmation policy",
            ),
            (
                "requires_target_attestation",
                "materialization-payload artifacts must not declare target attestation policy",
            ),
            (
                "default_destructive",
                "materialization-payload artifacts must not declare destructive-operation policy",
            ),
        ],
    };

    forbidden
        .iter()
        .filter(|(field, _)| object.contains_key(*field))
        .map(|(field, message)| {
            ArtifactDiagnostic::error(
                "forbidden-boundary-field",
                *message,
                Some((*field).to_string()),
            )
        })
        .collect()
}

fn validate_typed_semantics(kind: ArtifactKind, json: Value) -> Vec<ArtifactDiagnostic> {
    let result: Result<()> = match kind {
        ArtifactKind::MachineProfile => serde_json::from_value::<MachineProfileDocument>(json)
            .context("decoding machine profile")
            .and_then(|document| document.validate()),
        ArtifactKind::MaterializationPayload => serde_json::from_value::<MaterializationPayload>(json)
            .context("decoding materialization payload")
            .and_then(|payload| payload.validate()),
    };

    match result {
        Ok(()) => Vec::new(),
        Err(error) => vec![ArtifactDiagnostic::error(
            "semantic-validation-failed",
            format!("{error:#}"),
            None,
        )],
    }
}

fn validate_armory_metadata(path: &Path, kind: ArtifactKind) -> Vec<ArtifactDiagnostic> {
    let metadata_path = path.join("armory.toml");
    if !metadata_path.is_file() {
        return Vec::new();
    }

    let metadata = match std::fs::read_to_string(&metadata_path)
        .with_context(|| format!("reading {}", metadata_path.display()))
        .and_then(|content| toml::from_str::<ArmoryMetadata>(&content).context("parsing armory.toml"))
    {
        Ok(metadata) => metadata,
        Err(error) => {
            return vec![ArtifactDiagnostic::error(
                "armory-metadata-invalid",
                format!("{error:#}"),
                None,
            )]
        }
    };

    let mut diagnostics = Vec::new();
    check_metadata_field(
        &mut diagnostics,
        "artifact.kind",
        metadata.artifact.kind.as_deref(),
        kind.as_str(),
    );
    check_metadata_field(
        &mut diagnostics,
        "artifact.source",
        metadata.artifact.source.as_deref(),
        kind.entrypoint(),
    );
    check_metadata_field(
        &mut diagnostics,
        "artifact.schema",
        metadata.artifact.schema.as_deref(),
        kind.schema(),
    );
    check_metadata_field(
        &mut diagnostics,
        "artifact.artifact_type",
        metadata.artifact.artifact_type.as_deref(),
        kind.artifact_type(),
    );
    diagnostics
}

fn check_metadata_field(
    diagnostics: &mut Vec<ArtifactDiagnostic>,
    field: &str,
    actual: Option<&str>,
    expected: &str,
) {
    match actual {
        Some(actual) if actual == expected => {}
        Some(actual) => diagnostics.push(ArtifactDiagnostic::error(
            "armory-metadata-mismatch",
            format!("{field} must be '{expected}', found '{actual}'"),
            Some(field.to_string()),
        )),
        None => diagnostics.push(ArtifactDiagnostic::error(
            "armory-metadata-missing-field",
            format!("{field} is required when armory.toml is present"),
            Some(field.to_string()),
        )),
    }
}

#[derive(Debug, Deserialize)]
struct ArmoryMetadata {
    artifact: ArmoryArtifactMetadata,
}

#[derive(Debug, Deserialize)]
struct ArmoryArtifactMetadata {
    kind: Option<String>,
    source: Option<String>,
    schema: Option<String>,
    artifact_type: Option<String>,
}
