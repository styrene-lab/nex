use anyhow::Result;

use crate::bootstrap;
use crate::config::Config;
use crate::discover::Platform;
use crate::exec;
use crate::homebrew_bootstrap;
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
    bootstrap::ensure_switch_ready(config.platform)?;
    homebrew_bootstrap::preflight(config, dry_run)?;
    let brew_missing_before =
        config.platform == Platform::Darwin && !homebrew_bootstrap::expected_brew_binary_exists();
    output::status("switching...");
    exec::system_rebuild_switch(&config.repo, &config.hostname, config.platform)?;
    output::status("done");
    if brew_missing_before && homebrew_bootstrap::expected_brew_binary_exists() {
        output::status("Homebrew was bootstrapped by nix-homebrew");
        eprintln!("  Run `nex switch` once more to activate declarative brew/cask management.");
    }
    Ok(())
}
