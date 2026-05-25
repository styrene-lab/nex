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
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
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
            .env("NEX_TESTING", "1")
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

// ── Machine profile tests ─────────────────────────────────────────────────

fn valid_machine_profile_toml() -> &'static str {
    r#"
[machine_profile]
schema = "io.styrene.nex.machine-profile.v1"
id = "io.styrene.nex.machine-profile.test"
slug = "test"
name = "Test Machine Profile"
version = "1.0.0"
min_nex = "0.18.0"

[machine_profile.defaults]
mode = "plan-only"
target = "oci-image"

[machine_profile.safety]
default_destructive = false
requires_confirmation = true
requires_target_attestation = true
allowed_targets = ["nix-devshell", "oci-image", "vm", "physical-machine"]

[machine_profile.secrets]
required = ["GITHUB_TOKEN"]
optional = ["AWS_PROFILE"]

[[dependencies]]
kind = "forge-template"
id = "nixos-workstation"
version = ">=1.0.0"
required = true
"#
}

fn valid_machine_profile_pkl_json() -> &'static str {
    r#"{
  "machine_profile": {
    "schema": "io.styrene.nex.machine-profile.v1",
    "id": "io.styrene.nex.machine-profile.test",
    "slug": "test",
    "name": "Test Machine Profile",
    "version": "1.0.0",
    "min_nex": "0.18.0",
    "defaults": { "mode": "plan-only", "target": "oci-image" },
    "safety": {
      "default_destructive": false,
      "requires_confirmation": true,
      "requires_target_attestation": true,
      "allowed_targets": ["nix-devshell", "oci-image", "vm", "physical-machine"]
    },
    "secrets": { "required": ["GITHUB_TOKEN"], "optional": ["AWS_PROFILE"] }
  },
  "dependencies": [
    { "kind": "forge-template", "id": "nixos-workstation", "version": ">=1.0.0", "required": true }
  ]
}
"#
}

#[test]
fn machine_profile_validate_accepts_valid_manifest() {
    let sb = Sandbox::new();
    let profile_dir = sb.home.path().join("machine-profile");
    fs::create_dir_all(&profile_dir).expect("create profile dir");
    fs::write(profile_dir.join("machine-profile.pkl"), valid_machine_profile_pkl_json())
        .expect("write machine profile");

    let fake_pkl = write_fake_pkl(sb.home.path(), valid_machine_profile_pkl_json());
    sb.nex()
        .env("NEX_PKL", &fake_pkl)
        .args(["machine-profile", "validate", profile_dir.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("machine-profile.pkl is valid"));
}

#[test]
fn machine_profile_inspect_prints_metadata() {
    let sb = Sandbox::new();
    let profile_path = sb.home.path().join("machine-profile.pkl");
    fs::write(&profile_path, valid_machine_profile_pkl_json()).expect("write machine profile");
    let fake_pkl = write_fake_pkl(sb.home.path(), valid_machine_profile_pkl_json());

    sb.nex()
        .env("NEX_PKL", &fake_pkl)
        .args(["machine-profile", "inspect", profile_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Machine Profile"))
        .stdout(predicate::str::contains("ID: io.styrene.nex.machine-profile.test"))
        .stdout(predicate::str::contains("Mode: plan-only"))
        .stdout(predicate::str::contains("forge-template:nixos-workstation"));
}

#[test]
fn machine_profile_validate_rejects_secret_values() {
    let sb = Sandbox::new();
    let profile_path = sb.home.path().join("machine-profile.pkl");
    let invalid = valid_machine_profile_pkl_json().replace("GITHUB_TOKEN", "GITHUB_TOKEN=secret");
    fs::write(&profile_path, &invalid)
    .expect("write machine profile");

    let fake_pkl = write_fake_pkl(sb.home.path(), &invalid);
    sb.nex()
        .env("NEX_PKL", &fake_pkl)
        .args(["machine-profile", "validate", profile_path.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("must be a name"));
}

// ── Profile fragment tests ────────────────────────────────────────────────

fn valid_profile_fragment_pkl_json() -> &'static str {
    r#"{
  "fragment": {
    "schema": "io.styrene.nex.profile-fragment.v1",
    "id": "gpu/amd",
    "name": "amd",
    "description": "AMD GPU",
    "category": "gpu",
    "requires": ["platform/linux"],
    "conflicts": ["gpu/nvidia", "gpu/intel"],
    "platforms": ["linux"],
    "visibility": "public",
    "safety": {
      "mutates_system_services": false,
      "mutates_hardware_drivers": true,
      "requires_confirmation": true
    }
  }
}
"#
}

#[test]
fn profile_fragment_validate_accepts_valid_manifest() {
    let sb = Sandbox::new();
    let fragment_path = sb.home.path().join("amd.pkl");
    fs::write(&fragment_path, valid_profile_fragment_pkl_json()).expect("write fragment");
    let fake_pkl = write_fake_pkl(sb.home.path(), valid_profile_fragment_pkl_json());

    sb.nex()
        .env("NEX_PKL", &fake_pkl)
        .args(["profile-fragment", "validate", fragment_path.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("amd.pkl is valid"));
}

#[test]
fn profile_fragment_inspect_prints_metadata() {
    let sb = Sandbox::new();
    let fragment_path = sb.home.path().join("amd.pkl");
    fs::write(&fragment_path, valid_profile_fragment_pkl_json()).expect("write fragment");
    let fake_pkl = write_fake_pkl(sb.home.path(), valid_profile_fragment_pkl_json());

    sb.nex()
        .env("NEX_PKL", &fake_pkl)
        .args(["profile-fragment", "inspect", fragment_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Profile Fragment"))
        .stdout(predicate::str::contains("ID: gpu/amd"))
        .stdout(predicate::str::contains("Requires: platform/linux"))
        .stdout(predicate::str::contains("Conflicts: gpu/nvidia, gpu/intel"));
}

#[test]
fn profile_fragment_directory_validation_checks_path_id() {
    let sb = Sandbox::new();
    let fragment_dir = sb.home.path().join("fragments");
    fs::create_dir_all(fragment_dir.join("gpu")).expect("create fragment dir");
    fs::write(fragment_dir.join("gpu").join("amd.pkl"), valid_profile_fragment_pkl_json())
        .expect("write fragment");
    let fake_pkl = write_fake_pkl(sb.home.path(), valid_profile_fragment_pkl_json());

    sb.nex()
        .env("NEX_PKL", &fake_pkl)
        .args([
            "profile-fragment",
            "validate",
            fragment_dir.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("1 profile fragments valid"));
}

#[test]
fn profile_fragment_directory_validation_rejects_path_id_mismatch() {
    let sb = Sandbox::new();
    let fragment_dir = sb.home.path().join("fragments");
    fs::create_dir_all(fragment_dir.join("gpu")).expect("create fragment dir");
    let invalid = valid_profile_fragment_pkl_json()
        .replace("\"id\": \"gpu/amd\"", "\"id\": \"audio/amd\"");
    fs::write(fragment_dir.join("gpu").join("amd.pkl"), &invalid).expect("write fragment");
    let fake_pkl = write_fake_pkl(sb.home.path(), &invalid);

    sb.nex()
        .env("NEX_PKL", &fake_pkl)
        .args([
            "profile-fragment",
            "validate",
            fragment_dir.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("must start with its category prefix"));
}

// ── Machine profile signing tests ───────────────────────────────────────────────────────────

#[test]
fn profile_sign_creates_signed_toml() {
    let sb = Sandbox::new().with_identity();

    let profile_path = sb.home.path().join("test-machine-profile.toml");
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

// ── Forge materialization tests ────────────────────────────────────────────

#[test]
fn forge_check_materialization_evaluates_workspace() {
    let sb = Sandbox::new();
    let workspace = sb.home.path().join("materialization");
    fs::create_dir_all(&workspace).expect("create workspace");
    fs::write(workspace.join("flake.nix"), "{}").expect("write flake");

    sb.nex()
        .args([
            "forge",
            "check-materialization",
            workspace.to_str().unwrap(),
            "--hostname",
            "test-host",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("checking materialization"))
        .stdout(predicate::str::contains("materialization evaluates"));
}

#[test]
fn forge_check_materialization_rejects_invalid_hostname() {
    let sb = Sandbox::new();
    let workspace = sb.home.path().join("materialization");
    fs::create_dir_all(&workspace).expect("create workspace");
    fs::write(workspace.join("flake.nix"), "{}").expect("write flake");

    sb.nex()
        .args([
            "forge",
            "check-materialization",
            workspace.to_str().unwrap(),
            "--hostname",
            "bad/host",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("hostname must contain only"));
}

#[test]
fn forge_check_materialization_rejects_workspace_without_flake() {
    let sb = Sandbox::new();
    let workspace = sb.home.path().join("materialization");
    fs::create_dir_all(&workspace).expect("create workspace");

    sb.nex()
        .args([
            "forge",
            "check-materialization",
            workspace.to_str().unwrap(),
            "--hostname",
            "test-host",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not contain flake.nix"));
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

#[test]
#[cfg(unix)]
fn forge_check_validates_template_with_json_report() {
    let sb = Sandbox::new();
    let dir = sb.home.path().join("forge-template");
    fs::create_dir_all(&dir).expect("create forge template dir");
    let forge_pkl = dir.join("forge.pkl");
    let forge_toml = dir.join("forge.toml");
    fs::write(&forge_pkl, "name = \"minimal-workstation\"\n").expect("write forge.pkl");
    fs::write(
        &forge_toml,
        r#"
[forge_template]
id = "minimal-workstation"
version = "1.0.0"
canonical_format = "pkl"
visibility = "public"
profile_class = "desktop"
destructive_capabilities = ["image-build"]
network_requirements = ["package-download"]
"#,
    )
    .expect("write forge.toml");

    let fake_pkl = dir.join("fake-pkl");
    fs::write(
        &fake_pkl,
        r#"#!/bin/sh
cat <<'JSON'
{
  "name": "minimal-workstation",
  "profileClass": "desktop",
  "visibility": "public",
  "plan": {
    "mode": "image-build",
    "target": "operator-selected",
    "requiresNetwork": true
  }
}
JSON
"#,
    )
    .expect("write fake pkl");
    let mut perms = fs::metadata(&fake_pkl)
        .expect("stat fake pkl")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&fake_pkl, perms).expect("chmod fake pkl");

    sb.nex()
        .env("NEX_PKL", &fake_pkl)
        .args([
            "forge",
            "check",
            forge_pkl.to_str().unwrap(),
            "--metadata",
            forge_toml.to_str().unwrap(),
            "--json",
            "--no-execute",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"valid\": true"))
        .stdout(predicate::str::contains("\"id\": \"minimal-workstation\""))
        .stdout(predicate::str::contains("\"canonicalFormat\": \"pkl\""));
}

#[test]
#[cfg(unix)]
fn forge_run_dry_run_plans_request_without_building() {
    let sb = Sandbox::new();
    let dir = sb.home.path().join("forge-run");
    fs::create_dir_all(&dir).expect("create forge run dir");
    let request = dir.join("request.pkl");
    fs::write(&request, "operation = \"bundle\"\n").expect("write request");
    let fake_pkl = write_fake_pkl(
        &dir,
        r#"{
  "schemaVersion": 1,
  "operation": "bundle",
  "hostname": "seed",
  "arch": "x86_64",
  "target": {
    "kind": "bundle"
  }
}
"#,
    );

    sb.nex()
        .env("NEX_PKL", &fake_pkl)
        .args([
            "--dry-run",
            "forge",
            "run",
            "--request",
            request.to_str().unwrap(),
            "--events",
            "jsonl",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"type\":\"phase_started\""))
        .stdout(predicate::str::contains("\"type\":\"run_completed\""))
        .stderr(predicate::str::contains("\"operation\": \"bundle\""));
}

#[test]
#[cfg(unix)]
fn forge_run_refuses_blocked_destructive_request() {
    let sb = Sandbox::new();
    let dir = sb.home.path().join("forge-run-blocked");
    fs::create_dir_all(&dir).expect("create forge run dir");
    let request = dir.join("request.pkl");
    fs::write(&request, "operation = \"usb-install\"\n").expect("write request");
    let fake_pkl = write_fake_pkl(
        &dir,
        r#"{
  "schemaVersion": 1,
  "operation": "usb-install",
  "hostname": "seed",
  "arch": "x86_64",
  "target": {
    "kind": "usb",
    "disk": "/dev/sda"
  }
}
"#,
    );

    sb.nex()
        .env("NEX_PKL", &fake_pkl)
        .args([
            "forge",
            "run",
            "--request",
            request.to_str().unwrap(),
            "--events",
            "jsonl",
        ])
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("\"type\":\"blocker\""))
        .stderr(predicate::str::contains("DESTRUCTIVE_FLASH_NOT_ALLOWED"));
}

#[cfg(unix)]
fn write_fake_pkl(dir: &Path, json: &str) -> PathBuf {
    let fake_pkl = dir.join("fake-pkl");
    fs::write(
        &fake_pkl,
        format!(
            r#"#!/bin/sh
cat <<'JSON'
{json}
JSON
"#
        ),
    )
    .expect("write fake pkl");
    let mut perms = fs::metadata(&fake_pkl)
        .expect("stat fake pkl")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&fake_pkl, perms).expect("chmod fake pkl");
    fake_pkl
}

// ── Build image tests ──────────────────────────────────────────────────────

#[test]
fn build_image_accepts_styrene_package_manifest() {
    let sb = Sandbox::new();
    let package_dir = sb.home.path().join("agent-package");
    fs::create_dir_all(&package_dir).expect("create package dir");
    fs::write(
        package_dir.join("machine-profile.toml"),
        r#"
[meta]
name = "profile-fallback"

[container]
packages = ["git"]
"#,
    )
    .expect("write profile");
    fs::write(
        package_dir.join("styrene-package.toml"),
        r#"
[package]
name = "styrene.agent.primary"
version = "0.1.0"

[nex]
profile = "./machine-profile.toml"

[image]
name = "ghcr.io/styrene-lab/primary"
tag = "0.1.0"
entrypoint = "/bin/omegon"
cmd = ["serve", "--control-plane", "0.0.0.0:7842"]
ports = [7842]

[agent]
role = "primary-driver"
mode = "daemon"
posture = "orchestrator"
"#,
    )
    .expect("write package manifest");

    sb.nex()
        .args(["build-image", package_dir.to_str().unwrap(), "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "package: styrene.agent.primary:0.1.0",
        ))
        .stdout(predicate::str::contains(
            "Would build: ghcr.io/styrene-lab/primary:0.1.0",
        ));
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

// ── Identity export tests ───────────────────────────────────────────────────

#[test]
fn identity_wg_exports_keypair() {
    let sb = Sandbox::new().with_identity();
    sb.nex()
        .args(["identity", "wg"])
        .assert()
        .success()
        .stderr(predicate::str::contains("privkey"))
        .stderr(predicate::str::contains("wireguard"));
}

#[test]
fn identity_age_exports_key() {
    let sb = Sandbox::new().with_identity();
    sb.nex()
        .args(["identity", "age"])
        .assert()
        .success()
        .stderr(predicate::str::contains("age"));
}

// ── Machine profile verify tests ───────────────────────────────────────────────────

#[test]
fn profile_apply_verify_unsigned_fails() {
    let sb = Sandbox::new().with_identity().with_config();
    let profile_path = sb.home.path().join("test-machine-profile.toml");
    std::fs::write(
        &profile_path,
        "[meta]\nname = \"test\"\n\n[packages]\nnix = [\"git\"]\n",
    )
    .expect("write profile");
    sb.nex()
        .args([
            "profile",
            "apply",
            profile_path.to_str().unwrap(),
            "--verify",
        ])
        .assert()
        .failure();
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
