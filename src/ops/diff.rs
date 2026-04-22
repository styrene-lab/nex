use anyhow::Result;

use crate::config::Config;
use crate::exec;
use crate::output;

pub fn run(config: &Config) -> Result<()> {
    output::status("building...");
    exec::system_rebuild_build(&config.repo, &config.hostname, config.platform)?;
    output::status("diff vs current system:");
    exec::nix_diff_closures(&config.repo)
}
