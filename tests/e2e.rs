//! End-to-end tests for nex — runs the compiled binary in a sandboxed environment.
//!
//! Each test gets its own TempDir as HOME with a pre-scaffolded nix config repo
//! and mock binaries. Uses `assert_cmd` to invoke the binary and `predicates`
//! to check output.
//!
//! Environment variables control user input (no interactive prompts):
//! - NEX_TEST_PASSPHRASE: bypass password prompts
//! - NEX_TEST_CONFIRM: bypass confirm prompts (y/n)
//! - NEX_TEST_INPUT: bypass text input prompts

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

// ── Sandbox ─────────────────────────────────────────────────────────────────

struct Sandbox {
    home: TempDir,
    repo: PathBuf,
    mocks: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let home = TempDir::new().expect("create tempdir");
        let repo = scaffold_repo(home.path());
        let mocks = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/integration/mocks");
        Self { home, repo, mocks }
    }

    fn with_identity(self) -> Self {
        let key_dir = self.home.path().join(".config/styrene");
        fs::create_dir_all(&key_dir).expect("create styrene config dir");
        let signer = styrene_identity::file_signer::FileSigner::with_static_passphrase(
            key_dir.join("identity.key"),
            b"testpass",
        );
        signer.generate(b"testpass").expect("generate identity");
        self
    }

    fn with_config(self) -> Self {
        let config_dir = self.home.path().join(".config/nex");
        fs::create_dir_all(&config_dir).expect("create nex config dir");
        fs::write(
            config_dir.join("config.toml"),
            format!(
                "repo_path = \"{}\"\nhostname = \"test-host\"\n",
                self.repo.display()
            ),
        )
        .expect("write config");
        self
    }

    fn identity_path(&self) -> PathBuf {
        self.home.path().join(".config/styrene/identity.key")
    }

    fn nex(&self) -> Command {
        let mut cmd = Command::cargo_bin("nex").expect("find nex binary");
        cmd.env("HOME", self.home.path())
            .env("NEX_REPO", &self.repo)
            .env("NEX_HOSTNAME", "test-host")
            .env("NEX_TEST_PASSPHRASE", "testpass")
            .env("NEX_TEST_CONFIRM", "y")
            .env("NEX_TEST_INPUT", "test-value");
        // Add mocks to PATH if they exist
        if self.mocks.exists() {
            let path = format!(
                "{}:{}",
                self.mocks.display(),
                std::env::var("PATH").unwrap_or_default()
            );
            cmd.env("PATH", path);
        }
        cmd
    }
}

fn scaffold_repo(home: &Path) -> PathBuf {
    let repo = home.join("nix-config");
    let home_dir = repo.join("nix/modules/home");
    let darwin_dir = repo.join("nix/modules/darwin");
    fs::create_dir_all(&home_dir).expect("create home dir");
    fs::create_dir_all(&darwin_dir).expect("create darwin dir");

    // Minimal base.nix with the expected pattern
    fs::write(
        home_dir.join("base.nix"),
        r#"{ pkgs, ... }:

{
  home = {
    username = "testuser";
    homeDirectory = "/home/testuser";
    stateVersion = "25.05";
    sessionPath = [ "$HOME/.local/bin" ];
  };

  home.packages = with pkgs; [
    git
    vim
  ];

  programs.home-manager.enable = true;
}
"#,
    )
    .expect("write base.nix");

    // Minimal homebrew.nix
    fs::write(
        darwin_dir.join("homebrew.nix"),
        r#"{ ... }:

{
  homebrew = {
    enable = true;
    onActivation.cleanup = "zap";

    brews = [
      "wget"
    ];

    casks = [
      "firefox"
    ];
  };
}
"#,
    )
    .expect("write homebrew.nix");

    // Minimal darwin/base.nix (doctor checks unfree here)
    fs::write(
        darwin_dir.join("base.nix"),
        "{ ... }:\n{\n  nix.enable = false;\n  nixpkgs.config.allowUnfree = true;\n}\n",
    )
    .expect("write darwin base.nix");

    // Minimal flake.nix (doctor checks for mac-app-util in this file)
    fs::write(
        repo.join("flake.nix"),
        r#"{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    home-manager.url = "github:nix-community/home-manager";
    mac-app-util.url = "github:hraban/mac-app-util";
  };
  outputs = { nixpkgs, home-manager, mac-app-util }: {};
}
"#,
    )
    .expect("write flake.nix");

    // git init + commit
    let git = |args: &[&str]| {
        StdCommand::new("git")
            .args(args)
            .current_dir(&repo)
            .env("GIT_AUTHOR_NAME", "test")
            .env("GIT_AUTHOR_EMAIL", "test@test.com")
            .env("GIT_COMMITTER_NAME", "test")
            .env("GIT_COMMITTER_EMAIL", "test@test.com")
            .output()
            .expect("git command");
    };
    git(&["init"]);
    git(&["add", "-A"]);
    git(&["commit", "-m", "scaffold"]);

    repo
}

// ── Identity tests ──────────────────────────────────────────────────────────

#[test]
fn identity_init_creates_key_file() {
    let sb = Sandbox::new();
    assert!(!sb.identity_path().exists());

    sb.nex()
        .args(["identity", "init"])
        .assert()
        .success()
        .stderr(predicate::str::contains("identity created"));

    assert!(sb.identity_path().exists());
    // File should be 97 bytes (STID v1 format)
    let meta = fs::metadata(sb.identity_path()).expect("read metadata");
    assert_eq!(meta.len(), 97);
}

#[test]
fn identity_init_refuses_overwrite() {
    let sb = Sandbox::new().with_identity();
    assert!(sb.identity_path().exists());

    sb.nex()
        .args(["identity", "init"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn identity_show_displays_hash() {
    let sb = Sandbox::new().with_identity();

    sb.nex()
        .args(["identity", "show"])
        .assert()
        .success()
        .stderr(predicate::str::contains("hash"))
        .stderr(predicate::str::contains("pubkey"))
        .stderr(predicate::str::contains("ssh host"))
        .stderr(predicate::str::contains("wireguard"))
        .stderr(predicate::str::contains("age"));
}

#[test]
fn identity_list_finds_default() {
    let sb = Sandbox::new().with_identity();

    sb.nex()
        .args(["identity", "list"])
        .assert()
        .success()
        .stderr(predicate::str::contains("default"));
}

#[test]
fn identity_list_empty_when_no_identity() {
    let sb = Sandbox::new();

    sb.nex()
        .args(["identity", "list"])
        .assert()
        .success()
        .stderr(predicate::str::contains("no identities found"));
}

#[test]
fn identity_ssh_exports_pubkey() {
    let sb = Sandbox::new().with_identity();

    sb.nex()
        .args(["identity", "ssh", "github"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("ssh-ed25519 "));
}

#[test]
fn identity_ssh_add_registers_label() {
    let sb = Sandbox::new().with_identity().with_config();

    sb.nex()
        .args(["identity", "ssh", "--add", "github"])
        .assert()
        .success()
        .stderr(predicate::str::contains("registered SSH label"))
        .stdout(predicate::str::starts_with("ssh-ed25519 "));

    // Check config was updated
    let config =
        fs::read_to_string(sb.home.path().join(".config/nex/config.toml")).expect("read config");
    assert!(
        config.contains("github"),
        "config should contain github label"
    );
}

#[test]
fn identity_git_show_works() {
    let sb = Sandbox::new();

    sb.nex()
        .args(["identity", "git", "--show"])
        .assert()
        .success()
        .stderr(predicate::str::contains("git signing config"));
}

// ── Profile tests ───────────────────────────────────────────────────────────

#[test]
fn profile_sign_creates_signed_toml() {
    let sb = Sandbox::new().with_identity();

    let profile_path = sb.home.path().join("test-profile.toml");
    fs::write(
        &profile_path,
        "[meta]\nname = \"test\"\n\n[packages]\nnix = [\"git\"]\n",
    )
    .expect("write profile");

    // Run from home dir so the signed output lands there
    sb.nex()
        .args(["profile", "sign", profile_path.to_str().unwrap()])
        .current_dir(sb.home.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("profile signed"));
}

#[test]
fn profile_verify_rejects_missing_file() {
    let sb = Sandbox::new();

    sb.nex()
        .args(["profile", "verify", "nonexistent.toml"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("file not found"));
}

#[test]
fn profile_apply_hint_in_help() {
    let sb = Sandbox::new();

    sb.nex()
        .args(["profile", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("renamed to"))
        .stdout(predicate::str::contains("nex profile apply"));
}

// ── Install tests ───────────────────────────────────────────────────────────

#[test]
fn install_nix_adds_to_file() {
    let sb = Sandbox::new().with_config();

    sb.nex()
        .args(["install", "--nix", "ripgrep", "--dry-run"])
        .assert()
        .success()
        .stderr(predicate::str::contains("would add ripgrep"));
}

#[test]
fn list_shows_packages() {
    let sb = Sandbox::new().with_config();

    sb.nex()
        .args(["list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("git"))
        .stdout(predicate::str::contains("vim"));
}

// ── Forge tests ─────────────────────────────────────────────────────────────

#[test]
fn forge_dry_run_shows_plan() {
    let sb = Sandbox::new();

    sb.nex()
        .args(["forge", "--dry-run", "--hostname", "test-node"])
        .assert()
        .success()
        .stderr(predicate::str::contains("would build installer"))
        .stderr(predicate::str::contains("test-node"));
}

#[test]
fn forge_arch_flag_case_insensitive() {
    let sb = Sandbox::new();

    sb.nex()
        .args(["forge", "--dry-run", "--arch", "AARCH64"])
        .assert()
        .success();

    sb.nex()
        .args(["forge", "--dry-run", "--arch", "ARM64"])
        .assert()
        .success();
}

#[test]
fn forge_unknown_arch_fails() {
    let sb = Sandbox::new();

    sb.nex()
        .args(["forge", "--dry-run", "--arch", "mips"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown architecture"));
}

// ── Doctor tests ────────────────────────────────────────────────────────────

#[test]
fn doctor_reports_missing_identity() {
    let sb = Sandbox::new().with_config();

    sb.nex()
        .args(["doctor"])
        .assert()
        .success()
        .stderr(predicate::str::contains("no identity file"));
}

#[test]
fn doctor_reports_identity_present() {
    let sb = Sandbox::new().with_identity().with_config();

    sb.nex()
        .args(["doctor"])
        .assert()
        .success()
        .stderr(predicate::str::contains("identity.key"));
}

// ── Config tests ────────────────────────────────────────────────────────────

#[test]
fn help_shows_version() {
    Command::cargo_bin("nex")
        .expect("find binary")
        .args(["--version"])
        .assert()
        .success()
        .stdout(predicate::str::contains("nex"));
}
