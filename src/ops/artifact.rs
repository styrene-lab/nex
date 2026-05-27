use std::path::Path;

use anyhow::{bail, Result};

pub fn run_check(path: &Path, evidence: &str, json: bool) -> Result<()> {
    let report = crate::artifact::check_artifact_dir_with_evidence(path, evidence);
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("Artifact Check");
        println!("  Path: {}", report.path);
        if let Some(kind) = report.kind {
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
            if let Some(field) = &diagnostic.field {
                println!("  - {} [{}]: {}", diagnostic.code, field, diagnostic.message);
            } else {
                println!("  - {}: {}", diagnostic.code, diagnostic.message);
            }
        }
    }

    if report.ok {
        Ok(())
    } else {
        bail!("artifact check failed")
    }
}
