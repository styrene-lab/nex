use std::path::Path;

use anyhow::Result;

use crate::hardware_inventory::{
    attest_disk, match_profiles, scan_host, HardwareDisk, HardwareInventory,
    HardwareProfileMatchReport,
};

pub fn run_scan(json: bool, output: Option<&Path>) -> Result<()> {
    let inventory = scan_host()?;
    let encoded = if json || output.is_some() {
        serde_json::to_string_pretty(&inventory)?
    } else {
        render_inventory_human(&inventory)
    };

    if let Some(output) = output {
        std::fs::write(output, format!("{encoded}\n"))?;
    } else {
        println!("{encoded}");
    }
    Ok(())
}

pub fn run_attest(disk: &str, json: bool) -> Result<()> {
    let report = attest_disk(disk)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("{}", render_disk_human(&report.disk));
    }
    Ok(())
}

pub fn run_match(inventory: Option<&Path>, purpose: Option<&str>, json: bool) -> Result<()> {
    let inventory = if let Some(path) = inventory {
        let content = std::fs::read_to_string(path)?;
        serde_json::from_str::<HardwareInventory>(&content)?
    } else {
        scan_host()?
    };
    let report = match_profiles(&inventory, purpose);
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("{}", render_match_human(&report));
    }
    Ok(())
}

fn render_inventory_human(inventory: &HardwareInventory) -> String {
    let mut out = Vec::new();
    out.push("Hardware Inventory".to_string());
    out.push(format!(
        "  Platform: {:?} {}",
        inventory.platform, inventory.arch
    ));
    if let Some(vendor) = &inventory.vendor {
        out.push(format!("  Vendor: {vendor}"));
    }
    if let Some(model) = &inventory.model {
        out.push(format!("  Model: {model}"));
    }
    if let Some(model_name) = &inventory.model_name {
        out.push(format!("  Name: {model_name}"));
    }
    if let Some(cpu) = &inventory.cpu {
        out.push(format!("  CPU: {}", cpu.summary));
    }
    if let Some(memory) = &inventory.memory {
        out.push(format!("  Memory: {}", memory.summary));
    }
    if !inventory.evidence.commands.is_empty() {
        out.push("".to_string());
        out.push("Evidence commands".to_string());
        for command in &inventory.evidence.commands {
            out.push(format!("  - {command}"));
        }
    }
    out.push("".to_string());
    out.push("Disks".to_string());
    if inventory.disks.is_empty() {
        out.push("  none discovered".to_string());
    } else {
        for disk in &inventory.disks {
            out.push(format!(
                "  - {} ({}) {} {} confidence={:?}",
                disk.id,
                disk.path,
                disk.bus.as_deref().unwrap_or("unknown-bus"),
                serde_json::to_string(&disk.classification.target_attestation)
                    .unwrap_or_else(|_| "unknown".to_string())
                    .trim_matches('"'),
                disk.classification.confidence
            ));
            if let Some(size) = disk.size_bytes {
                out.push(format!("      size: {}", human_bytes(size)));
            }
            out.push(format!(
                "      destructive default: {:?}",
                disk.classification.destructive_default
            ));
            for reason in &disk.classification.reasons {
                out.push(format!("      reason: {reason}"));
            }
        }
    }
    if !inventory.warnings.is_empty() {
        out.push("".to_string());
        out.push("Warnings".to_string());
        for warning in &inventory.warnings {
            out.push(format!("  - {}: {}", warning.code, warning.message));
        }
    }
    out.join("\n")
}

fn render_disk_human(disk: &HardwareDisk) -> String {
    let mut out = Vec::new();
    out.push(format!("Disk Attestation: {} ({})", disk.id, disk.path));
    if let Some(bus) = &disk.bus {
        out.push(format!("  Bus: {bus}"));
    }
    if let Some(model) = &disk.model {
        out.push(format!("  Model: {model}"));
    }
    if let Some(size) = disk.size_bytes {
        out.push(format!("  Size: {}", human_bytes(size)));
    }
    out.push(format!(
        "  Target attestation: {}",
        serde_json::to_string(&disk.classification.target_attestation)
            .unwrap_or_else(|_| "unknown".to_string())
            .trim_matches('"')
    ));
    out.push(format!(
        "  Destructive default: {:?}",
        disk.classification.destructive_default
    ));
    out.push(format!(
        "  Confidence: {:?}",
        disk.classification.confidence
    ));
    for reason in &disk.classification.reasons {
        out.push(format!("  Reason: {reason}"));
    }
    out.join("\n")
}

fn render_match_human(report: &HardwareProfileMatchReport) -> String {
    let mut out = Vec::new();
    out.push("Hardware Profile Match".to_string());
    out.push(format!("  Hardware class: {:?}", report.hardware_class));
    if let Some(purpose) = &report.requested_purpose {
        out.push(format!("  Purpose: {purpose}"));
    }
    out.push("".to_string());
    out.push("Recommendations".to_string());
    if report.recommendations.is_empty() {
        out.push("  none".to_string());
    } else {
        for recommendation in &report.recommendations {
            out.push(format!(
                "  - {} score={} confidence={:?}",
                recommendation.profile_ref, recommendation.score, recommendation.confidence
            ));
            for reason in &recommendation.reasons {
                out.push(format!("      reason: {reason}"));
            }
            for missing in &recommendation.missing {
                out.push(format!("      missing: {missing}"));
            }
        }
    }
    out.join("\n")
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
