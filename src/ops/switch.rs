use anyhow::Result;

use crate::config::Config;
use crate::discover::Platform;
use crate::exec;
use crate::output;

pub fn run(config: &Config, dry_run: bool) -> Result<()> {
    let label = match config.platform {
        Platform::Darwin => "darwin-rebuild switch",
        Platform::Linux => "nixos-rebuild switch",
    };
    if dry_run {
        output::dry_run(&format!("would run {label}"));
        return Ok(());
    }
    output::status("switching...");
    exec::system_rebuild_switch(&config.repo, &config.hostname, config.platform)?;
    output::status("done");
    Ok(())
}
