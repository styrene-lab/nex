use anyhow::Result;

use crate::exec;

pub fn run(query: &str) -> Result<()> {
    exec::nix_search(query)
}
