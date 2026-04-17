use anyhow::Result;

use crate::config::Config;
use crate::exec;
use crate::output;

pub fn run(config: &Config, dry_run: bool) -> Result<()> {
    if dry_run {
        output::dry_run("would rollback to previous generation");
        return Ok(());
    }
    output::status("rolling back...");
    exec::darwin_rebuild_rollback(&config.repo, &config.hostname)?;
    output::status("done");
    Ok(())
}
