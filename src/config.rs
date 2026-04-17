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
    /// When true, auto-pick nix for equal-version conflicts without prompting.
    pub prefer_nix_on_equal: bool,
}

/// Optional config file at ~/.config/nex/config.toml.
#[derive(Deserialize, Default)]
struct FileConfig {
    repo_path: Option<String>,
    hostname: Option<String>,
    prefer_nix_on_equal: Option<bool>,
}

impl Config {
    /// Resolve configuration from CLI args, env vars, config file, and auto-discovery.
    pub fn resolve(cli_repo: Option<PathBuf>, cli_hostname: Option<String>) -> Result<Self> {
        let file_config = load_file_config().unwrap_or_default();

        let repo = cli_repo
            .or_else(|| file_config.repo_path.map(PathBuf::from))
            .or_else(|| discover::find_repo().ok())
            .context(
                "Could not find nix-darwin repo. Run `nex init`, set NEX_REPO, \
                 or create ~/.config/nex/config.toml with repo_path.",
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

        let prefer_nix_on_equal = file_config.prefer_nix_on_equal.unwrap_or(false);

        Ok(Config {
            repo,
            hostname,
            nix_packages_file,
            homebrew_file,
            module_files,
            prefer_nix_on_equal,
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

/// Persist a key=value into the config file, preserving existing content.
pub fn set_preference(key: &str, value: &str) -> Result<()> {
    let path = config_dir()?.join("config.toml");
    let mut content = if path.exists() {
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?
    } else {
        String::new()
    };

    // Replace existing key or append
    let line = format!("{key} = {value}");
    let mut found = false;
    let updated: Vec<String> = content
        .lines()
        .map(|l| {
            if l.trim_start().starts_with(&format!("{key} "))
                || l.trim_start().starts_with(&format!("{key}="))
            {
                found = true;
                line.clone()
            } else {
                l.to_string()
            }
        })
        .collect();

    content = updated.join("\n");
    if !found {
        if !content.ends_with('\n') && !content.is_empty() {
            content.push('\n');
        }
        content.push_str(&line);
        content.push('\n');
    }

    std::fs::create_dir_all(config_dir()?)?;
    std::fs::write(&path, content)?;
    Ok(())
}

/// Canonical config directory: ~/.config/nex/
pub fn config_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("no home directory")?;
    Ok(home.join(".config/nex"))
}

fn load_file_config() -> Result<FileConfig> {
    // Primary: ~/.config/nex/config.toml (documented, discoverable)
    let primary = config_dir()?.join("config.toml");
    if primary.exists() {
        let content = std::fs::read_to_string(&primary)
            .with_context(|| format!("reading {}", primary.display()))?;
        return Ok(toml::from_str(&content)?);
    }

    // Fallback: platform config dir (~/Library/Application Support/nex/ on macOS)
    // for backwards compatibility with configs written before this fix.
    if let Some(platform_dir) = dirs::config_dir() {
        let legacy = platform_dir.join("nex/config.toml");
        if legacy.exists() {
            let content = std::fs::read_to_string(&legacy)
                .with_context(|| format!("reading {}", legacy.display()))?;
            return Ok(toml::from_str(&content)?);
        }
    }

    Ok(FileConfig::default())
}
