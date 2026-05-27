use std::path::Path;

use anyhow::{bail, Result};

pub fn run_check(path: &Path, json: bool) -> Result<()> {
    let report = crate::artifact::check_artifact_dir(path);
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
