use std::path::Path;

use anyhow::Result;

use crate::devenv_import::inspect_devenv_project;

pub fn run_inspect(path: &Path, json: bool) -> Result<()> {
    let report = inspect_devenv_project(path)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    println!("devenv project: {}", report.root.display());
    println!(
        "  portable: {}  project: {}  machine candidates: {}  review: {}  unsupported: {}",
        report.summary.portable,
        report.summary.project_scoped,
        report.summary.machine_scoped_candidate,
        report.summary.requires_review,
        report.summary.unsupported,
    );
    for item in &report.items {
        let target = item
            .nex_candidate
            .as_ref()
            .map(|candidate| candidate.target.as_str())
            .unwrap_or("manual-review");
        println!(
            "  - {:?} {:?}: {} -> {}",
            item.kind, item.bucket, item.devenv.option, target
        );
    }
    Ok(())
}

pub fn run_explain(path: &Path, json: bool) -> Result<()> {
    let report = inspect_devenv_project(path)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    println!("Nex can treat this devenv project as an import source, not as the source of truth.");
    println!("Portable items become profile fragments or outputs; machine-scoped items require Nex safety review.");
    println!();
    run_inspect(path, false)
}
