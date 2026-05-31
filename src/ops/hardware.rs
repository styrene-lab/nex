use std::path::Path;

use anyhow::Result;

use crate::hardware_inventory::{attest_disk, match_profiles, scan_host, HardwareInventory};

pub fn run_scan(json: bool, output: Option<&Path>) -> Result<()> {
    let inventory = scan_host()?;
    if !json && output.is_none() {
        eprintln!("human hardware scan output is not implemented yet; emitting JSON");
    }
    let encoded = serde_json::to_string_pretty(&inventory)?;

    if let Some(output) = output {
        std::fs::write(output, format!("{encoded}\n"))?;
    } else {
        println!("{encoded}");
    }
    Ok(())
}

pub fn run_attest(disk: &str, json: bool) -> Result<()> {
    let report = attest_disk(disk)?;
    if !json {
        eprintln!("human disk attestation output is not implemented yet; emitting JSON");
    }
    println!("{}", serde_json::to_string_pretty(&report)?);
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
    if !json {
        eprintln!("human hardware profile match output is not implemented yet; emitting JSON");
    }
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
