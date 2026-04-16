use anyhow::Result;

use crate::config::Config;
use crate::edit::{self, EditSession};
use crate::exec;
use crate::nixfile;
use crate::output;

pub enum RemoveMode {
    Nix,
    Cask,
    Brew,
}

pub fn run(config: &Config, mode: RemoveMode, packages: &[String], dry_run: bool) -> Result<()> {
    if packages.is_empty() {
        anyhow::bail!("no packages specified");
    }

    let mut session = EditSession::new();
    let mut any_removed = false;

    for pkg in packages {
        match mode {
            RemoveMode::Nix => {
                // Search all nix files for the package
                let mut removed = false;
                for nix_file in config.all_nix_package_files() {
                    if edit::contains(nix_file, &nixfile::NIX_PACKAGES, pkg)? {
                        if dry_run {
                            output::dry_run(&format!(
                                "would remove {pkg} from {}",
                                nix_file.display()
                            ));
                            removed = true;
                            break;
                        }
                        session.backup(nix_file)?;
                        if edit::remove(nix_file, &nixfile::NIX_PACKAGES, pkg)? {
                            output::removed(pkg);
                            any_removed = true;
                            removed = true;
                            break;
                        }
                    }
                }
                if !removed {
                    output::not_found(pkg, "not in any nix package list");
                }
            }
            RemoveMode::Cask => {
                if dry_run {
                    if edit::contains(&config.homebrew_file, &nixfile::HOMEBREW_CASKS, pkg)? {
                        output::dry_run(&format!("would remove cask {pkg}"));
                    } else {
                        output::not_found(pkg, "not in casks");
                    }
                    continue;
                }

                session.backup(&config.homebrew_file)?;
                if edit::remove(&config.homebrew_file, &nixfile::HOMEBREW_CASKS, pkg)? {
                    output::removed(pkg);
                    any_removed = true;
                } else {
                    output::not_found(pkg, "not in casks");
                }
            }
            RemoveMode::Brew => {
                if dry_run {
                    if edit::contains(&config.homebrew_file, &nixfile::HOMEBREW_BREWS, pkg)? {
                        output::dry_run(&format!("would remove brew {pkg}"));
                    } else {
                        output::not_found(pkg, "not in brews");
                    }
                    continue;
                }

                session.backup(&config.homebrew_file)?;
                if edit::remove(&config.homebrew_file, &nixfile::HOMEBREW_BREWS, pkg)? {
                    output::removed(pkg);
                    any_removed = true;
                } else {
                    output::not_found(pkg, "not in brews");
                }
            }
        }
    }

    if dry_run || !any_removed {
        return Ok(());
    }

    output::status("switching...");
    match exec::darwin_rebuild_switch(&config.repo, &config.hostname) {
        Ok(()) => {
            session.commit_all()?;
            output::status("done");
            Ok(())
        }
        Err(e) => {
            output::error("switch failed, reverting changes");
            session.revert_all()?;
            Err(e)
        }
    }
}
