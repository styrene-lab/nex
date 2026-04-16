use anyhow::Result;

use crate::exec;
use crate::output;

pub fn run() -> Result<()> {
    output::status("garbage collecting...");
    exec::nix_gc()?;
    output::status("done");
    Ok(())
}
