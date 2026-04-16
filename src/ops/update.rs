use anyhow::Result;

use crate::config::Config;
use crate::exec;
use crate::output;

pub fn run(config: &Config) -> Result<()> {
    output::status("updating flake inputs...");
    exec::nix_flake_update(&config.repo)?;
    output::status("switching...");
    exec::darwin_rebuild_switch(&config.repo, &config.hostname)?;
    output::status("done");
    Ok(())
}
