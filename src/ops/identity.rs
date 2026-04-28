use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use zeroize::Zeroize;

use styrene_identity::derive::{KeyDeriver, KeyPurpose};
use styrene_identity::file_signer::{ClosurePassphraseProvider, FileSigner};
use styrene_identity::pubkey::{ed25519_verifying_key, x25519_public_key};
use styrene_identity::signer::SignerError;

use crate::output;

/// Compute the RNS signing pubkey (hex) and Signum-compatible identity hash.
/// Signum derives identity from: HKDF("styrene-rns-signing-v1") → Ed25519 pubkey → SHA-256[:16]
fn signum_identity(root: &styrene_identity::signer::RootSecret) -> (String, String) {
    use sha2::{Digest, Sha256};

    let deriver = KeyDeriver::new(root.as_bytes());
    let mut seed = deriver.derive(KeyPurpose::Signing);
    let vk = ed25519_verifying_key(&seed);
    seed.zeroize();

    let pubkey_hex = hex::encode(vk.as_bytes());
    let digest = Sha256::digest(vk.as_bytes());
    let hash = hex::encode(&digest[..16]);
    (pubkey_hex, hash)
}

fn default_path() -> PathBuf {
    FileSigner::default_path()
}

/// Read a passphrase from the terminal (no echo). Rejects empty input.
fn read_passphrase(prompt: &str) -> Result<Vec<u8>> {
    let passphrase = dialoguer::Password::new()
        .with_prompt(prompt)
        .interact()
        .context("failed to read passphrase")?;
    if passphrase.is_empty() {
        bail!("passphrase must not be empty");
    }
    Ok(passphrase.into_bytes())
}

/// Build a FileSigner that uses an already-collected passphrase.
fn signer_with_passphrase(path: &PathBuf, passphrase: &[u8]) -> FileSigner {
    let pp = passphrase.to_vec();
    let provider = Box::new(ClosurePassphraseProvider::new(move || Ok(pp.clone())));
    FileSigner::new(path, provider)
}

/// Compute the canonical identity hash from a root secret.
/// SHA-256(RNS-signing-Ed25519-pubkey)[:16], 32 hex chars.
///
/// The RNS signing key is the canonical identity — it's the mesh identity
/// used by Signum, styrened, and all mesh operations. Git signing, SSH,
/// and other keys derive from the same root but are purpose-specific.
fn identity_hash(root: &styrene_identity::signer::RootSecret) -> String {
    let (_, hash) = signum_identity(root);
    hash
}

/// `nex identity init` — generate a new StyreneIdentity.
pub fn run_init(path: Option<PathBuf>) -> Result<()> {
    let path = path.unwrap_or_else(default_path);

    if path.exists() {
        bail!(
            "identity already exists at {}\nuse `nex identity show` to inspect it",
            path.display()
        );
    }

    output::status("creating new Styrene identity");

    let mut passphrase = dialoguer::Password::new()
        .with_prompt("Passphrase")
        .with_confirmation("Confirm passphrase", "Passphrases do not match")
        .interact()
        .context("failed to read passphrase")?
        .into_bytes();

    if passphrase.is_empty() {
        bail!("passphrase must not be empty");
    }

    let signer = signer_with_passphrase(&path, &passphrase);
    signer
        .generate(&passphrase)
        .context("failed to generate identity")?;

    let root = signer
        .load(&passphrase)
        .context("failed to load newly created identity")?;
    passphrase.zeroize();

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

/// `nex identity link` — link this identity to a Signum hub.
pub fn run_link(url: &str, code: Option<&str>, path: Option<PathBuf>) -> Result<()> {
    let path = path.unwrap_or_else(default_path);

    if !path.exists() {
        bail!(
            "no identity at {}\nrun `nex identity init` to create one",
            path.display()
        );
    }

    let mut passphrase = read_passphrase("Passphrase")?;
    let signer = signer_with_passphrase(&path, &passphrase);

    let root = match signer.load(&passphrase) {
        Ok(r) => {
            passphrase.zeroize();
            r
        }
        Err(SignerError::DecryptionFailed(_)) => {
            passphrase.zeroize();
            bail!("wrong passphrase");
        }
        Err(e) => {
            passphrase.zeroize();
            bail!("failed to load identity: {e}");
        }
    };

    let (pubkey_hex, hash) = signum_identity(&root);
    // RootSecret implements Drop+Zeroize via the styrene-identity crate.
    // Explicit drop here ends the borrow; zeroization happens in Drop.
    drop(root);

    let base_url = url.trim_end_matches('/');

    if let Some(invite_code) = code {
        // With invite code: register directly via API — no browser needed.
        // NOTE: The invite code is visible in shell history and process listing.
        // For sensitive environments, pipe it: echo <code> | xargs -I{} nex identity link <url> --code {}
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
        // No invite code: display pubkey for the user to register via admin UI.
        // Open the hub's admin page in the browser for convenience.
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

/// `nex identity show` — display identity hash and derived public keys.
pub fn run_show(path: Option<PathBuf>) -> Result<()> {
    let path = path.unwrap_or_else(default_path);

    if !path.exists() {
        bail!(
            "no identity at {}\nrun `nex identity init` to create one",
            path.display()
        );
    }

    let mut passphrase = read_passphrase("Passphrase")?;
    let signer = signer_with_passphrase(&path, &passphrase);

    let root = match signer.load(&passphrase) {
        Ok(r) => {
            passphrase.zeroize();
            r
        }
        Err(SignerError::DecryptionFailed(_)) => {
            passphrase.zeroize();
            bail!("wrong passphrase");
        }
        Err(e) => {
            passphrase.zeroize();
            bail!("failed to load identity: {e}");
        }
    };

    let deriver = KeyDeriver::new(root.as_bytes());
    let (pubkey_hex, hash) = signum_identity(&root);

    let mut ssh_seed = deriver.derive(KeyPurpose::SshHost);
    let ssh_vk = ed25519_verifying_key(&ssh_seed);
    ssh_seed.zeroize();

    let mut age_secret = deriver.derive(KeyPurpose::Age);
    let age_pk = x25519_public_key(&age_secret);
    age_secret.zeroize();

    eprintln!();
    output::status("styrene identity");
    eprintln!("  path       {}", path.display());
    eprintln!("  hash       {hash}");
    eprintln!("  pubkey     {pubkey_hex}");
    eprintln!("  ssh host   {}", hex::encode(ssh_vk.as_bytes()));
    eprintln!("  age key    {}", hex::encode(age_pk.as_bytes()));

    Ok(())
}
