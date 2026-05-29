use anyhow::Result;

use crate::armory_lock;

pub fn status() -> Result<()> {
    let lock_path = armory_lock::package_lock_path()?;
    if !lock_path.exists() {
        println!("Armory package lock: absent");
        println!("roots: 0");
        println!("materialized packages: 0");
        return Ok(());
    }

    let lock = armory_lock::read_package_lock()?;
    println!("schema: {}", lock.schema);
    println!("registries:");
    for registry in &lock.registries {
        println!("  - {} ({})", registry.name, registry.url);
    }
    println!("roots:");
    for root in &lock.roots {
        println!("  - {}", root.package_ref);
    }
    println!("packages:");
    for package in &lock.packages {
        let state = if package.verified && package.path.is_some() {
            "installed"
        } else {
            "pending"
        };
        println!("  - {} [{}]", package.package_ref, state);
        if let Some(version) = &package.version {
            println!("      version: {version}");
        }
        if let Some(digest) = &package.digest {
            println!("      digest: {digest}");
        }
        if let Some(path) = &package.path {
            println!("      path: {path}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_treats_absent_lock_as_empty_state() {
        let dir = tempfile::tempdir().expect("tempdir");
        let old_home = std::env::var_os("HOME");
        std::env::set_var("HOME", dir.path());

        let result = status();

        match old_home {
            Some(home) => std::env::set_var("HOME", home),
            None => std::env::remove_var("HOME"),
        }

        result.expect("absent package lock should be a valid empty status");
    }
}
