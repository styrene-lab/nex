use anyhow::Result;

use crate::config::Config;
use crate::exec;
use crate::output;

pub fn run(config: &Config) -> Result<()> {
    output::status("rolling back...");
    exec::darwin_rebuild_rollback(&config.repo, &config.hostname)?;
    output::status("done");
    Ok(())
}
