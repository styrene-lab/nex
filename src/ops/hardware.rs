use std::path::Path;

use anyhow::Result;

use crate::hardware_inventory::scan_host;

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
