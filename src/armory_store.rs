use std::fs::File;
use std::io::Read;
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
    let mut lock = armory_lock::read_package_lock()?;
    let records = materialize_package_lock(&mut lock)?;
    armory_lock::write_package_lock(&lock)?;
    if let Some(activation_lock) = armory_lock::activation_lock_for_package_lock(&lock)? {
        armory_lock::write_activation_lock(&activation_lock)?;
    }
    Ok(records)
}

pub fn materialize_package_lock(lock: &mut PackageLock) -> Result<Vec<StoreRecord>> {
    let mut records = Vec::new();
    for package in &mut lock.packages {
        let record = materialize_package(package)?;
        package.path = Some(record.path.display().to_string());
        package.verified = record.verified;
        package.installed_at = Some(current_timestamp());
        records.push(record);
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

    let path = match package.path.as_deref() {
        Some(path) => PathBuf::from(path),
        None => store_path_for_digest(digest)?,
    };
    if path.exists() {
        let actual = compute_path_sha256(&path)?;
        if actual != *digest {
            bail!(
                "digest mismatch for existing store path {}: lock declares {}, computed {}",
                path.display(),
                digest,
                actual
            );
        }
        validate_package(package, &path)?;
        return Ok(StoreRecord {
            package_ref: package.package_ref.clone(),
            path,
            verified: true,
        });
    }

    std::fs::create_dir_all(&path)?;
    fetch_oci(oci_ref, digest, &path)?;
    let actual = compute_path_sha256(&path)?;
    if actual != *digest {
        let _ = std::fs::remove_dir_all(&path);
        bail!(
            "digest mismatch for {}: registry declared {}, fetched {}",
            package.package_ref,
            digest,
            actual
        );
    }
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

fn compute_path_sha256(path: &Path) -> Result<String> {
    if path.is_file() {
        return compute_file_sha256(path);
    }
    if path.is_dir() {
        let mut files = Vec::new();
        collect_files(path, path, &mut files)?;
        let mut content = String::new();
        for (relative, digest) in files {
            content.push_str(&digest);
            content.push_str("  ");
            content.push_str(&relative);
            content.push('\n');
        }
        return Ok(format!("sha256:{}", sha256_hex(content.as_bytes())));
    }
    bail!(
        "cannot compute digest for non-file/non-directory path {}",
        path.display()
    )
}

fn collect_files(root: &Path, path: &Path, files: &mut Vec<(String, String)>) -> Result<()> {
    let mut entries = std::fs::read_dir(path)
        .with_context(|| format!("reading {}", path.display()))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            collect_files(root, &path, files)?;
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            files.push((relative, compute_file_sha256(&path)?));
        }
    }
    Ok(())
}

fn compute_file_sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(format!("sha256:{}", sha256_hex(&bytes)))
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    hex::encode(digest)
}

fn current_timestamp() -> String {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => duration.as_secs().to_string(),
        Err(_) => "0".to_string(),
    }
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
    use super::{compute_path_sha256, materialize_package_lock, store_path_for_digest};
    use crate::armory_lock::{
        LockedPackage, LockedRegistry, LockedRoot, PackageLock, PACKAGE_LOCK_SCHEMA,
    };

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

    #[test]
    fn computes_file_sha256_digest() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("payload.txt");
        std::fs::write(&file, b"abc").expect("write payload");
        assert_eq!(
            compute_path_sha256(&file).expect("digest"),
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn computes_directory_digest_from_sorted_file_digests() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("b.txt"), b"b").expect("write b");
        std::fs::write(dir.path().join("a.txt"), b"a").expect("write a");
        let digest = compute_path_sha256(dir.path()).expect("digest");
        assert!(digest.starts_with("sha256:"));
    }

    #[test]
    fn materialization_updates_lock_entries() {
        let dir = tempfile::tempdir().expect("temp dir");
        let digest = compute_path_sha256(dir.path()).expect("digest");
        let mut lock = PackageLock {
            schema: PACKAGE_LOCK_SCHEMA.to_string(),
            registries: vec![LockedRegistry {
                name: "test".to_string(),
                url: "https://example.test/index.json".to_string(),
            }],
            roots: vec![LockedRoot {
                package_ref: "profile/root".to_string(),
            }],
            packages: vec![LockedPackage {
                package_ref: "profile/root".to_string(),
                version: Some("1.0.0".to_string()),
                registry: "test".to_string(),
                oci_ref: Some("oci://example/root".to_string()),
                digest: Some(digest),
                dependencies: Vec::new(),
                path: Some(dir.path().display().to_string()),
                verified: false,
                installed_at: None,
            }],
        };

        let records = materialize_package_lock(&mut lock).expect("materialize existing path");

        assert_eq!(records.len(), 1);
        assert!(lock.packages[0].verified);
        assert!(lock.packages[0].installed_at.is_some());
    }
}
