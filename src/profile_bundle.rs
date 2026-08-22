use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PROFILE_SOURCE_FILE: &str = "source";
pub const PROFILE_RESOLVED_FILE: &str = "resolved.toml";
pub const PROFILE_STATE_FILE: &str = "state.json";

#[derive(Clone, Debug)]
pub struct ProfileBundle {
    pub source: String,
    pub resolved_toml: String,
    pub state: ProfileState,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileState {
    pub schema: u32,
    pub requested_source: String,
    pub layers: Vec<ProfileLayerState>,
    pub resolved_content_sha256: String,
    pub nex_version: String,
    pub resolver_schema: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProfileLayerState {
    pub source: String,
}

impl ProfileBundle {
    pub fn new(source: &str, resolved_toml: String, layers: &[String]) -> Result<Self> {
        let bundle = Self {
            source: source.to_string(),
            state: ProfileState {
                schema: 1,
                requested_source: source.to_string(),
                layers: layers
                    .iter()
                    .map(|source| ProfileLayerState {
                        source: source.clone(),
                    })
                    .collect(),
                resolved_content_sha256: content_sha256(&resolved_toml),
                nex_version: env!("CARGO_PKG_VERSION").to_string(),
                resolver_schema: "legacy-profile-v1".to_string(),
            },
            resolved_toml,
        };
        bundle.validate()?;
        Ok(bundle)
    }

    pub fn read_from(dir: &Path) -> Result<Option<Self>> {
        let source_path = dir.join(PROFILE_SOURCE_FILE);
        let resolved_path = dir.join(PROFILE_RESOLVED_FILE);
        let state_path = dir.join(PROFILE_STATE_FILE);
        let present = [
            source_path.as_path(),
            resolved_path.as_path(),
            state_path.as_path(),
        ]
        .map(Path::exists);

        if !present.iter().any(|present| *present) {
            if dir.exists() {
                bail!(
                    "unsupported bundled profile in {}: expected {}, {}, and {}",
                    dir.display(),
                    PROFILE_SOURCE_FILE,
                    PROFILE_RESOLVED_FILE,
                    PROFILE_STATE_FILE
                );
            }
            return Ok(None);
        }
        if !present.iter().all(|present| *present) {
            bail!(
                "incomplete bundled profile in {}: expected {}, {}, and {}",
                dir.display(),
                PROFILE_SOURCE_FILE,
                PROFILE_RESOLVED_FILE,
                PROFILE_STATE_FILE
            );
        }

        let source = std::fs::read_to_string(&source_path)
            .with_context(|| format!("failed to read {}", source_path.display()))?
            .trim()
            .to_string();
        let resolved_toml = std::fs::read_to_string(&resolved_path)
            .with_context(|| format!("failed to read {}", resolved_path.display()))?;
        let state: ProfileState = serde_json::from_str(
            &std::fs::read_to_string(&state_path)
                .with_context(|| format!("failed to read {}", state_path.display()))?,
        )
        .with_context(|| format!("invalid profile state in {}", state_path.display()))?;

        let bundle = Self {
            source,
            resolved_toml,
            state,
        };
        bundle.validate()?;
        Ok(Some(bundle))
    }

    pub fn write_to(&self, dir: &Path) -> Result<()> {
        self.validate()?;
        std::fs::create_dir_all(dir)
            .with_context(|| format!("failed to create {}", dir.display()))?;
        std::fs::write(dir.join(PROFILE_SOURCE_FILE), format!("{}\n", self.source))?;
        std::fs::write(dir.join(PROFILE_RESOLVED_FILE), &self.resolved_toml)?;
        std::fs::write(
            dir.join(PROFILE_STATE_FILE),
            format!("{}\n", serde_json::to_string_pretty(&self.state)?),
        )?;
        Ok(())
    }

    pub fn validate(&self) -> Result<toml::Value> {
        if self.source.trim().is_empty() {
            bail!("bundled profile source is empty");
        }
        if self.state.schema != 1 {
            bail!("unsupported bundled profile schema {}", self.state.schema);
        }
        if self.state.requested_source != self.source {
            bail!("bundled profile source does not match profile state");
        }
        if self.state.resolver_schema != "legacy-profile-v1" {
            bail!(
                "unsupported profile resolver schema {}",
                self.state.resolver_schema
            );
        }
        if self.state.layers.is_empty() {
            bail!("bundled profile state contains no layers");
        }
        if self
            .state
            .layers
            .iter()
            .any(|layer| layer.source.trim().is_empty())
        {
            bail!("bundled profile state contains an empty layer source");
        }
        if self.state.layers.last().map(|layer| layer.source.as_str()) != Some(self.source.as_str())
        {
            bail!("bundled profile leaf layer does not match its source");
        }

        let digest = content_sha256(&self.resolved_toml);
        if self.state.resolved_content_sha256 != digest {
            bail!("bundled profile content digest does not match profile state");
        }

        let profile: toml::Value = toml::from_str(&self.resolved_toml)
            .with_context(|| format!("invalid resolved profile from {}", self.source))?;
        if profile.get("secrets").is_some() {
            bail!("resolved profiles must not contain a top-level secrets table");
        }
        Ok(profile)
    }
}

fn content_sha256(content: &str) -> String {
    hex::encode(Sha256::digest(content.as_bytes()))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_valid_profile_bundle() {
        let dir = tempfile::tempdir().unwrap();
        let layers = vec!["base/profile".to_string(), "owner/profile".to_string()];
        let bundle = ProfileBundle::new(
            "owner/profile",
            "[packages]\nnix = [\"ripgrep\"]\n".to_string(),
            &layers,
        )
        .unwrap();

        bundle.write_to(dir.path()).unwrap();
        let loaded = ProfileBundle::read_from(dir.path()).unwrap().unwrap();

        assert_eq!(loaded.source, "owner/profile");
        assert_eq!(loaded.state.layers.len(), 2);
        assert!(loaded.validate().is_ok());
    }

    #[test]
    fn rejects_incomplete_profile_bundle() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(PROFILE_SOURCE_FILE), "owner/profile\n").unwrap();

        let error = ProfileBundle::read_from(dir.path()).unwrap_err();
        assert!(error.to_string().contains("incomplete bundled profile"));
    }

    #[test]
    fn rejects_modified_resolved_content() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = ProfileBundle::new(
            "owner/profile",
            "[packages]\nnix = [\"ripgrep\"]\n".to_string(),
            &["owner/profile".to_string()],
        )
        .unwrap();
        bundle.write_to(dir.path()).unwrap();
        std::fs::write(
            dir.path().join(PROFILE_RESOLVED_FILE),
            "[packages]\nnix = [\"curl\"]\n",
        )
        .unwrap();

        let error = ProfileBundle::read_from(dir.path()).unwrap_err();
        assert!(error.to_string().contains("digest does not match"));
    }

    #[test]
    fn rejects_secret_bearing_resolved_profile() {
        let error = ProfileBundle::new(
            "owner/profile",
            "[secrets]\ntoken = \"do-not-persist\"\n".to_string(),
            &["owner/profile".to_string()],
        )
        .unwrap_err();

        assert!(error.to_string().contains("must not contain"));
    }
}
