mod aliases;
mod cli;
mod config;
mod discover;
mod document;
mod edit;
mod exec;
pub mod forge;
pub mod input;
pub mod machine_profile;
mod materialization;
mod menu;
mod nixfile;
mod ops;
mod output;
mod pkl;
mod profile_fragment;
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
            ref action,
            ref profile,
            ref hostname,
            ref disk,
            ref output,
            ref arch,
        } => {
            if let Some(action) = action {
                return match action {
                    cli::ForgeAction::Plan { request } => ops::forge::run_plan(request),
                    cli::ForgeAction::Run { request, events } => {
                        ops::forge::run_request(request, events, cli.dry_run)
                    }
                    cli::ForgeAction::Preflight { request, json } => {
                        ops::forge::run_preflight(request, *json)
                    }
                    cli::ForgeAction::Check {
                        path,
                        metadata,
                        json,
                        no_execute,
                    } => ops::forge::run_check(path, metadata.as_deref(), *json, *no_execute),
                    cli::ForgeAction::CheckMaterialization {
                        workspace,
                        source,
                        hostname,
                        target,
                    } => ops::forge::run_check_materialization(
                        workspace.as_deref(),
                        source.as_deref(),
                        hostname,
                        target,
                    ),
                    cli::ForgeAction::BuildModule { source, name, output } => {
                        ops::forge::run_build_module(source, name, output)
                    }
                };
            }
            return ops::forge::run(
                profile.as_deref(),
                hostname.as_deref(),
                disk.as_deref(),
                output.as_deref(),
                arch.as_deref(),
                cli.dry_run,
            );
        }
        Command::Polymerize { ref bundle } => return ops::polymerize::run(bundle.as_deref()),
        Command::BuildImage {
            ref source,
            ref name,
            ref tag,
        } => return ops::build_image::run(source, name.as_deref(), tag.as_deref(), cli.dry_run),
        Command::Develop { ref flake } => return ops::develop::run(flake),
        Command::Dev { ref project } => return ops::dev::run(project),
        Command::Config { ref action } => {
            return match action {
                cli::ConfigAction::Export { format, output } => {
                    ops::config::run_export(format, output.as_deref())
                }
                cli::ConfigAction::Migrate { keep_toml } => ops::config::run_migrate(*keep_toml),
            }
        }
        Command::Rbac { ref action } => {
            return match action {
                cli::RbacAction::Sync {
                    hub_url,
                    identity,
                    token,
                    output,
                } => ops::rbac::run_sync(
                    hub_url,
                    identity.as_deref(),
                    token.as_deref(),
                    output.clone(),
                ),
            }
        }
        Command::Identity { ref action } => {
            return match action {
                cli::IdentityAction::Init { path } => ops::identity::run_init(path.clone()),
                cli::IdentityAction::Show { path } => ops::identity::run_show(path.clone()),
                cli::IdentityAction::List => ops::identity::run_list(),
                cli::IdentityAction::Ssh { label, list, add } => {
                    ops::identity::run_ssh(label.clone(), *list, add.clone())
                }
                cli::IdentityAction::Git { show } => ops::identity::run_git(*show),
                cli::IdentityAction::Wg => ops::identity::run_wg(),
                cli::IdentityAction::Age => ops::identity::run_age(),
                cli::IdentityAction::Link { url, code, path } => {
                    ops::identity::run_link(url, code.as_deref(), path.clone())
                }
            }
        }
        Command::MachineProfile { ref action } => {
            return match action {
                cli::MachineProfileAction::Validate { path } => {
                    ops::machine_profile::run_validate(path)
                }
                cli::MachineProfileAction::Inspect { path } => {
                    ops::machine_profile::run_inspect(path)
                }
            }
        }
        Command::ProfileFragment { ref action } => {
            return match action {
                cli::ProfileFragmentAction::Validate { path } => {
                    ops::profile_fragment::run_validate(path)
                }
                cli::ProfileFragmentAction::Inspect { path } => {
                    ops::profile_fragment::run_inspect(path)
                }
            }
        }
        Command::Profile { ref action } => match action {
            cli::ProfileAction::Sign { source, detached } => {
                return ops::profile::run_sign(source, *detached)
            }
            cli::ProfileAction::Verify { source } => return ops::profile::run_verify(source),
            cli::ProfileAction::Apply { .. } => {} // handled below (needs config)
        },
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
        Command::Profile { ref action } => match action {
            cli::ProfileAction::Apply { source, verify } => {
                ops::profile::run(&config, source, *verify, cli.dry_run)
            }
            // Sign and Verify are handled in the pre-config block above
            _ => unreachable!(),
        },
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
        | Command::MachineProfile { .. }
        | Command::ProfileFragment { .. }
        | Command::Config { .. }
        | Command::Identity { .. }
        | Command::Rbac { .. } => {
            unreachable!()
        }
    }
}
