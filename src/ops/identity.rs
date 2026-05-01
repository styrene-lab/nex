use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use zeroize::{Zeroize, Zeroizing};

use styrene_identity::derive::{KeyDeriver, KeyPurpose};
use styrene_identity::export::AllPublicKeys;
use styrene_identity::file_signer::FileSigner;
use styrene_identity::identity::{identity_hash, identity_pubkey};
use styrene_identity::pubkey;
use styrene_identity::signer::SignerError;

use crate::output;

fn default_path() -> PathBuf {
    FileSigner::default_path()
}

/// Read a passphrase from the terminal (no echo). Rejects empty input.
/// Returns a `Zeroizing` wrapper that automatically clears memory on drop.
fn read_passphrase(prompt: &str) -> Result<Zeroizing<Vec<u8>>> {
    let passphrase = crate::input::input().password(prompt)?;
    if passphrase.is_empty() {
        bail!("passphrase must not be empty");
    }
    Ok(Zeroizing::new(passphrase.into_bytes()))
}

/// Load identity root secret with passphrase. The passphrase is wrapped in
/// `Zeroizing` so it is automatically cleared on drop — no manual zeroize needed.
fn load_root(
    path: &PathBuf,
    passphrase: &Zeroizing<Vec<u8>>,
) -> Result<styrene_identity::signer::RootSecret> {
    let signer = FileSigner::with_static_passphrase(path, passphrase);
    signer.load(passphrase).map_err(|e| match e {
        SignerError::DecryptionFailed(_) => anyhow::anyhow!("wrong passphrase"),
        e => anyhow::anyhow!("failed to load identity: {e}"),
    })
}

// ── init ────────────────────────────────────────────────────────────────────

pub fn run_init(path: Option<PathBuf>) -> Result<()> {
    let path = path.unwrap_or_else(default_path);

    if path.exists() {
        bail!(
            "identity already exists at {}\nuse `nex identity show` to inspect it",
            path.display()
        );
    }

    output::status("creating new Styrene identity");

    let passphrase = Zeroizing::new(
        crate::input::input()
            .password_with_confirm("Passphrase")?
            .into_bytes(),
    );

    if passphrase.is_empty() {
        bail!("passphrase must not be empty");
    }

    let signer = FileSigner::with_static_passphrase(&path, &passphrase);
    signer
        .generate(&passphrase)
        .context("failed to generate identity")?;

    let root = signer
        .load(&passphrase)
        .context("failed to load newly created identity")?;

    let hash = identity_hash(&root);

    eprintln!();
    output::status("identity created");
    eprintln!("  path  {}", path.display());
    eprintln!("  hash  {hash}");
    eprintln!();
    eprintln!(
        "  {}",
        console::style("Back up this file — losing it means losing your identity.").dim()
    );

    Ok(())
}

// ── show ────────────────────────────────────────────────────────────────────

pub fn run_show(path: Option<PathBuf>) -> Result<()> {
    let path = path.unwrap_or_else(default_path);

    if !path.exists() {
        bail!(
            "no identity at {}\nrun `nex identity init` to create one",
            path.display()
        );
    }

    let passphrase = read_passphrase("Passphrase")?;
    let root = load_root(&path, &passphrase)?;
    let keys = AllPublicKeys::from_root(&root);

    eprintln!();
    output::status("styrene identity");
    eprintln!("  path       {}", path.display());
    eprintln!("  hash       {}", keys.identity_hash);
    eprintln!("  pubkey     {}", keys.signing_pubkey_hex);
    eprintln!("  ssh host   {}", keys.ssh_host_fingerprint);
    eprintln!("  wireguard  {}", keys.wireguard_pubkey);
    if !keys.age_recipient.is_empty() {
        eprintln!("  age        {}", keys.age_recipient);
    }

    Ok(())
}

// ── list ────────────────────────────────────────────────────────────────────

pub fn run_list() -> Result<()> {
    eprintln!();
    output::status("styrene identities");

    let mut found = false;

    // Default path
    let default = default_path();
    if default.is_file() {
        print_identity_metadata(&default, "default");
        found = true;
    }

    // Scan for additional .key files in ~/.config/styrene/
    if let Some(parent) = default.parent() {
        if parent.is_dir() {
            let mut extras: Vec<_> = std::fs::read_dir(parent)?
                .flatten()
                .filter(|e| {
                    let p = e.path();
                    p.extension().and_then(|x| x.to_str()) == Some("key") && p != default
                })
                .map(|e| e.path())
                .collect();
            extras.sort();
            for extra in &extras {
                let label = extra
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown");
                print_identity_metadata(extra, label);
                found = true;
            }
        }
    }

    // STYRENE_IDENTITY_PATH env var
    if let Ok(env_path) = std::env::var("STYRENE_IDENTITY_PATH") {
        let p = PathBuf::from(&env_path);
        if p.is_file() && p != default {
            print_identity_metadata(&p, "env:STYRENE_IDENTITY_PATH");
            found = true;
        }
    }

    // STYRENE_IDENTITY_HASH env var (hash-only)
    if let Ok(hash) = std::env::var("STYRENE_IDENTITY_HASH") {
        if !hash.is_empty() {
            eprintln!(
                "  {} {}  {}  {}",
                console::style("○").dim(),
                console::style("env").bold(),
                hash,
                console::style("(hash-only, no signing)").dim()
            );
            found = true;
        }
    }

    if !found {
        eprintln!(
            "  {} no identities found — run {}",
            console::style("○").dim(),
            console::style("nex identity init").bold()
        );
    }

    eprintln!();
    Ok(())
}

fn print_identity_metadata(path: &PathBuf, label: &str) {
    let meta = std::fs::metadata(path);
    let size_ok = meta.as_ref().map(|m| m.len() == 97).unwrap_or(false);

    #[cfg(unix)]
    let perms_ok = meta
        .as_ref()
        .map(|m| {
            use std::os::unix::fs::PermissionsExt;
            m.permissions().mode() & 0o777 == 0o600
        })
        .unwrap_or(false);
    #[cfg(not(unix))]
    let perms_ok = true;

    let status = if size_ok && perms_ok {
        console::style("●").green().to_string()
    } else {
        console::style("●").yellow().to_string()
    };

    eprintln!(
        "  {status} {}  {}",
        console::style(label).bold(),
        console::style(path.display()).dim()
    );

    if !size_ok {
        if let Ok(m) = &meta {
            eprintln!(
                "    {}",
                console::style(format!("unexpected size: {} bytes (expected 97)", m.len()))
                    .yellow()
            );
        }
    }
    #[cfg(unix)]
    if !perms_ok {
        if let Ok(m) = &meta {
            use std::os::unix::fs::PermissionsExt;
            eprintln!(
                "    {}",
                console::style(format!(
                    "permissions: {:#o} (should be 0o600)",
                    m.permissions().mode() & 0o777
                ))
                .yellow()
            );
        }
    }
}

// ── ssh ─────────────────────────────────────────────────────────────────────

pub fn run_ssh(label: Option<String>, list: bool, add: Option<String>) -> Result<()> {
    if list {
        return run_ssh_list();
    }
    if let Some(new_label) = add {
        return run_ssh_add(&new_label);
    }
    if let Some(label) = label {
        return run_ssh_export(&label);
    }
    bail!(
        "usage: nex identity ssh <label>\n       \
         nex identity ssh --list\n       \
         nex identity ssh --add <label>"
    );
}

fn run_ssh_export(label: &str) -> Result<()> {
    let path = default_path();
    if !path.exists() {
        bail!("no identity — run `nex identity init` first");
    }

    let passphrase = read_passphrase("Passphrase")?;
    let root = load_root(&path, &passphrase)?;
    let deriver = KeyDeriver::new(root.as_bytes());

    let mut seed = deriver
        .derive_ssh_user_key(label)
        .context("invalid SSH key label")?;
    let comment = format!("styrene-ssh-user-{label}");
    let pubkey = styrene_identity::format::ssh_pubkey(&seed, &comment);
    let fingerprint = styrene_identity::format::ssh_pubkey_fingerprint(&seed);
    seed.zeroize();

    eprintln!();
    output::status(&format!("ssh key: {label}"));
    eprintln!("  fingerprint  {fingerprint}");
    eprintln!();

    // Pubkey to stdout for piping
    print!("{pubkey}");

    eprintln!();
    eprintln!(
        "  {}",
        console::style(
            "Paste this key into your SSH provider (e.g. GitHub → Settings → SSH keys)."
        )
        .dim()
    );

    Ok(())
}

fn run_ssh_list() -> Result<()> {
    let id_config = crate::config::load_identity_config()?;
    let labels = id_config.ssh.and_then(|s| s.labels).unwrap_or_default();

    if labels.is_empty() {
        eprintln!();
        eprintln!(
            "  no SSH labels registered — try {}",
            console::style("nex identity ssh --add github").bold()
        );
        eprintln!();
        return Ok(());
    }

    let path = default_path();
    if !path.exists() {
        bail!("no identity — run `nex identity init` first");
    }

    let passphrase = read_passphrase("Passphrase")?;
    let root = load_root(&path, &passphrase)?;
    let deriver = KeyDeriver::new(root.as_bytes());

    eprintln!();
    output::status("ssh keys");
    for label in &labels {
        let mut seed = deriver
            .derive_ssh_user_key(label)
            .context("invalid SSH key label")?;
        let fingerprint = styrene_identity::format::ssh_pubkey_fingerprint(&seed);
        seed.zeroize();
        eprintln!("  {}  {fingerprint}", console::style(label).bold());
    }
    eprintln!();

    Ok(())
}

fn run_ssh_add(label: &str) -> Result<()> {
    crate::config::append_to_list("identity.ssh.labels", label)?;
    eprintln!(
        "  {} registered SSH label: {}",
        console::style("✓").green().bold(),
        console::style(label).bold()
    );
    // Show the key immediately
    run_ssh_export(label)
}

// ── git ─────────────────────────────────────────────────────────────────────

pub fn run_git(show: bool) -> Result<()> {
    if show {
        return run_git_show();
    }
    run_git_configure()
}

fn run_git_configure() -> Result<()> {
    let id_config = crate::config::load_identity_config()?;
    let git_config = id_config.git.unwrap_or_default();

    let name = match git_config.name {
        Some(n) => n,
        None => {
            let input = crate::input::input().input_text("Full name for git commits", None)?;
            crate::config::set_nested_preference(
                "identity.git.name",
                toml::Value::String(input.clone()),
            )?;
            input
        }
    };

    let email = match git_config.email {
        Some(e) => e,
        None => {
            let input = crate::input::input().input_text("Email for git commits", None)?;
            crate::config::set_nested_preference(
                "identity.git.email",
                toml::Value::String(input.clone()),
            )?;
            input
        }
    };

    let path = default_path();
    if !path.exists() {
        bail!("no identity — run `nex identity init` first");
    }

    let passphrase = read_passphrase("Passphrase")?;
    let root = load_root(&path, &passphrase)?;
    let deriver = KeyDeriver::new(root.as_bytes());

    let mut seed = deriver.derive(KeyPurpose::Signing);
    let config_snippet = styrene_identity::format::git_signing_config(&seed);
    seed.zeroize();

    // Extract the signingkey value from the config snippet
    let signing_key = config_snippet
        .lines()
        .find(|l| l.contains("signingkey"))
        .and_then(|l| l.split("= ").nth(1))
        .context("failed to parse signing key from config")?;

    // Apply via git config --global
    let git = |args: &[&str]| -> Result<()> {
        let status = std::process::Command::new("git")
            .args(args)
            .status()
            .context("failed to run git")?;
        if !status.success() {
            bail!("git config failed");
        }
        Ok(())
    };

    git(&["config", "--global", "user.name", &name])?;
    git(&["config", "--global", "user.email", &email])?;
    git(&["config", "--global", "gpg.format", "ssh"])?;
    git(&["config", "--global", "user.signingkey", signing_key])?;
    git(&["config", "--global", "commit.gpgsign", "true"])?;

    eprintln!();
    output::status("git signing configured");
    eprintln!("  name     {name}");
    eprintln!("  email    {email}");
    eprintln!("  signing  enabled (SSH key)");
    eprintln!();

    Ok(())
}

fn run_git_show() -> Result<()> {
    let get = |key: &str| -> String {
        std::process::Command::new("git")
            .args(["config", "--global", key])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|| console::style("(not set)").dim().to_string())
    };

    eprintln!();
    output::status("git signing config");
    eprintln!("  user.name       {}", get("user.name"));
    eprintln!("  user.email      {}", get("user.email"));
    eprintln!("  gpg.format      {}", get("gpg.format"));
    eprintln!("  user.signingkey {}", get("user.signingkey"));
    eprintln!("  commit.gpgsign  {}", get("commit.gpgsign"));
    eprintln!();

    Ok(())
}

// ── wg ─────────────────────────────────────────────────────────────────────

pub fn run_wg() -> Result<()> {
    let path = default_path();
    if !path.exists() {
        bail!("no identity — run `nex identity init` first");
    }

    let passphrase = read_passphrase("Passphrase")?;
    let root = load_root(&path, &passphrase)?;
    let deriver = KeyDeriver::new(root.as_bytes());

    let mut seed = deriver.derive(KeyPurpose::WireGuard);
    let privkey = styrene_identity::format::wireguard_privkey(&seed);
    let pubkey = styrene_identity::format::wireguard_pubkey(&seed);
    seed.zeroize();

    eprintln!();
    output::status("wireguard key pair");
    eprintln!("  privkey  {privkey}");
    eprintln!();

    // Pubkey to stdout for piping
    print!("{pubkey}");

    eprintln!();
    eprintln!(
        "  {}",
        console::style(
            "Public key printed to stdout (pipeable). Private key shown above on stderr."
        )
        .dim()
    );

    Ok(())
}

// ── age ────────────────────────────────────────────────────────────────────

pub fn run_age() -> Result<()> {
    let path = default_path();
    if !path.exists() {
        bail!("no identity — run `nex identity init` first");
    }

    let passphrase = read_passphrase("Passphrase")?;
    let root = load_root(&path, &passphrase)?;
    let deriver = KeyDeriver::new(root.as_bytes());

    let mut seed = deriver.derive(KeyPurpose::Age);
    let pk = pubkey::x25519_public_key(&seed);

    eprintln!();
    output::status("age encryption key pair");
    eprintln!("  identity   {}", hex::encode(seed));
    eprintln!();

    // Recipient (public) to stdout for piping
    print!("{}", hex::encode(pk.as_bytes()));

    seed.zeroize();

    eprintln!();
    eprintln!(
        "  {}",
        console::style("Recipient (public) printed to stdout (pipeable). Identity (secret) shown above on stderr.").dim()
    );
    eprintln!(
        "  {}",
        console::style("Note: enable the age-format feature for Bech32-encoded AGE-SECRET-KEY-1.../age1... format.").dim()
    );

    Ok(())
}

// ── link ────────────────────────────────────────────────────────────────────

pub fn run_link(url: &str, code: Option<&str>, path: Option<PathBuf>) -> Result<()> {
    let path = path.unwrap_or_else(default_path);

    if !path.exists() {
        bail!(
            "no identity at {}\nrun `nex identity init` to create one",
            path.display()
        );
    }

    let passphrase = read_passphrase("Passphrase")?;
    let root = load_root(&path, &passphrase)?;

    let hash = identity_hash(&root);
    let pubkey_hex = hex::encode(identity_pubkey(&root));
    drop(root);

    let base_url = url.trim_end_matches('/');

    if let Some(invite_code) = code {
        output::status("linking identity to hub...");

        let body = serde_json::json!({
            "code": invite_code,
            "pubkey_hex": pubkey_hex,
        });

        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("failed to build HTTP client")?;

        let resp = client
            .post(format!("{base_url}/enroll"))
            .json(&body)
            .send()
            .context("failed to connect to hub — check the URL and your network")?;

        if resp.status().is_success() {
            let data: serde_json::Value = resp.json().context("invalid response")?;
            let username = data["username"].as_str().unwrap_or("unknown");

            eprintln!();
            output::status("identity linked");
            eprintln!("  hub       {base_url}");
            eprintln!("  username  {username}");
            eprintln!("  hash      {hash}");
            eprintln!();
            eprintln!(
                "  {}",
                console::style("You can now sign in to hub services with your mesh identity.")
                    .dim()
            );
        } else {
            let status = resp.status();
            let body = resp.text().context("failed to read error response")?;
            let error: serde_json::Value =
                serde_json::from_str(&body).unwrap_or(serde_json::json!({"error": body}));
            let msg = error["error"].as_str().unwrap_or("unknown error");
            bail!("hub returned {status}: {msg}");
        }
    } else {
        eprintln!();
        output::status("identity ready to link");
        eprintln!("  hub       {base_url}");
        eprintln!("  pubkey    {pubkey_hex}");
        eprintln!("  hash      {hash}");
        eprintln!();
        eprintln!(
            "  {}",
            console::style("Paste the pubkey into the hub's admin UI (Existing Identity tab),")
                .dim()
        );
        eprintln!(
            "  {}",
            console::style(
                "or ask an admin for an invite code: nex identity link <url> --code <CODE>"
            )
            .dim()
        );
        eprintln!();

        let admin_url = format!("{base_url}/admin");
        if open::that(&admin_url).is_err() {
            eprintln!("  Open manually: {admin_url}");
        }
    }

    Ok(())
}
