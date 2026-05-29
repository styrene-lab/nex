use anyhow::Result;

use crate::armory_lock;
use crate::config::Config;

pub fn refresh(config: &Config) -> Result<()> {
    armory_lock::refresh(config)
}
