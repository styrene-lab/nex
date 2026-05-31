use std::collections::BTreeMap;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

const NEX_MAPPING_PKL: &str = include_str!("../data/devenv/nex-mapping.v1.pkl");
const UPSTREAM_SOURCE_JSON: &str = include_str!("../data/devenv/upstream/source.json");
const UPSTREAM_DEVENV_SCHEMA_JSON: &str =
    include_str!("../data/devenv/upstream/devenv.schema.json");

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DevenvMappingCatalog {
    pub schema: String,
    pub reviewed_at: String,
    pub source: String,
    pub mappings: BTreeMap<String, DevenvSurfaceMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DevenvSurfaceMapping {
    pub kind: String,
    pub bucket: String,
    pub target: String,
    #[serde(default)]
    pub safety: Vec<String>,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct DevenvUpstreamSource {
    pub schema: String,
    pub repo: String,
    pub rev: String,
    pub reviewed_at: String,
    pub options_source: String,
    pub yaml_schema_source: String,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DevenvSurfaceCatalogReport {
    pub schema: String,
    pub mapping: DevenvMappingCatalog,
    pub upstream: DevenvUpstreamSource,
    pub yaml_top_level_properties: Vec<String>,
}

pub fn load_devenv_surface_catalog() -> Result<DevenvSurfaceCatalogReport> {
    let mapping = parse_mapping_pkl(NEX_MAPPING_PKL)?;
    validate_mapping(&mapping)?;
    let upstream: DevenvUpstreamSource = serde_json::from_str(UPSTREAM_SOURCE_JSON)
        .context("parsing embedded devenv upstream source metadata")?;
    let yaml_schema: serde_json::Value = serde_json::from_str(UPSTREAM_DEVENV_SCHEMA_JSON)
        .context("parsing embedded devenv.yaml schema")?;
    let mut yaml_top_level_properties = yaml_schema
        .get("properties")
        .and_then(serde_json::Value::as_object)
        .map(|properties| properties.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    yaml_top_level_properties.sort();

    Ok(DevenvSurfaceCatalogReport {
        schema: "io.styrene.nex.devenv-surface-catalog-report.v1".to_string(),
        mapping,
        upstream,
        yaml_top_level_properties,
    })
}

pub fn find_mapping<'a>(
    catalog: &'a DevenvMappingCatalog,
    option_path: &str,
) -> Option<(&'a str, &'a DevenvSurfaceMapping)> {
    catalog
        .mappings
        .iter()
        .filter(|(pattern, _)| mapping_pattern_matches(pattern, option_path))
        .max_by_key(|(pattern, _)| pattern_specificity(pattern))
        .map(|(pattern, mapping)| (pattern.as_str(), mapping))
}

fn parse_mapping_pkl(input: &str) -> Result<DevenvMappingCatalog> {
    let schema = capture_scalar(input, "schema").context("mapping schema is missing")?;
    let reviewed_at =
        capture_scalar(input, "reviewedAt").context("mapping reviewedAt is missing")?;
    let source = capture_scalar(input, "source").unwrap_or_default();
    let mappings_body = capture_block(input, "mappings").context("mappings block is missing")?;
    let mappings = parse_mapping_entries(&mappings_body)?;
    Ok(DevenvMappingCatalog {
        schema,
        reviewed_at,
        source,
        mappings,
    })
}

fn parse_mapping_entries(body: &str) -> Result<BTreeMap<String, DevenvSurfaceMapping>> {
    let mut mappings = BTreeMap::new();
    let mut remaining = body;
    while let Some(start) = remaining.find("[\"") {
        remaining = &remaining[start + 2..];
        let Some(end_pattern) = remaining.find("\"]") else {
            bail!("unterminated mapping pattern");
        };
        let pattern = remaining[..end_pattern].to_string();
        remaining = &remaining[end_pattern + 2..];
        let Some(open_brace) = remaining.find('{') else {
            bail!("mapping {pattern} is missing body");
        };
        let (entry_body, consumed) = extract_braced_block(&remaining[open_brace..])?;
        remaining = &remaining[open_brace + consumed..];
        mappings.insert(pattern, parse_mapping_entry(&entry_body)?);
    }
    Ok(mappings)
}

fn parse_mapping_entry(body: &str) -> Result<DevenvSurfaceMapping> {
    Ok(DevenvSurfaceMapping {
        kind: capture_scalar(body, "kind").context("mapping kind is missing")?,
        bucket: capture_scalar(body, "bucket").context("mapping bucket is missing")?,
        target: capture_scalar(body, "target").context("mapping target is missing")?,
        safety: capture_string_list(body, "safety")?,
        action: capture_scalar(body, "action").context("mapping action is missing")?,
        rationale: capture_scalar(body, "rationale"),
    })
}

fn validate_mapping(mapping: &DevenvMappingCatalog) -> Result<()> {
    if mapping.schema != "io.styrene.nex.devenv-mapping.v1" {
        bail!("unsupported devenv mapping schema {}", mapping.schema);
    }
    if mapping.mappings.is_empty() {
        bail!("devenv mapping catalog must contain at least one mapping");
    }
    for (pattern, entry) in &mapping.mappings {
        if pattern.trim().is_empty() {
            bail!("devenv mapping pattern cannot be empty");
        }
        if entry.kind.trim().is_empty()
            || entry.bucket.trim().is_empty()
            || entry.action.trim().is_empty()
        {
            bail!("devenv mapping {pattern} has an empty required field");
        }
    }
    Ok(())
}

fn mapping_pattern_matches(pattern: &str, option_path: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix(".*") {
        option_path == prefix || option_path.starts_with(&format!("{prefix}."))
    } else {
        option_path == pattern
    }
}

fn pattern_specificity(pattern: &str) -> usize {
    pattern.trim_end_matches(".*").len()
}

fn capture_scalar(input: &str, key: &str) -> Option<String> {
    let needle = format!("{key} = \"");
    let start = input.find(&needle)? + needle.len();
    let end = input[start..].find('"')?;
    Some(input[start..start + end].to_string())
}

fn capture_string_list(input: &str, key: &str) -> Result<Vec<String>> {
    let needle = format!("{key} = List(");
    let Some(start) = input.find(&needle) else {
        return Ok(Vec::new());
    };
    let start = start + needle.len();
    let Some(end) = input[start..].find(')') else {
        bail!("unterminated List for {key}");
    };
    Ok(input[start..start + end]
        .split(',')
        .filter_map(|raw| {
            let trimmed = raw.trim();
            trimmed
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .map(ToString::to_string)
        })
        .collect())
}

fn capture_block(input: &str, key: &str) -> Option<String> {
    let start = input.find(key)?;
    let after_key = &input[start + key.len()..];
    let open_brace = after_key.find('{')?;
    let (block, _) = extract_braced_block(&after_key[open_brace..]).ok()?;
    Some(block)
}

fn extract_braced_block(input: &str) -> Result<(String, usize)> {
    if !input.starts_with('{') {
        bail!("block must start with an opening brace");
    }
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (index, ch) in input.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Ok((input[1..index].to_string(), index + 1));
                }
            }
            _ => {}
        }
    }
    bail!("unterminated braced block")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_embedded_devenv_surface_catalog() -> Result<()> {
        let report = load_devenv_surface_catalog()?;

        assert_eq!(report.mapping.schema, "io.styrene.nex.devenv-mapping.v1");
        assert_eq!(
            report.upstream.rev,
            "21d68a204558895af93ad82014f8fa83f9c9a51e"
        );
        assert!(report.mapping.mappings.contains_key("packages"));
        assert!(report.mapping.mappings.contains_key("languages.*"));
        assert!(report
            .yaml_top_level_properties
            .contains(&"secretspec".to_string()));
        Ok(())
    }

    #[test]
    fn matches_exact_and_wildcard_surface_mappings() -> Result<()> {
        let report = load_devenv_surface_catalog()?;

        let (_, packages) = find_mapping(&report.mapping, "packages").expect("packages mapping");
        assert_eq!(packages.target, "profile.packages");

        let (_, rust) =
            find_mapping(&report.mapping, "languages.rust.enable").expect("rust mapping");
        assert_eq!(rust.kind, "language");
        assert_eq!(rust.bucket, "portable");

        let (_, postgres) =
            find_mapping(&report.mapping, "services.postgres.enable").expect("service mapping");
        assert_eq!(postgres.bucket, "machine-scoped-candidate");
        assert!(find_mapping(&report.mapping, "unknown.future.option").is_none());
        Ok(())
    }
}
