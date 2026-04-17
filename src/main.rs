mod aliases;
mod cli;
mod config;
mod discover;
mod edit;
mod exec;
mod nixfile;
mod ops;
mod output;
mod resolve;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Command};
use config::Config;

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Commands that don't need config resolution
    match cli.command {
        Command::Init { from } => return ops::init::run(from, cli.dry_run),
        Command::Search { query } => return ops::search::run(&query),
        Command::SelfUpdate => return ops::self_update::run(),
        Command::Gc => return ops::gc::run(),
        _ => {}
    }

    let config = Config::resolve(cli.repo.clone(), cli.hostname.clone())?;

    match cli.command {
        Command::Install {
            nix,
            cask,
            brew,
            packages,
        } => {
            let mode = if nix {
                ops::install::InstallMode::Nix
            } else if cask {
                ops::install::InstallMode::Cask
            } else if brew {
                ops::install::InstallMode::Brew
            } else {
                ops::install::InstallMode::Auto
            };
            ops::install::run(&config, mode, &packages, cli.dry_run)
        }
        Command::Remove {
            cask,
            brew,
            packages,
        } => {
            let mode = if cask {
                ops::remove::RemoveMode::Cask
            } else if brew {
                ops::remove::RemoveMode::Brew
            } else {
                ops::remove::RemoveMode::Auto
            };
            ops::remove::run(&config, mode, &packages, cli.dry_run)
        }
        Command::List => ops::list::run(&config),
        Command::Migrate => ops::migrate::run(&config),
        Command::Switch => ops::switch::run(&config),
        Command::Update => ops::update::run(&config),
        Command::Rollback => ops::rollback::run(&config),
        Command::Try { package } => ops::try_pkg::run(&package),
        Command::Diff => ops::diff::run(&config),
        // Already handled above
        Command::Init { .. } | Command::Search { .. } | Command::SelfUpdate | Command::Gc => {
            unreachable!()
        }
    }
}
