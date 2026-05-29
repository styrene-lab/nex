use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::armory::{self, ArmoryIndex, ArmoryPackage, PackageRef};
use crate::config::{Config, RegistryConfig};

pub const PACKAGE_LOCK_SCHEMA: &str = "io.styrene.nex.package-lock.v1";
pub const ACTIVATION_LOCK_SCHEMA: &str = "io.styrene.nex.omegon-activation-lock.v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PackageLock {
    pub schema: String,
    pub registries: Vec<LockedRegistry>,
    pub roots: Vec<LockedRoot>,
    pub packages: Vec<LockedPackage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LockedRegistry {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LockedRoot {
    pub package_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LockedPackage {
    pub package_ref: String,
    pub version: Option<String>,
    pub registry: String,
    pub oci_ref: Option<String>,
    pub digest: Option<String>,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OmegonActivationLock {
    pub schema: String,
    pub root: ActivationRoot,
    pub packages: Vec<ActivationPackage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ActivationRoot {
    pub kind: String,
    pub id: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ActivationPackage {
    pub kind: String,
    pub id: String,
    pub version: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct ResolvedGraph {
    pub root: PackageRef,
    pub registry: RegistryConfig,
    pub packages: Vec<LockedPackage>,
    pub optional_skipped: Vec<String>,
}

pub fn install(config: &Config, root: &PackageRef, dry_run: bool) -> Result<()> {
    let graph = resolve_from_config(config, root)?;
    print_plan(&graph);
    if dry_run {
        return Ok(());
    }

    let package_lock = package_lock_for_graph(&graph);
    write_package_lock(&package_lock)?;

    if is_omegon_runtime_root(&graph.root.kind) {
        let activation_lock = activation_lock_for_graph(&graph)?;
        write_activation_lock(&activation_lock)?;
    }

    println!("  wrote {}", package_lock_path()?.display());
    if is_omegon_runtime_root(&graph.root.kind) {
        println!("  wrote {}", activation_lock_path()?.display());
    }
    Ok(())
}

pub fn refresh(config: &Config) -> Result<()> {
    let existing = read_package_lock()?;
    for root in existing.roots {
        let package_ref = PackageRef::parse(&root.package_ref)?;
        install(config, &package_ref, false)?;
    }
    Ok(())
}

pub fn resolve_from_config(config: &Config, root: &PackageRef) -> Result<ResolvedGraph> {
    for registry in &config.registries {
        let index = armory::fetch_index(registry)?;
        if armory::find(&index, root).is_some() {
            return resolve_graph(registry, &index, root);
        }
    }
    bail!("Armory package {root} not found in configured registries")
}

pub fn resolve_graph(
    registry: &RegistryConfig,
    index: &ArmoryIndex,
    root: &PackageRef,
) -> Result<ResolvedGraph> {
    let mut packages = BTreeMap::new();
    let mut optional_skipped = Vec::new();
    let mut visiting = Vec::new();
    resolve_one(
        registry,
        index,
        root,
        &mut packages,
        &mut optional_skipped,
        &mut visiting,
    )?;

    Ok(ResolvedGraph {
        root: root.clone(),
        registry: registry.clone(),
        packages: packages.into_values().collect(),
        optional_skipped,
    })
}

fn resolve_one(
    registry: &RegistryConfig,
    index: &ArmoryIndex,
    package_ref: &PackageRef,
    packages: &mut BTreeMap<String, LockedPackage>,
    optional_skipped: &mut Vec<String>,
    visiting: &mut Vec<String>,
) -> Result<()> {
    let key = package_ref.to_string();
    if packages.contains_key(&key) {
        return Ok(());
    }
    if let Some(pos) = visiting.iter().position(|item| item == &key) {
        let mut cycle = visiting[pos..].to_vec();
        cycle.push(key);
        bail!("Armory dependency cycle: {}", cycle.join(" -> "));
    }

    let package = armory::find(index, package_ref)
        .with_context(|| format!("missing required Armory dependency {package_ref}"))?;

    visiting.push(key.clone());
    let dependencies = required_dependency_refs(package);
    for dependency in &dependencies {
        let dep_ref = PackageRef::parse(dependency)?;
        resolve_one(
            registry,
            index,
            &dep_ref,
            packages,
            optional_skipped,
            visiting,
        )?;
    }
    for optional in optional_dependency_refs(package) {
        optional_skipped.push(optional);
    }
    visiting.pop();

    let locked = LockedPackage {
        package_ref: key.clone(),
        version: package.version.clone(),
        registry: registry.name.clone(),
        oci_ref: package.oci_ref.clone(),
        digest: package.digest.clone(),
        dependencies,
    };

    if let Some(existing) = packages.get(&key) {
        if existing.version != locked.version || existing.digest != locked.digest {
            bail!("conflicting Armory package records for {key}");
        }
    }
    packages.insert(key, locked);
    Ok(())
}

fn required_dependency_refs(package: &ArmoryPackage) -> Vec<String> {
    package
        .dependencies
        .iter()
        .filter(|dep| !dep.optional.unwrap_or(false))
        .filter_map(|dep| dep.display_ref().map(ToString::to_string))
        .collect()
}

fn optional_dependency_refs(package: &ArmoryPackage) -> Vec<String> {
    let mut refs: Vec<String> = package
        .dependencies
        .iter()
        .filter(|dep| dep.optional.unwrap_or(false))
        .filter_map(|dep| dep.display_ref().map(ToString::to_string))
        .collect();
    refs.extend(
        package
            .optional_dependencies
            .iter()
            .filter_map(|dep| dep.display_ref().map(ToString::to_string)),
    );
    refs
}

fn package_lock_for_graph(graph: &ResolvedGraph) -> PackageLock {
    PackageLock {
        schema: PACKAGE_LOCK_SCHEMA.to_string(),
        registries: vec![LockedRegistry {
            name: graph.registry.name.clone(),
            url: graph.registry.url.clone(),
        }],
        roots: vec![LockedRoot {
            package_ref: graph.root.to_string(),
        }],
        packages: graph.packages.clone(),
    }
}

fn activation_lock_for_graph(graph: &ResolvedGraph) -> Result<OmegonActivationLock> {
    let root_package = graph
        .packages
        .iter()
        .find(|package| package.package_ref == graph.root.to_string())
        .context("resolved graph missing root package")?;
    Ok(OmegonActivationLock {
        schema: ACTIVATION_LOCK_SCHEMA.to_string(),
        root: ActivationRoot {
            kind: graph.root.kind.clone(),
            id: graph.root.id.clone(),
            version: root_package.version.clone(),
        },
        packages: graph
            .packages
            .iter()
            .map(|package| {
                let package_ref = PackageRef::parse(&package.package_ref)?;
                Ok(ActivationPackage {
                    kind: package_ref.kind,
                    id: package_ref.id,
                    version: package.version.clone(),
                    status: "pending".to_string(),
                })
            })
            .collect::<Result<Vec<_>>>()?,
    })
}

fn is_omegon_runtime_root(kind: &str) -> bool {
    matches!(
        kind,
        "skill" | "persona" | "tone" | "profile" | "agent" | "extension" | "workstation"
    )
}

fn print_plan(graph: &ResolvedGraph) {
    println!("Armory install plan: {}", graph.root);
    for package in &graph.packages {
        let version = package.version.as_deref().unwrap_or("unknown");
        println!("  {} {}", package.package_ref, version);
    }
    for optional in &graph.optional_skipped {
        println!("  optional skipped: {optional}");
    }
}

fn write_package_lock(lock: &PackageLock) -> Result<()> {
    let path = package_lock_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(lock)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

fn write_activation_lock(lock: &OmegonActivationLock) -> Result<()> {
    let path = activation_lock_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(lock)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

fn read_package_lock() -> Result<PackageLock> {
    let path = package_lock_path()?;
    let bytes = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_slice(&bytes).context("parsing package lock")
}

pub fn package_lock_path() -> Result<PathBuf> {
    Ok(state_dir()?.join("packages.lock.json"))
}

pub fn activation_lock_path() -> Result<PathBuf> {
    Ok(state_dir()?.join("omegon-activation-lock.json"))
}

fn state_dir() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".local/state/nex"))
}

#[cfg(test)]
mod tests {
    use super::{activation_lock_for_graph, resolve_graph};
    use crate::armory::parse_index;
    use crate::armory::PackageRef;
    use crate::config::RegistryConfig;

    fn registry() -> RegistryConfig {
        RegistryConfig {
            name: "test".to_string(),
            url: "https://example.test/index.json".to_string(),
            trust: None,
        }
    }

    #[test]
    fn resolves_required_dependencies() {
        let index = parse_index(
            br#"{"packages":[
                {"packageRef":"profile/root","version":"1.0.0","dependencies":[{"packageRef":"skill/rust"}]},
                {"packageRef":"skill/rust","version":"1.0.0"}
            ]}"#,
        )
        .expect("index");
        let graph = resolve_graph(
            &registry(),
            &index,
            &PackageRef::parse("profile/root").expect("ref"),
        )
        .expect("graph");
        assert_eq!(graph.packages.len(), 2);
    }

    #[test]
    fn missing_dependency_fails() {
        let index = parse_index(
            br#"{"packages":[{"packageRef":"profile/root","dependencies":[{"packageRef":"skill/missing"}]}]}"#,
        )
        .expect("index");
        let error = resolve_graph(
            &registry(),
            &index,
            &PackageRef::parse("profile/root").expect("ref"),
        )
        .expect_err("missing dependency");
        assert!(format!("{error:#}").contains("skill/missing"));
    }

    #[test]
    fn dependency_cycle_fails() {
        let index = parse_index(
            br#"{"packages":[
                {"packageRef":"profile/a","dependencies":[{"packageRef":"profile/b"}]},
                {"packageRef":"profile/b","dependencies":[{"packageRef":"profile/a"}]}
            ]}"#,
        )
        .expect("index");
        let error = resolve_graph(
            &registry(),
            &index,
            &PackageRef::parse("profile/a").expect("ref"),
        )
        .expect_err("cycle");
        assert!(format!("{error:#}").contains("profile/a -> profile/b -> profile/a"));
    }

    #[test]
    fn activation_lock_is_pending_until_materialized() {
        let index =
            parse_index(br#"{"packages":[{"packageRef":"profile/root","version":"1.0.0"}]}"#)
                .expect("index");
        let graph = resolve_graph(
            &registry(),
            &index,
            &PackageRef::parse("profile/root").expect("ref"),
        )
        .expect("graph");
        let lock = activation_lock_for_graph(&graph).expect("activation lock");
        assert_eq!(lock.packages[0].status, "pending");
    }
}
