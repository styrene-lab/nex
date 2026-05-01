use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "nex",
    about = "Package manager for nix-darwin, NixOS, and homebrew"
)]
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
    /// Move a system-owned config (e.g. /etc/nixos) into a user-writable
    /// directory so nex no longer needs sudo to read or edit it.
    Relocate {
        /// Target path. Defaults to ~/nix-config.
        #[arg(long)]
        to: Option<PathBuf>,
    },
    /// Capture all installed brew packages into the nex config
    Adopt,
    /// Install packages
    Install {
        /// Force install as a Nix package (skip auto-resolution)
        #[arg(long, conflicts_with_all = ["cask", "brew"])]
        nix: bool,
        /// Install as a Homebrew cask (GUI app)
        #[arg(long, conflicts_with_all = ["nix", "brew"])]
        cask: bool,
        /// Install as a Homebrew formula
        #[arg(long, conflicts_with_all = ["nix", "cask"])]
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
    /// Manage and apply machine profiles
    #[command(
        after_help = "Note: `nex profile <source>` was renamed to `nex profile apply <source>` in v0.16.0"
    )]
    Profile {
        #[command(subcommand)]
        action: ProfileAction,
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

        /// Target architecture (x86_64 or aarch64). Prompted interactively if omitted.
        #[arg(long)]
        arch: Option<String>,
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
    },
    /// Enter a dev shell from a flake (wraps nix develop)
    Develop {
        /// Flake reference (e.g. github:styrene-lab/nex, ., ./path/to/flake)
        #[arg(value_name = "FLAKE")]
        flake: String,
    },
    /// Open a project with omegon AI coding agent (requires omegon)
    Dev {
        /// Project flake reference or bare name (e.g. styrene-lab/nex, omegon, .)
        #[arg(value_name = "PROJECT")]
        project: String,
    },
    /// Manage Styrene identity (key generation, display)
    Identity {
        #[command(subcommand)]
        action: IdentityAction,
    },
    /// Manage RBAC roster (sync from hub)
    Rbac {
        #[command(subcommand)]
        action: RbacAction,
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

#[derive(Subcommand)]
pub enum IdentityAction {
    /// Generate a new Styrene identity
    Init {
        /// Path to the identity file (default: ~/.config/styrene/identity.key)
        #[arg(long)]
        path: Option<PathBuf>,
    },
    /// Display identity hash and derived public keys
    Show {
        /// Path to the identity file (default: ~/.config/styrene/identity.key)
        #[arg(long)]
        path: Option<PathBuf>,
    },
    /// Scan for all identities on this machine
    List,
    /// Export or manage SSH public keys derived from identity
    Ssh {
        /// Label for the SSH key (e.g. "github", "work")
        label: Option<String>,
        /// List all registered SSH key labels
        #[arg(long, conflicts_with = "label")]
        list: bool,
        /// Register a new SSH key label
        #[arg(long, conflicts_with_all = ["label", "list"])]
        add: Option<String>,
    },
    /// Configure git commit signing with identity
    Git {
        /// Show current git signing configuration
        #[arg(long)]
        show: bool,
    },
    /// Export WireGuard key pair derived from identity
    Wg,
    /// Export age encryption identity/recipient derived from identity
    Age,
    /// Link this identity to a Signum hub for SSO
    Link {
        /// Signum instance URL (e.g. https://signum.styrene.io)
        url: String,
        /// Enrollment invite code (if required by the hub)
        #[arg(long)]
        code: Option<String>,
        /// Path to the identity file (default: ~/.config/styrene/identity.key)
        #[arg(long)]
        path: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
pub enum ProfileAction {
    /// Apply a profile to this machine
    Apply {
        /// GitHub repo (user/repo), URL, or local path
        #[arg(value_name = "SOURCE")]
        source: String,
        /// Verify the profile signature before applying (local .signed.toml files only)
        #[arg(long)]
        verify: bool,
    },
    /// Sign a profile with your Styrene identity
    Sign {
        /// GitHub repo (user/repo) or local path to profile.toml
        #[arg(value_name = "SOURCE")]
        source: String,
        /// Write signature to a detached .sig file instead of embedding in [meta]
        #[arg(long)]
        detached: bool,
    },
    /// Verify a signed profile
    Verify {
        /// GitHub repo (user/repo) or local path to profile.toml
        #[arg(value_name = "SOURCE")]
        source: String,
    },
}

#[derive(Subcommand)]
pub enum RbacAction {
    /// Sync RBAC roster entries from a Signum hub
    Sync {
        /// Hub URL (e.g. https://signum.styrene.io)
        #[arg(value_name = "HUB_URL")]
        hub_url: String,
        /// Fetch only a specific identity's entry
        #[arg(long)]
        identity: Option<String>,
        /// Admin token for full roster access (single-identity fetch is public)
        #[arg(long, env = "SIGNUM_ADMIN_TOKEN")]
        token: Option<String>,
        /// Output path for the TOML config (default: ~/.config/styrene/config.toml)
        #[arg(long, short)]
        output: Option<PathBuf>,
    },
}
