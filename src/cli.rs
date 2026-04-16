use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "nex", about = "Package manager for nix-darwin + homebrew")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Path to the nix-darwin repo (overrides auto-discovery)
    #[arg(long, global = true, env = "NEX_REPO")]
    pub repo: Option<PathBuf>,

    /// Hostname for darwin-rebuild (overrides auto-detect)
    #[arg(long, global = true, env = "NEX_HOSTNAME")]
    pub hostname: Option<String>,

    /// Show what would change without editing or switching
    #[arg(long, global = true)]
    pub dry_run: bool,
}

#[derive(Subcommand)]
pub enum Command {
    /// Install packages
    Install {
        /// Force install as a Nix package (skip auto-resolution)
        #[arg(long)]
        nix: bool,
        /// Install as a Homebrew cask (GUI app)
        #[arg(long)]
        cask: bool,
        /// Install as a Homebrew formula
        #[arg(long)]
        brew: bool,
        /// Package names
        packages: Vec<String>,
    },
    /// Remove packages
    Remove {
        /// Remove a Homebrew cask
        #[arg(long)]
        cask: bool,
        /// Remove a Homebrew formula
        #[arg(long)]
        brew: bool,
        /// Package names
        packages: Vec<String>,
    },
    /// Search nixpkgs for a package
    Search {
        /// Search query
        query: String,
    },
    /// List all declared packages
    List,
    /// Update flake inputs and switch
    Update,
    /// Rebuild and activate
    Switch,
    /// Rollback to previous generation
    Rollback,
    /// Try a package in an ephemeral nix shell
    Try {
        /// Package to try
        package: String,
    },
    /// Preview what would change
    Diff,
    /// Garbage collect nix store
    Gc,
}
