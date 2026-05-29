use anyhow::Result;

use crate::armory_lock;
use crate::armory_store;
use crate::config::Config;

pub fn refresh(config: &Config) -> Result<()> {
    armory_lock::refresh(config)
}

pub fn materialize(_config: &Config) -> Result<()> {
    let records = armory_store::materialize_lock()?;
    for record in records {
        println!(
            "{} -> {} ({})",
            record.package_ref,
            record.path.display(),
            if record.verified {
                "verified"
            } else {
                "unverified"
            }
        );
    }
    Ok(())
}
