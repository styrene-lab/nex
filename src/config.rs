use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::discover;

/// Resolved configuration for a nex session.
pub struct Config {
    /// Path to the nix-darwin repo root.
    pub repo: PathBuf,
    /// Hostname for darwin-rebuild.
    pub hostname: String,
    /// Path to the primary nix packages file (relative to repo).
    pub nix_packages_file: PathBuf,
    /// Path to the homebrew nix file (relative to repo).
    pub homebrew_file: PathBuf,
    /// Additional nix module files with home.packages lists.
    pub module_files: Vec<(String, PathBuf)>,
}

/// Optional config file at ~/.config/nex/config.toml.
#[derive(Deserialize, Default)]
struct FileConfig {
    repo_path: Option<String>,
    hostname: Option<String>,
}

impl Config {
    /// Resolve configuration from CLI args, env vars, config file, and auto-discovery.
    pub fn resolve(cli_repo: Option<PathBuf>, cli_hostname: Option<String>) -> Result<Self> {
        let file_config = load_file_config().unwrap_or_default();

        let repo = cli_repo
            .or_else(|| file_config.repo_path.map(PathBuf::from))
            .or_else(|| discover::find_repo().ok())
            .context(
                "Could not find nix-darwin repo. Set NEX_REPO, \
                 create ~/.config/nex/config.toml, or run from within the repo.",
            )?;

        let hostname = cli_hostname
            .or(file_config.hostname)
            .or_else(|| discover::hostname().ok())
            .context("Could not detect hostname. Set NEX_HOSTNAME.")?;

        // Standard file locations within the repo
        let nix_packages_file = repo.join("nix/modules/home/base.nix");
        let homebrew_file = repo.join("nix/modules/darwin/homebrew.nix");

        // Discover additional module files with home.packages
        let mut module_files = Vec::new();
        let k8s_path = repo.join("nix/modules/home/kubernetes.nix");
        if k8s_path.exists() {
            module_files.push(("kubernetes".to_string(), k8s_path));
        }

        Ok(Config {
            repo,
            hostname,
            nix_packages_file,
            homebrew_file,
            module_files,
        })
    }

    /// All nix files that contain home.packages lists (for duplicate checking).
    pub fn all_nix_package_files(&self) -> Vec<&PathBuf> {
        let mut files = vec![&self.nix_packages_file];
        for (_, path) in &self.module_files {
            files.push(path);
        }
        files
    }
}

fn load_file_config() -> Result<FileConfig> {
    let config_dir = dirs::config_dir().context("no config directory")?;
    let config_path = config_dir.join("nex/config.toml");
    if !config_path.exists() {
        return Ok(FileConfig::default());
    }
    let content = std::fs::read_to_string(&config_path)
        .with_context(|| format!("reading {}", config_path.display()))?;
    let config: FileConfig = toml::from_str(&content)?;
    Ok(config)
}
