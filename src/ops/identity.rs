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
    let mut seed = deriver.derive(KeyPurpose::RnsSigning);
    let vk = ed25519_verifying_key(&seed);
    seed.zeroize();

    let pubkey_hex = hex::encode(vk.as_bytes());
    let digest = Sha256::digest(vk.as_bytes());
    let hash = hex::encode(&digest[..16]);
    (pubkey_hex, hash)
}

/// Identity hash: first 16 bytes (32 hex chars) of SHA-256(git-signing-pubkey).
/// Truncated for human readability while retaining 128 bits of collision resistance.
const IDENTITY_HASH_HEX_LEN: usize = 32;

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

/// Compute the identity hash from a root secret.
/// SHA-256(git-signing-Ed25519-pubkey), truncated to 32 hex chars.
fn identity_hash(root: &styrene_identity::signer::RootSecret) -> String {
    use sha2::{Digest, Sha256};

    let deriver = KeyDeriver::new(root.as_bytes());
    let mut seed = deriver.derive(KeyPurpose::GitSigning);
    let vk = ed25519_verifying_key(&seed);
    seed.zeroize();

    let digest = Sha256::digest(vk.as_bytes());
    hex::encode(&digest[..IDENTITY_HASH_HEX_LEN / 2])
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
pub fn run_link(url: String, code: Option<String>, path: Option<PathBuf>) -> Result<()> {
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
    drop(root);

    // Normalize the URL
    let base_url = url.trim_end_matches('/');

    if let Some(invite_code) = code {
        // With invite code: register directly via API — no browser needed
        output::status("linking identity to hub...");

        let body = serde_json::json!({
            "code": invite_code,
            "pubkey_hex": pubkey_hex,
        });

        let client = reqwest::blocking::Client::new();
        let resp = client
            .post(format!("{base_url}/enroll"))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .context("failed to connect to hub")?;

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
                console::style("You can now sign in to hub services with your mesh identity.").dim()
            );
        } else {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            let error: serde_json::Value =
                serde_json::from_str(&body).unwrap_or(serde_json::json!({"error": body}));
            let msg = error["error"].as_str().unwrap_or("unknown error");
            bail!("hub returned {status}: {msg}");
        }
    } else {
        // No invite code: open browser with pubkey for easy paste
        let enroll_url = format!(
            "{base_url}/admin#pubkey={}",
            pubkey_hex
        );

        eprintln!();
        output::status("opening hub in browser");
        eprintln!("  pubkey  {pubkey_hex}");
        eprintln!("  hash    {hash}");
        eprintln!();
        eprintln!(
            "  {}",
            console::style("Paste the pubkey above into the hub's admin UI to register.").dim()
        );
        eprintln!();

        // Try to open the browser
        if let Err(_) = open::that(&enroll_url) {
            eprintln!("  Could not open browser. Visit this URL manually:");
            eprintln!("  {enroll_url}");
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
    let hash = identity_hash(&root);

    let mut git_seed = deriver.derive(KeyPurpose::GitSigning);
    let git_vk = ed25519_verifying_key(&git_seed);
    git_seed.zeroize();

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
    eprintln!("  git key    {}", hex::encode(git_vk.as_bytes()));
    eprintln!("  ssh key    {}", hex::encode(ssh_vk.as_bytes()));
    eprintln!("  age key    {}", hex::encode(age_pk.as_bytes()));

    Ok(())
}
