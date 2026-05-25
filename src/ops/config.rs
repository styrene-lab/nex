use std::path::Path;

use anyhow::{bail, Context, Result};

pub fn run_export(format: &str, output: Option<&Path>) -> Result<()> {
    let rendered = match format {
        "toml" => crate::config::export_config_toml()?,
        other => bail!("unsupported config export format '{other}'; supported: toml"),
    };

    if let Some(output) = output {
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        crate::edit::atomic_write_bytes(output, rendered.as_bytes())
            .with_context(|| format!("writing {}", output.display()))?;
    } else {
        print!("{rendered}");
    }
    Ok(())
}

pub fn run_migrate(keep_toml: bool) -> Result<()> {
    let written = crate::config::migrate_to_pkl(keep_toml)?;
    eprintln!("migrated local Nex config to {}", written.display());
    if keep_toml {
        eprintln!("regenerated compatibility TOML from canonical Pkl");
    }
    Ok(())
}
