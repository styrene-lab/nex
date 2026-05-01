//! RBAC roster management — sync hub-signed entries from a Signum hub.

use std::path::PathBuf;

use anyhow::{bail, Context, Result};

use crate::output;

/// A hub-signed roster entry (matches Signum /api/roster response).
#[derive(Debug, serde::Deserialize)]
struct SignedRosterEntry {
    entry: RosterEntryPayload,
    hub_hash: String,
    hub_pubkey: String,
    signature: String,
    issued_at: i64,
    expires_at: i64,
}

#[derive(Debug, serde::Deserialize)]
struct RosterEntryPayload {
    identity_hash: String,
    role: String,
    label: String,
    #[serde(default)]
    grants: Vec<String>,
}

/// Run `nex rbac sync <hub-url>`.
pub fn run_sync(
    hub_url: &str,
    identity: Option<&str>,
    token: Option<&str>,
    output_path: Option<PathBuf>,
) -> Result<()> {
    let base_url = hub_url.trim_end_matches('/');

    output::status(&format!("syncing RBAC roster from {base_url}"));

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .context("failed to build HTTP client")?;

    let entries: Vec<SignedRosterEntry> = if let Some(hash) = identity {
        // Single identity fetch (public endpoint)
        let url = format!("{base_url}/api/roster/{hash}");
        let resp = client
            .get(&url)
            .send()
            .context("failed to connect to hub")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            bail!("hub returned {status}: {body}");
        }
        let entry: SignedRosterEntry = resp.json().context("invalid response")?;
        vec![entry]
    } else {
        // Full roster (admin token required)
        let Some(admin_token) = token else {
            bail!(
                "full roster sync requires --token or SIGNUM_ADMIN_TOKEN\n\
                 use --identity <hash> for single-identity fetch (no token needed)"
            );
        };
        let url = format!("{base_url}/api/roster");
        let resp = client
            .get(&url)
            .header("Authorization", format!("Bearer {admin_token}"))
            .send()
            .context("failed to connect to hub")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            bail!("hub returned {status}: {body}");
        }
        resp.json().context("invalid response")?
    };

    if entries.is_empty() {
        output::status("no roster entries returned from hub");
        return Ok(());
    }

    // Extract hub info for the trusted_hubs section
    let hub_hash = entries[0].hub_hash.clone();
    let hub_pubkey = entries[0].hub_pubkey.clone();

    // Build TOML config snippet
    let mut toml_lines = Vec::new();
    toml_lines.push(String::new());
    toml_lines.push("# Hub-signed RBAC roster entries (synced from Signum hub)".into());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    toml_lines.push(format!("# Synced at: {now} (unix)"));
    toml_lines.push(String::new());
    toml_lines.push("[[rbac.trusted_hubs]]".into());
    toml_lines.push(format!(r#"hub_hash = "{hub_hash}""#));
    toml_lines.push(format!(r#"hub_pubkey = "{hub_pubkey}""#));
    toml_lines.push(format!(r#"label = "{base_url}""#));

    for entry in &entries {
        toml_lines.push(String::new());
        toml_lines.push("[[rbac.hub_entries]]".into());
        toml_lines.push(format!(r#"hub_hash = "{hub_hash}""#));
        toml_lines.push(format!(r#"hub_pubkey = "{hub_pubkey}""#));
        toml_lines.push(format!(r#"signature = "{}""#, entry.signature));
        toml_lines.push(format!("issued_at = {}", entry.issued_at));
        toml_lines.push(format!("expires_at = {}", entry.expires_at));
        toml_lines.push(String::new());
        toml_lines.push("[rbac.hub_entries.entry]".into());
        toml_lines.push(format!(
            r#"identity_hash = "{}""#,
            entry.entry.identity_hash
        ));
        toml_lines.push(format!(r#"role = "{}""#, entry.entry.role));
        toml_lines.push(format!(r#"label = "{}""#, entry.entry.label));
        if !entry.entry.grants.is_empty() {
            let grants_str = entry
                .entry
                .grants
                .iter()
                .map(|g| format!(r#""{g}""#))
                .collect::<Vec<_>>()
                .join(", ");
            toml_lines.push(format!("grants = [{grants_str}]"));
        }
    }

    let toml_snippet = toml_lines.join("\n");

    // Write to file or print to stdout
    let out_path = output_path.unwrap_or_else(default_config_path);
    if out_path.as_os_str() == "-" {
        // Print to stdout for piping
        println!("{toml_snippet}");
    } else {
        // Append to existing config (or create)
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&out_path)
            .context(format!("failed to open {}", out_path.display()))?;
        writeln!(file, "{toml_snippet}")
            .context(format!("failed to write to {}", out_path.display()))?;

        output::status(&format!(
            "wrote {} roster entries to {}",
            entries.len(),
            out_path.display()
        ));
    }

    for entry in &entries {
        eprintln!(
            "  {} {} ({})",
            console::style(&entry.entry.identity_hash[..12]).dim(),
            entry.entry.role,
            entry.entry.label,
        );
    }

    Ok(())
}

fn default_config_path() -> PathBuf {
    let config_dir = std::env::var("STYRENE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("styrene")
        });
    config_dir.join("config.toml")
}
