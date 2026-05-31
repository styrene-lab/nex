use std::path::Path;

use anyhow::Result;

use crate::devenv_import::{inspect_devenv_project, plan_devenv_migration};
use crate::devenv_surface::load_devenv_surface_catalog;

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

pub fn run_plan(path: &Path, json: bool) -> Result<()> {
    let plan = plan_devenv_migration(path)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&plan)?);
        return Ok(());
    }
    println!("devenv migration plan: {}", plan.root.display());
    println!(
        "  ready: {}  blocked: {}  portable: {}  machine candidates: {}  review: {}",
        plan.actions.len(),
        plan.blocked.len(),
        plan.summary.portable,
        plan.summary.machine_scoped_candidate,
        plan.summary.requires_review
    );
    if !plan.actions.is_empty() {
        println!("ready actions:");
        for action in &plan.actions {
            println!(
                "  - {:?}: {} -> {}",
                action.action, action.id, action.target
            );
        }
    }
    if !plan.blocked.is_empty() {
        println!("blocked pending review:");
        for action in &plan.blocked {
            let reason = action
                .blockers
                .first()
                .map(|b| b.message.as_str())
                .unwrap_or("operator review required");
            println!("  - {:?}: {} ({reason})", action.action, action.id);
        }
    }
    Ok(())
}

pub fn run_catalog_list(json: bool) -> Result<()> {
    let report = load_devenv_surface_catalog()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    println!("devenv surface catalog");
    println!(
        "  upstream: {} @ {}",
        report.upstream.repo, report.upstream.rev
    );
    println!("  mapping reviewed: {}", report.mapping.reviewed_at);
    println!("  mappings: {}", report.mapping.mappings.len());
    println!(
        "  devenv.yaml top-level keys: {}",
        report.yaml_top_level_properties.join(", ")
    );
    if let Some((pattern, mapping)) =
        crate::devenv_surface::find_mapping(&report.mapping, "languages.rust.enable")
    {
        println!(
            "  example: languages.rust.enable matches {pattern} -> {}",
            mapping.target
        );
    }
    for (pattern, mapping) in &report.mapping.mappings {
        println!(
            "  - {pattern}: {} -> {} ({})",
            mapping.kind, mapping.target, mapping.bucket
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
