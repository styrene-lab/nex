use std::path::Path;

use anyhow::{bail, Result};

pub fn run_check(path: &Path, evidence: &str, json: bool) -> Result<()> {
    let report = crate::artifact::check_artifact_dir_with_evidence(path, evidence);
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_artifact_report(&report);
    }

    if report.ok {
        Ok(())
    } else {
        bail!("artifact check failed")
    }
}

pub fn run_check_relationship(profile: &Path, payload: &Path, json: bool) -> Result<()> {
    let report = crate::artifact::check_artifact_relationship(profile, payload);
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("Artifact Relationship Check");
        println!("  Relationship: machine-profile/materialization-payload");
        println!("\nProfile");
        print_relationship_side(&report.profile);
        println!("\nPayload");
        print_relationship_side(&report.payload);
        println!(
            "\nRelationship Status: {}",
            if report.ok { "ok" } else { "failed" }
        );
        for diagnostic in &report.diagnostics {
            print_diagnostic(diagnostic);
        }
    }

    if report.ok {
        Ok(())
    } else {
        bail!("artifact relationship check failed")
    }
}

fn print_artifact_report(report: &crate::artifact::ArtifactCheckReport) {
    println!("Artifact Check");
    println!("  Path: {}", report.path);
    if let Some(kind) = report.artifact_kind {
        println!("  Kind: {}", kind.as_str());
    }
    if let Some(entrypoint) = &report.entrypoint {
        println!("  Entrypoint: {entrypoint}");
    }
    if let Some(evidence) = &report.evidence {
        println!("  Evidence: {} ({})", evidence.tier, evidence.result);
    }
    println!("  Status: {}", if report.ok { "ok" } else { "failed" });
    for diagnostic in &report.diagnostics {
        print_diagnostic(diagnostic);
    }
}

fn print_diagnostic(diagnostic: &crate::artifact::ArtifactDiagnostic) {
    if let Some(path) = &diagnostic.path {
        println!("  - {} [{}]: {}", diagnostic.code, path, diagnostic.message);
    } else {
        println!("  - {}: {}", diagnostic.code, diagnostic.message);
    }
}

fn print_relationship_side(side: &crate::artifact::ArtifactRelationshipSide) {
    if let Some(kind) = side.artifact_kind {
        println!("  Kind: {}", kind.as_str());
    }
    if let Some(id) = &side.id {
        println!("  ID: {id}");
    }
    if let Some(schema) = &side.schema {
        println!("  Schema: {schema}");
    }
    println!("  Status: {}", if side.ok { "ok" } else { "failed" });
}
