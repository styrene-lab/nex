use anyhow::Result;

use crate::config::Config;
use crate::exec;
use crate::output;

pub fn run(config: &Config, dry_run: bool) -> Result<()> {
    if dry_run {
        output::dry_run("would run darwin-rebuild switch");
        return Ok(());
    }
    output::status("switching...");
    exec::darwin_rebuild_switch(&config.repo, &config.hostname)?;
    output::status("done");
    Ok(())
}
