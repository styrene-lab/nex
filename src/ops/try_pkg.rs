use anyhow::Result;

use crate::exec;
use crate::output;

pub fn run(package: &str, dry_run: bool) -> Result<()> {
    if dry_run {
        output::dry_run(&format!("would open ephemeral shell with {package}"));
        return Ok(());
    }
    output::status(&format!("opening ephemeral shell with {package}..."));
    exec::nix_shell(package)
}
