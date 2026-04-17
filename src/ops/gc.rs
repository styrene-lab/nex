use anyhow::Result;

use crate::exec;
use crate::output;

pub fn run(dry_run: bool) -> Result<()> {
    if dry_run {
        output::dry_run("would garbage collect nix store");
        return Ok(());
    }
    output::status("garbage collecting...");
    exec::nix_gc()?;
    output::status("done");
    Ok(())
}
