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
    // Initialize tracing subscriber. Controlled by NEX_LOG env var.
    // Examples: NEX_LOG=debug, NEX_LOG=nex=trace, NEX_LOG=nex::edit=debug
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("NEX_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("off")),
        )
        .with_target(true)
        .with_writer(std::io::stderr)
        .without_time()
        .init();

    let cli = Cli::parse();

    // Commands that don't need config resolution
    match cli.command {
        Command::Init { from } => return ops::init::run(from, cli.dry_run),
        Command::Relocate { ref to } => return ops::relocate::run(to.as_deref(), cli.dry_run),
        Command::Search { query } => return ops::search::run(&query),
        Command::SelfUpdate => return ops::self_update::run(),
        Command::Gc => return ops::gc::run(cli.dry_run),
        Command::Forge {
            ref profile,
            ref hostname,
            ref disk,
            ref output,
        } => {
            return ops::forge::run(
                profile.as_deref(),
                hostname.as_deref(),
                disk.as_deref(),
                output.as_deref(),
                cli.dry_run,
            )
        }
        Command::Polymerize { ref bundle } => return ops::polymerize::run(bundle.as_deref()),
        Command::BuildImage {
            ref profile,
            ref name,
            ref tag,
        } => return ops::build_image::run(profile, name.as_deref(), tag, cli.dry_run),
        Command::Develop { ref flake } => return ops::develop::run(flake),
        Command::Dev { ref project } => return ops::dev::run(project),
        Command::Identity { ref action } => {
            return match action {
                cli::IdentityAction::Init { path } => ops::identity::run_init(path.clone()),
                cli::IdentityAction::Show { path } => ops::identity::run_show(path.clone()),
                cli::IdentityAction::Link { url, code, path } => {
                    ops::identity::run_link(url, code.as_deref(), path.clone())
                }
            }
        }
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
        Command::Adopt => ops::adopt::run(&config, cli.dry_run),
        Command::List => ops::list::run(&config),
        Command::Migrate => ops::migrate::run(&config),
        Command::Profile { source } => ops::profile::run(&config, &source, cli.dry_run),
        Command::Doctor => ops::doctor::run(&config),
        Command::Switch => ops::switch::run(&config, cli.dry_run),
        Command::Update => ops::update::run(&config, cli.dry_run),
        Command::Rollback => ops::rollback::run(&config, cli.dry_run),
        Command::Try { package } => ops::try_pkg::run(&package, cli.dry_run),
        Command::Diff => ops::diff::run(&config),
        // Already handled above
        Command::Init { .. }
        | Command::Relocate { .. }
        | Command::Search { .. }
        | Command::SelfUpdate
        | Command::Gc
        | Command::Forge { .. }
        | Command::Polymerize { .. }
        | Command::BuildImage { .. }
        | Command::Develop { .. }
        | Command::Dev { .. }
        | Command::Identity { .. } => {
            unreachable!()
        }
    }
}
