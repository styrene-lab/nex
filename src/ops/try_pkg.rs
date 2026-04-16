use anyhow::Result;

use crate::exec;
use crate::output;

pub fn run(package: &str) -> Result<()> {
    output::status(&format!("opening ephemeral shell with {package}..."));
    exec::nix_shell(package)
}
