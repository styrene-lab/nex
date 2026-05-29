use anyhow::Result;

use crate::config::Config;
use crate::{armory, exec};

pub fn run(config: &Config, query: &str) -> Result<()> {
    exec::nix_search(query)?;

    for registry in &config.registries {
        match armory::fetch_index(registry) {
            Ok(index) => {
                let results = armory::search(&index, query);
                armory::print_search_results(registry, &results);
            }
            Err(error) => {
                eprintln!(
                    "  warning: could not search Armory registry {}: {error:#}",
                    registry.name
                );
            }
        }
    }

    Ok(())
}
