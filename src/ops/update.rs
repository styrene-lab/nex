use anyhow::Result;

use crate::config::Config;
use crate::exec;
use crate::output;

pub fn run(config: &Config, dry_run: bool) -> Result<()> {
    if dry_run {
        output::dry_run("would update flake inputs and switch");
        return Ok(());
    }
    output::status("updating flake inputs...");
    exec::nix_flake_update(&config.repo)?;
    output::status("switching...");
    exec::darwin_rebuild_switch(&config.repo, &config.hostname)?;
    output::status("done");
    Ok(())
}
