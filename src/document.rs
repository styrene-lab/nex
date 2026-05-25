use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::de::DeserializeOwned;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentFormat {
    Pkl,
    TomlCompat,
}

#[derive(Debug, Clone)]
pub struct LoadedDocument<T> {
    pub value: T,
    pub format: DocumentFormat,
    pub path: PathBuf,
}

impl<T> LoadedDocument<T> {
    pub fn is_canonical(&self) -> bool {
        self.format == DocumentFormat::Pkl
    }
}

pub fn load_document<T>(path: &Path, description: &str) -> Result<LoadedDocument<T>>
where
    T: DeserializeOwned,
{
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("pkl") => {
            let value = crate::pkl::evaluate_json(path)
                .and_then(|json| serde_json::from_value(json).context("decoding evaluated Pkl JSON"))
                .with_context(|| format!("loading canonical Pkl {description} {}", path.display()))?;
            Ok(LoadedDocument {
                value,
                format: DocumentFormat::Pkl,
                path: path.to_path_buf(),
            })
        }
        Some("toml") => {
            let content = std::fs::read_to_string(path)
                .with_context(|| format!("reading compatibility TOML {description} {}", path.display()))?;
            let value = toml::from_str(&content)
                .with_context(|| format!("parsing compatibility TOML {description} {}", path.display()))?;
            Ok(LoadedDocument {
                value,
                format: DocumentFormat::TomlCompat,
                path: path.to_path_buf(),
            })
        }
        Some(ext) => bail!(
            "unsupported {description} extension .{ext}; canonical Nex documents use .pkl (.toml is compatibility/interchange)"
        ),
        None => bail!(
            "{description} path must have an extension; canonical Nex documents use .pkl (.toml is compatibility/interchange)"
        ),
    }
}
