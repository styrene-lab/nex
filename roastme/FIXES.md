# Remediation Plan

Fixes for each issue in ROAST.md, ordered by severity and blast radius.

---

## Priority 1: Security (do these first)

### FIX-3: Self-update supply chain hardening
**Files:** `src/ops/self_update.rs`, `site/public/install.sh`
**What:**
- [ ] Add SHA256 checksum verification: publish checksums in releases, download and verify before extracting
- [ ] Add `--strip-components=1` or validate extracted paths don't contain `..` before extracting tarball
- [ ] Validate the download URL scheme is HTTPS and matches `github.com/styrene-lab/nex`
- [ ] Consider adding cosign signature verification long-term

### FIX-5: WiFi credential handling in polymerize
**Files:** `src/ops/polymerize.rs`
**What:**
- [ ] Write wpa_supplicant.conf with mode 0600 (use `OpenOptions` with `mode()`)
- [ ] Use a `scopeguard` or manual Drop impl to guarantee cleanup on all exit paths
- [ ] Escape SSID and PSK values to prevent wpa_supplicant config injection

### FIX-9a: Package name validation
**Files:** `src/ops/install.rs`, `src/edit.rs`
**What:**
- [ ] Validate package names against a safe character set (alphanumeric, hyphen, underscore, period) before inserting into nix files
- [ ] Reject names containing quotes, semicolons, brackets, or whitespace

---

## Priority 2: Data Integrity (the "eat your config" bugs)

### FIX-2: Flush before persist in atomic_write
**Files:** `src/edit.rs`
**What:**
- [ ] Add `tmp.as_file().sync_all()?` before `tmp.persist(path)` in `atomic_write`
- [ ] Add `tmp.as_file().sync_all()?` before `tmp.persist(&backup_path)` in `backup`

### FIX-2b: Non-atomic writes across the codebase
**Files:** `src/ops/doctor.rs`, `src/ops/profile.rs`, `src/ops/polymerize.rs`, `src/ops/init.rs`, `src/config.rs`
**What:**
- [ ] Extract a shared `atomic_write_bytes(path, content)` utility (write to NamedTempFile in same dir, sync_all, persist)
- [ ] Replace all bare `std::fs::write` of nix config files with the atomic utility
- [ ] Replace `set_preference` write with atomic utility

### FIX-10: Backup verification in EditSession
**Files:** `src/edit.rs`
**What:**
- [ ] In `revert_all`, when `backup_path.exists()` is false, push an error instead of silently returning Ok
- [ ] In `restore`, change the missing-backup case from `Ok(())` to `Err` so callers know the revert failed

### FIX-7: Config file atomicity and locking
**Files:** `src/config.rs`
**What:**
- [ ] Use the new atomic write utility for `set_preference`
- [ ] Add advisory file locking (flock) around config read-modify-write to prevent concurrent corruption

---

## Priority 3: Correctness (wrong answers, wrong recommendations)

### FIX-6: Semantic version comparison
**Files:** `src/resolve.rs`, `Cargo.toml`
**What:**
- [ ] Add `semver` crate dependency
- [ ] Parse versions with `semver::Version::parse` (with lenient fallback for non-semver strings)
- [ ] Compare parsed versions instead of raw strings
- [ ] Fall back to string equality only when both versions fail to parse

### FIX-1: Harden list range detection
**Files:** `src/edit.rs`
**What:**
- [ ] Replace magic `+2` with exact indent match: close must be at `<= open_indent` (not `open_indent + 2`)
- [ ] Add a validation step: after finding the range, verify no lines between open+1 and close look like a different list opening
- [ ] Add test cases for nested lists, stray `];`, and hand-edited files with unusual indentation

### FIX-4: Surface git commit failures
**Files:** `src/ops/adopt.rs`, `src/ops/doctor.rs`, `src/ops/profile.rs`
**What:**
- [ ] Replace `let _ = Command::new("git")...` with a helper that checks exit status
- [ ] On failure, warn the user: "changes written to disk but git commit failed -- please commit manually"
- [ ] Do NOT make it a hard error (git might not be configured), but always inform

### FIX-9b: CLI input validation
**Files:** `src/cli.rs`
**What:**
- [ ] Add `#[clap(conflicts_with_all = ["cask", "brew"])]` to `--nix` (and symmetrically for the others)
- [ ] Add hostname validation (alphanumeric + hyphens, no leading/trailing hyphen)
- [ ] Add a confirmation prompt in `forge` when `--disk` targets a system disk (e.g. disk0, sda)

---

## Priority 4: Performance and Polish

### FIX-8: Deduplicate file reads in is_already_declared
**Files:** `src/ops/install.rs`, `src/edit.rs`
**What:**
- [ ] Add a `contains_any(path, list, names: &[&str]) -> Result<Option<String>>` function to edit.rs that reads the file once and checks all names in a single pass
- [ ] Refactor `is_already_declared` to use it, reading each file at most once

### FIX-11: Dynamic module discovery
**Files:** `src/config.rs`
**What:**
- [ ] Glob `nix/modules/home/*.nix` instead of hardcoding `kubernetes.nix`
- [ ] Filter out `base.nix` (already the primary) and `default.nix` (if it exists as a module entry point)
- [ ] Each discovered file gets its stem as the module name

### FIX-12: forge/polymerize hardening (lower priority, higher effort)
**Files:** `src/ops/forge.rs`, `src/ops/polymerize.rs`
**What:**
- [ ] Audit all `let _ =` patterns and convert to warn-on-failure
- [ ] Add exit-status checks for `dd`, `mkfs`, `mount`, `nixos-install`
- [ ] Add basic integration test coverage for the happy path (even if it requires a VM or container)
- [ ] Document that the placeholder nex binary on ISO is intentional (or fix it by cross-compiling)
