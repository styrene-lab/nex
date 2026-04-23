use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "nex", about = "Package manager for nix-darwin, NixOS, and homebrew")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Path to the nix-darwin repo (overrides auto-discovery)
    #[arg(long, global = true, env = "NEX_REPO")]
    pub repo: Option<PathBuf>,

    /// Hostname for system rebuild (overrides auto-detect)
    #[arg(long, global = true, env = "NEX_HOSTNAME")]
    pub hostname: Option<String>,

    /// Show what would change without editing or switching
    #[arg(long, global = true)]
    pub dry_run: bool,
}

#[derive(Subcommand)]
pub enum Command {
    /// Set up nix-darwin + homebrew (macOS) or NixOS (Linux) on this machine
    Init {
        /// Clone an existing nix-darwin repo instead of scaffolding
        #[arg(long)]
        from: Option<String>,
    },
    /// Capture all installed brew packages into the nex config
    Adopt,
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
    /// Identify brew packages that can migrate to nix
    Migrate,
    /// Apply a profile from a GitHub repo
    Profile {
        /// GitHub repo (user/repo) or URL
        #[arg(value_name = "SOURCE")]
        source: String,
    },
    /// Build a bootable NixOS installer USB, optionally with a baked-in profile
    Forge {
        /// Nex profile (GitHub user/repo) to bake in. Omit for generic styx installer.
        #[arg(value_name = "PROFILE")]
        profile: Option<String>,

        /// Hostname default for the target system (user can override at install time)
        #[arg(long)]
        hostname: Option<String>,

        /// Target USB device to flash (e.g. /dev/sdb, /dev/disk4). If omitted, builds bundle only.
        #[arg(long)]
        disk: Option<String>,

        /// Output directory for the bundle (default: /tmp/nex-forge)
        #[arg(long, short)]
        output: Option<PathBuf>,
    },
    /// Interactive NixOS installer (runs on target machine after booting from USB)
    Polymerize {
        /// Path to the bundled defaults (written by nex forge). Auto-detected from USB.
        #[arg(long)]
        bundle: Option<PathBuf>,
    },
    /// Build an OCI container image from a profile
    BuildImage {
        /// Nex profile (GitHub user/repo) or local path to profile.toml
        #[arg(value_name = "PROFILE")]
        profile: String,

        /// Image name (default: derived from profile name)
        #[arg(long)]
        name: Option<String>,

        /// Image tag (default: "latest")
        #[arg(long, default_value = "latest")]
        tag: String,

        /// Output format: "docker" (tarball) or "oci" (OCI layout)
        #[arg(long, default_value = "docker")]
        format: String,
    },
    /// Enter a dev shell from a flake (wraps nix develop)
    Develop {
        /// Flake reference (e.g. github:styrene-lab/nex, ., ./path/to/flake)
        #[arg(value_name = "FLAKE")]
        flake: String,
    },
    /// Check and fix common configuration issues
    Doctor,
    /// Update nex itself to the latest release
    SelfUpdate,
    /// Preview what would change
    Diff,
    /// Garbage collect nix store
    Gc,
}
