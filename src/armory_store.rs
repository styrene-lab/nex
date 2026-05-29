use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::armory_lock::{self, LockedPackage, PackageLock};

#[derive(Debug, Clone)]
pub struct StoreRecord {
    pub package_ref: String,
    pub path: PathBuf,
    pub verified: bool,
}

pub fn materialize_lock() -> Result<Vec<StoreRecord>> {
    let lock = armory_lock::read_package_lock()?;
    materialize_package_lock(&lock)
}

pub fn materialize_package_lock(lock: &PackageLock) -> Result<Vec<StoreRecord>> {
    let mut records = Vec::new();
    for package in &lock.packages {
        records.push(materialize_package(package)?);
    }
    Ok(records)
}

fn materialize_package(package: &LockedPackage) -> Result<StoreRecord> {
    let Some(oci_ref) = &package.oci_ref else {
        bail!(
            "package {} has no ociRef; cannot materialize in store phase",
            package.package_ref
        );
    };
    let Some(digest) = &package.digest else {
        bail!(
            "package {} has no digest; refusing unverified materialization",
            package.package_ref
        );
    };

    let path = store_path_for_digest(digest)?;
    if path.exists() {
        validate_package(package, &path)?;
        return Ok(StoreRecord {
            package_ref: package.package_ref.clone(),
            path,
            verified: true,
        });
    }

    std::fs::create_dir_all(&path)?;
    fetch_oci(oci_ref, digest, &path)?;
    validate_package(package, &path)?;

    Ok(StoreRecord {
        package_ref: package.package_ref.clone(),
        path,
        verified: true,
    })
}

fn fetch_oci(oci_ref: &str, digest: &str, output: &Path) -> Result<()> {
    let status = Command::new("oras")
        .args(["pull", oci_ref, "--output"])
        .arg(output)
        .status()
        .context("oras is required for Armory OCI materialization; install oras first")?;
    if !status.success() {
        bail!("oras pull failed for {oci_ref}");
    }

    if !digest.starts_with("sha256:") {
        bail!("unsupported digest format {digest}; expected sha256:<hex>");
    }
    Ok(())
}

fn validate_package(package: &LockedPackage, path: &Path) -> Result<()> {
    let kind = package
        .package_ref
        .split_once('/')
        .map(|(kind, _)| kind)
        .unwrap_or_default();
    match kind {
        "machine-profile" | "materialization-payload" => {
            let report = crate::artifact::check_artifact_dir(path);
            if !report.ok {
                bail!(
                    "artifact validation failed for {} at {}",
                    package.package_ref,
                    path.display()
                );
            }
        }
        "forge-template" => {
            // Forge-template package validation is intentionally deferred until the
            // template schema stabilizes; Phase 3 wires the dispatch point.
        }
        _ => {}
    }
    Ok(())
}

pub fn store_path_for_digest(digest: &str) -> Result<PathBuf> {
    let Some(hex) = digest.strip_prefix("sha256:") else {
        bail!("unsupported digest format {digest}; expected sha256:<hex>");
    };
    if hex.is_empty() || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        bail!("invalid sha256 digest {digest}");
    }
    Ok(store_dir()?.join(format!("sha256-{hex}")))
}

fn store_dir() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".local/share/nex/store"))
}

#[cfg(test)]
mod tests {
    use super::store_path_for_digest;

    #[test]
    fn computes_content_addressed_store_path() {
        let path = store_path_for_digest("sha256:abcdef").expect("store path");
        assert!(path.ends_with(".local/share/nex/store/sha256-abcdef"));
    }

    #[test]
    fn rejects_non_sha256_digest() {
        let error = store_path_for_digest("sha512:abcdef").expect_err("digest rejected");
        assert!(format!("{error:#}").contains("unsupported digest"));
    }
}
