use anyhow::Result;

use crate::armory_lock;

pub fn status() -> Result<()> {
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
