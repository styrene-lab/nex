use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::discover::{self, Platform};

/// Resolved configuration for a nex session.
pub struct Config {
    /// Path to the nix config repo root (nix-darwin or NixOS).
    pub repo: PathBuf,
    /// Hostname for system rebuild.
    pub hostname: String,
    /// Path to the primary nix packages file (relative to repo).
    pub nix_packages_file: PathBuf,
    /// Path to the homebrew nix file (relative to repo). None on Linux.
    pub homebrew_file: PathBuf,
    /// Additional nix module files with home.packages lists.
    pub module_files: Vec<(String, PathBuf)>,
    /// When true, auto-pick nix for equal-version conflicts without prompting.
    pub prefer_nix_on_equal: bool,
    /// The detected platform (Darwin or Linux).
    pub platform: Platform,
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
        tracing::debug!(cli_repo = ?cli_repo, cli_hostname = ?cli_hostname, "resolving config");
        let file_config = load_file_config().unwrap_or_default();
        let platform = discover::detect_platform();

        let not_found_msg = match platform {
            Platform::Darwin => {
                "Could not find nix-darwin repo. Run `nex init`, set NEX_REPO, \
                 or create ~/.config/nex/config.toml with repo_path."
            }
            Platform::Linux => {
                "Could not find NixOS config repo. Run `nex init`, set NEX_REPO, \
                 or create ~/.config/nex/config.toml with repo_path."
            }
        };

        let repo = cli_repo
            .or_else(|| file_config.repo_path.map(PathBuf::from))
            .or_else(|| discover::find_repo().ok())
            .context(not_found_msg)?;

        tracing::debug!(repo = %repo.display(), "config repo resolved");

        let hostname = cli_hostname
            .or(file_config.hostname)
            .or_else(|| discover::hostname().ok())
            .context("Could not detect hostname. Set NEX_HOSTNAME.")?;

        tracing::debug!(%hostname, "hostname resolved");

        // Validate hostname: alphanumeric and hyphens only, no leading/trailing hyphen
        if hostname.is_empty()
            || hostname.starts_with('-')
            || hostname.ends_with('-')
            || !hostname
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            anyhow::bail!(
                "invalid hostname \"{hostname}\": must be alphanumeric with hyphens, \
                 no leading/trailing hyphen"
            );
        }

        // Standard file locations — detect scaffolded vs flat layout
        let scaffolded = repo.join("nix/modules/home").exists();

        let nix_packages_file = if scaffolded {
            repo.join("nix/modules/home/base.nix")
        } else if repo.join("home.nix").exists() {
            repo.join("home.nix")
        } else {
            repo.join("nix/modules/home/base.nix") // fallback
        };

        let homebrew_file = match platform {
            Platform::Darwin => repo.join("nix/modules/darwin/homebrew.nix"),
            Platform::Linux => {
                if scaffolded {
                    repo.join("nix/modules/nixos/packages.nix")
                } else {
                    repo.join("configuration.nix")
                }
            }
        };

        // Discover additional module files with home.packages
        let mut module_files = Vec::new();
        let home_modules_dir = repo.join("nix/modules/home");
        if home_modules_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&home_modules_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) != Some("nix") {
                        continue;
                    }
                    let stem = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_string();
                    // Skip the primary packages file and module entry points
                    if stem == "base" || stem == "default" {
                        continue;
                    }
                    tracing::debug!(module = %stem, path = %path.display(), "discovered module");
                    module_files.push((stem, path));
                }
            }
            module_files.sort_by(|a, b| a.0.cmp(&b.0));
        }

        let prefer_nix_on_equal = file_config.prefer_nix_on_equal.unwrap_or(false);

        Ok(Config {
            repo,
            hostname,
            nix_packages_file,
            homebrew_file,
            module_files,
            prefer_nix_on_equal,
            platform,
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
    tracing::debug!(%key, %value, "setting preference");
    let path = config_dir()?.join("config.toml");
    let content = if path.exists() {
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?
    } else {
        String::new()
    };

    // Parse existing config as a TOML table
    let mut table: toml::map::Map<String, toml::Value> = if content.is_empty() {
        toml::map::Map::new()
    } else {
        toml::from_str(&content).with_context(|| format!("parsing {}", path.display()))?
    };

    // Parse the value string as a TOML value
    let parsed_value: toml::Value =
        toml::from_str::<toml::map::Map<String, toml::Value>>(&format!("v = {value}"))
            .with_context(|| format!("invalid TOML value: {value}"))
            .and_then(|t| {
                t.into_iter()
                    .next()
                    .map(|(_, v)| v)
                    .context("empty TOML parse result")
            })?;

    table.insert(key.to_string(), parsed_value);

    let serialized = toml::to_string(&table).context("serializing config")?;

    std::fs::create_dir_all(config_dir()?)?;
    crate::edit::atomic_write_bytes(&path, serialized.as_bytes())
        .with_context(|| format!("writing {}", path.display()))?;
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
        tracing::debug!(path = %primary.display(), "loaded config file");
        let content = std::fs::read_to_string(&primary)
            .with_context(|| format!("reading {}", primary.display()))?;
        return toml::from_str(&content)
            .with_context(|| format!("invalid config in {}", primary.display()));
    }

    // Fallback: platform config dir (~/Library/Application Support/nex/ on macOS)
    // for backwards compatibility with configs written before this fix.
    if let Some(platform_dir) = dirs::config_dir() {
        let legacy = platform_dir.join("nex/config.toml");
        if legacy.exists() {
            let content = std::fs::read_to_string(&legacy)
                .with_context(|| format!("reading {}", legacy.display()))?;
            return toml::from_str(&content)
                .with_context(|| format!("invalid config in {}", legacy.display()));
        }
    }

    Ok(FileConfig::default())
}
