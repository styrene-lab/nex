use anyhow::{bail, Result};

use crate::armory::{self, PackageRef};
use crate::config::Config;

pub fn run(config: &Config, value: &str) -> Result<()> {
    let package_ref = PackageRef::parse(value)?;

    for registry in &config.registries {
        let index = match armory::fetch_index(registry) {
            Ok(index) => index,
            Err(error) => {
                eprintln!(
                    "  warning: could not query Armory registry {}: {error:#}",
                    registry.name
                );
                continue;
            }
        };
        if let Some(package) = armory::find(&index, &package_ref) {
            armory::print_info(registry, package);
            return Ok(());
        }
    }

    bail!("Armory package {package_ref} not found in configured registries")
}
