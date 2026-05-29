use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::discover::{self, Platform};

pub const CONFIG_FILE: &str = "config.pkl";
pub const CONFIG_TOML_COMPAT_FILE: &str = "config.toml";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LocalConfigFormat {
    Pkl,
    TomlCompat,
}

struct LocalConfigSource {
    path: PathBuf,
    format: LocalConfigFormat,
}

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
    /// Armory package registries for Omegon/package discovery.
    pub registries: Vec<RegistryConfig>,
}

/// Optional config file at ~/.config/nex/config.pkl; config.toml is compatibility.
#[derive(Deserialize, Serialize, Default)]
struct FileConfig {
    repo_path: Option<String>,
    hostname: Option<String>,
    prefer_nix_on_equal: Option<bool>,
    identity: Option<IdentityConfig>,
    registries: Option<Vec<RegistryConfig>>,
}

/// Identity-related configuration.
#[derive(Deserialize, Serialize, Default, Clone)]
pub struct IdentityConfig {
    /// Git commit signing settings.
    pub git: Option<IdentityGitConfig>,
    /// SSH key label registry.
    pub ssh: Option<IdentitySshConfig>,
}

/// Git signing config stored in nex config.
#[derive(Deserialize, Serialize, Default, Clone)]
pub struct IdentityGitConfig {
    pub name: Option<String>,
    pub email: Option<String>,
}

/// SSH key labels registered for this identity.
#[derive(Deserialize, Serialize, Default, Clone)]
pub struct IdentitySshConfig {
    pub labels: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct RegistryConfig {
    pub name: String,
    pub url: String,
    pub trust: Option<String>,
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
                 or create ~/.config/nex/config.pkl with repo_path."
            }
            Platform::Linux => {
                "Could not find NixOS config repo. Run `nex init`, set NEX_REPO, \
                 or create ~/.config/nex/config.pkl with repo_path."
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
        let registries = file_config
            .registries
            .filter(|registries| !registries.is_empty())
            .unwrap_or_else(|| vec![crate::armory::default_registry()]);

        Ok(Config {
            repo,
            hostname,
            nix_packages_file,
            homebrew_file,
            module_files,
            prefer_nix_on_equal,
            platform,
            registries,
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
    let path = writable_config_path()?;
    let content = read_config_value_for_write(&path)?;

    let mut table = match content {
        toml::Value::Table(table) => table,
        _ => bail!("local Nex config root must be a table"),
    };

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
    write_config_value(&path, &toml::Value::Table(table))
}

/// Canonical config directory: ~/.config/nex/
pub fn config_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("no home directory")?;
    Ok(home.join(".config/nex"))
}

pub fn canonical_config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join(CONFIG_FILE))
}

pub fn toml_compat_config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join(CONFIG_TOML_COMPAT_FILE))
}

/// Load just the identity portion of the config (does not require a nix repo).
pub fn load_identity_config() -> Result<IdentityConfig> {
    let fc = load_file_config().unwrap_or_default();
    Ok(fc.identity.unwrap_or_default())
}

/// Set a nested config key using dotted notation (e.g. "identity.git.name").
/// Creates intermediate tables as needed.
pub fn set_nested_preference(dotted_key: &str, value: toml::Value) -> Result<()> {
    let path = writable_config_path()?;
    let mut root = read_config_value_for_write(&path)?;

    let parts: Vec<&str> = dotted_key.split('.').collect();
    let mut current = &mut root;
    for part in &parts[..parts.len() - 1] {
        if !current.is_table() {
            bail!("config key '{part}' is not a table");
        }
        current = current
            .as_table_mut()
            .expect("checked above")
            .entry(part.to_string())
            .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    }

    let leaf = parts.last().context("empty key")?;
    current
        .as_table_mut()
        .context("leaf parent is not a table")?
        .insert(leaf.to_string(), value);

    write_config_value(&path, &root)
}

/// Append a string to an array config value, creating it if it doesn't exist.
/// Skips duplicates.
pub fn append_to_list(dotted_key: &str, item: &str) -> Result<()> {
    let path = writable_config_path()?;
    let mut root = read_config_value_for_write(&path)?;

    let parts: Vec<&str> = dotted_key.split('.').collect();
    let mut current = &mut root;
    for part in &parts[..parts.len() - 1] {
        current = current
            .as_table_mut()
            .context("not a table")?
            .entry(part.to_string())
            .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    }

    let leaf = parts.last().context("empty key")?;
    let table = current
        .as_table_mut()
        .context("leaf parent is not a table")?;
    let arr = table
        .entry(leaf.to_string())
        .or_insert_with(|| toml::Value::Array(Vec::new()));

    let arr = arr.as_array_mut().context("config key is not an array")?;
    let item_val = toml::Value::String(item.to_string());
    if !arr.contains(&item_val) {
        arr.push(item_val);
    }

    write_config_value(&path, &root)
}

pub fn write_initial_config(repo_path: &Path, hostname: &str) -> Result<PathBuf> {
    let path = canonical_config_path()?;
    let mut table = toml::map::Map::new();
    table.insert(
        "repo_path".to_string(),
        toml::Value::String(repo_path.display().to_string()),
    );
    table.insert(
        "hostname".to_string(),
        toml::Value::String(hostname.to_string()),
    );
    write_config_value(&path, &toml::Value::Table(table))?;
    Ok(path)
}

pub fn migrate_to_pkl(keep_toml: bool) -> Result<PathBuf> {
    let source = resolve_config_source()?;
    let value = match source {
        Some(LocalConfigSource {
            path,
            format: LocalConfigFormat::Pkl,
        }) => read_config_value_for_write(&path)?,
        Some(LocalConfigSource {
            path,
            format: LocalConfigFormat::TomlCompat,
        }) => read_config_value_for_write(&path)?,
        None => toml::Value::Table(toml::map::Map::new()),
    };

    let canonical = canonical_config_path()?;
    write_config_value(&canonical, &value)?;

    if keep_toml {
        let compat = toml_compat_config_path()?;
        let rendered = toml::to_string_pretty(&value).context("serializing compatibility TOML")?;
        crate::edit::atomic_write_bytes(&compat, rendered.as_bytes())
            .with_context(|| format!("writing {}", compat.display()))?;
    }

    Ok(canonical)
}

pub fn export_config_toml() -> Result<String> {
    let config = load_file_config()?;
    let value = toml::Value::try_from(config).context("converting config to TOML value")?;
    toml::to_string_pretty(&value).context("serializing config TOML")
}

fn load_file_config() -> Result<FileConfig> {
    match resolve_config_source()? {
        Some(source) => match source.format {
            LocalConfigFormat::Pkl => {
                tracing::debug!(path = %source.path.display(), "loaded canonical Pkl config file");
                crate::document::load_document::<FileConfig>(&source.path, "local Nex config")
                    .map(|loaded| loaded.value)
            }
            LocalConfigFormat::TomlCompat => {
                tracing::debug!(path = %source.path.display(), "loaded compatibility TOML config file");
                let content = std::fs::read_to_string(&source.path)
                    .with_context(|| format!("reading {}", source.path.display()))?;
                toml::from_str(&content)
                    .with_context(|| format!("invalid config in {}", source.path.display()))
            }
        },
        None => Ok(FileConfig::default()),
    }
}

fn resolve_config_source() -> Result<Option<LocalConfigSource>> {
    let canonical = canonical_config_path()?;
    if canonical.exists() {
        return Ok(Some(LocalConfigSource {
            path: canonical,
            format: LocalConfigFormat::Pkl,
        }));
    }

    let compat = toml_compat_config_path()?;
    if compat.exists() {
        return Ok(Some(LocalConfigSource {
            path: compat,
            format: LocalConfigFormat::TomlCompat,
        }));
    }

    if let Some(platform_dir) = dirs::config_dir() {
        let legacy = platform_dir.join(format!("nex/{CONFIG_TOML_COMPAT_FILE}"));
        if legacy.exists() {
            return Ok(Some(LocalConfigSource {
                path: legacy,
                format: LocalConfigFormat::TomlCompat,
            }));
        }
    }

    Ok(None)
}

fn writable_config_path() -> Result<PathBuf> {
    let canonical = canonical_config_path()?;
    if canonical.exists() {
        return Ok(canonical);
    }
    let compat = toml_compat_config_path()?;
    if compat.exists() {
        return Ok(compat);
    }
    Ok(canonical)
}

fn read_config_value_for_write(path: &Path) -> Result<toml::Value> {
    if !path.exists() {
        return Ok(toml::Value::Table(toml::map::Map::new()));
    }
    match config_format_for_path(path)? {
        LocalConfigFormat::Pkl => {
            let config =
                crate::document::load_document::<FileConfig>(path, "local Nex config")?.value;
            toml::Value::try_from(config).context("converting config to mutable value")
        }
        LocalConfigFormat::TomlCompat => {
            let content = std::fs::read_to_string(path)
                .with_context(|| format!("reading {}", path.display()))?;
            if content.trim().is_empty() {
                Ok(toml::Value::Table(toml::map::Map::new()))
            } else {
                toml::from_str(&content).with_context(|| format!("parsing {}", path.display()))
            }
        }
    }
}

fn write_config_value(path: &Path, value: &toml::Value) -> Result<()> {
    let serialized = serialize_config_value(value, config_format_for_path(path)?)?;
    std::fs::create_dir_all(config_dir()?)?;
    crate::edit::atomic_write_bytes(path, serialized.as_bytes())
        .with_context(|| format!("writing {}", path.display()))
}

fn config_format_for_path(path: &Path) -> Result<LocalConfigFormat> {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("pkl") => Ok(LocalConfigFormat::Pkl),
        Some("toml") => Ok(LocalConfigFormat::TomlCompat),
        Some(ext) => bail!("unsupported config extension .{ext}; canonical Nex config uses .pkl"),
        None => bail!("config path must have an extension"),
    }
}

fn serialize_config_value(value: &toml::Value, format: LocalConfigFormat) -> Result<String> {
    match format {
        LocalConfigFormat::Pkl => serialize_pkl_document(value),
        LocalConfigFormat::TomlCompat => {
            toml::to_string_pretty(value).context("serializing config TOML")
        }
    }
}

fn serialize_pkl_document(value: &toml::Value) -> Result<String> {
    let mut out =
        String::from("// Generated by nex. Edit config.pkl as the canonical local config.\n");
    let table = value.as_table().context("config root must be a table")?;
    for (key, value) in table {
        write_pkl_entry(&mut out, key, value, 0)?;
    }
    Ok(out)
}

fn write_pkl_entry(out: &mut String, key: &str, value: &toml::Value, indent: usize) -> Result<()> {
    let padding = "  ".repeat(indent);
    match value {
        toml::Value::Table(table) => {
            out.push_str(&format!("{padding}{key} {{\n"));
            for (child_key, child_value) in table {
                write_pkl_entry(out, child_key, child_value, indent + 1)?;
            }
            out.push_str(&format!("{padding}}}\n"));
        }
        _ => {
            out.push_str(&format!(
                "{padding}{key} = {}\n",
                pkl_literal(value).with_context(|| format!("serializing config key {key}"))?
            ));
        }
    }
    Ok(())
}

fn pkl_literal(value: &toml::Value) -> Result<String> {
    match value {
        toml::Value::String(value) => Ok(format!("{value:?}")),
        toml::Value::Boolean(value) => Ok(value.to_string()),
        toml::Value::Integer(value) => Ok(value.to_string()),
        toml::Value::Float(value) => Ok(value.to_string()),
        toml::Value::Array(values) => {
            let rendered = values
                .iter()
                .map(pkl_literal)
                .collect::<Result<Vec<_>>>()?
                .join(", ");
            Ok(format!("List({rendered})"))
        }
        toml::Value::Datetime(value) => Ok(format!("{:?}", value.to_string())),
        toml::Value::Table(_) => bail!("nested table should be serialized as a Pkl block"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn serializes_generated_pkl_config() {
        let mut table = toml::map::Map::new();
        table.insert(
            "repo_path".to_string(),
            toml::Value::String("/tmp/repo".to_string()),
        );
        table.insert(
            "hostname".to_string(),
            toml::Value::String("test-host".to_string()),
        );
        table.insert(
            "prefer_nix_on_equal".to_string(),
            toml::Value::Boolean(true),
        );

        let rendered = serialize_pkl_document(&toml::Value::Table(table)).unwrap();
        assert!(rendered.contains("repo_path = \"/tmp/repo\""));
        assert!(rendered.contains("hostname = \"test-host\""));
        assert!(rendered.contains("prefer_nix_on_equal = true"));
    }

    #[test]
    fn resolves_pkl_before_toml() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(CONFIG_FILE), "repo_path = \"/pkl\"\n").unwrap();
        fs::write(
            dir.path().join(CONFIG_TOML_COMPAT_FILE),
            "repo_path = \"/toml\"\n",
        )
        .unwrap();

        let pkl = dir.path().join(CONFIG_FILE);
        let toml = dir.path().join(CONFIG_TOML_COMPAT_FILE);
        assert!(pkl.exists());
        assert!(toml.exists());
        assert_eq!(
            config_format_for_path(&pkl).unwrap(),
            LocalConfigFormat::Pkl
        );
        assert_eq!(
            config_format_for_path(&toml).unwrap(),
            LocalConfigFormat::TomlCompat
        );
    }
    #[test]
    fn migrate_preserves_toml_export_shape() {
        let mut table = toml::map::Map::new();
        table.insert(
            "repo_path".to_string(),
            toml::Value::String("/tmp/repo".to_string()),
        );
        table.insert(
            "hostname".to_string(),
            toml::Value::String("test-host".to_string()),
        );
        let value = toml::Value::Table(table);
        let pkl = serialize_config_value(&value, LocalConfigFormat::Pkl).unwrap();
        let toml = serialize_config_value(&value, LocalConfigFormat::TomlCompat).unwrap();

        assert!(pkl.contains("repo_path = \"/tmp/repo\""));
        assert!(toml.contains("repo_path = \"/tmp/repo\""));
        assert!(toml::from_str::<toml::Value>(&toml).is_ok());
    }
}
