# Remediation Plan (Round 2)

Fixes for each issue in ROAST-2.md.

---

## Priority 1: Security

### FIX-R2-2: Make checksum verification mandatory in self-update
**File:** `src/ops/self_update.rs`
**What:**
- [ ] If checksum file download fails, bail with a hard error instead of warning and continuing
- [ ] Add `--skip-checksum` flag for users who explicitly want to bypass (opt-in risk, not opt-out safety)

### FIX-R2-3: Fall back to sha256sum when shasum is unavailable
**File:** `src/ops/self_update.rs`
**What:**
- [ ] Try `shasum -a 256` first, fall back to `sha256sum` if shasum fails with NotFound
- [ ] Both have identical output format (`hash  filename`), so parsing stays the same

### FIX-R2-4: Add checksum verification to install.sh
**File:** `site/public/install.sh`
**What:**
- [ ] After downloading the tarball, fetch `checksums.sha256` from the same release
- [ ] Verify with `sha256sum -c` or `shasum -a 256 -c`
- [ ] Fail the install if checksum doesn't match (not a warning)

### FIX-R2-5: Pin GitHub Actions to commit SHAs in release workflow
**File:** `.github/workflows/release.yml`
**What:**
- [ ] Replace `actions/checkout@v4` with full SHA pin
- [ ] Replace `actions/upload-artifact@v4` with full SHA pin
- [ ] Replace `actions/download-artifact@v4` with full SHA pin
- [ ] Replace `dtolnay/rust-toolchain@stable` with full SHA pin
- [ ] Add comment with human-readable version next to each SHA

### FIX-R2-11: Prefer hardcoded nix paths over PATH lookup
**File:** `src/exec.rs`
**What:**
- [ ] Check well-known paths FIRST (`/nix/var/nix/profiles/default/bin/nix`, `/run/current-system/sw/bin/nix`)
- [ ] Only fall back to PATH-based `Command::new("nix")` if no well-known path exists
- [ ] Same for `find_darwin_rebuild` and `find_nixos_rebuild`

---

## Priority 2: Correctness

### FIX-R2-1: Fix parse_item quote handling and dot inconsistency
**File:** `src/nixfile.rs`
**What:**
- [ ] Replace `trim_start_matches('"')` with `strip_prefix('"')` to remove exactly one leading quote
- [ ] Remove `word.contains('.')` from the bare identifier rejection list -- dots are valid in nixpkgs attrs (e.g. `python3.11` is `python311` in nixpkgs but dotted attrs exist)
- [ ] Or: remove `.` from `validate_pkg_name`'s allowed chars if dots genuinely shouldn't appear
- [ ] Pick one and make parse_item and validate_pkg_name agree

### FIX-R2-6: Add validate_pkg_name to remove path
**File:** `src/edit.rs`
**What:**
- [ ] Add `validate_pkg_name(pkg)?` at the top of `remove()` for consistency

### FIX-R2-7: Make profile string replacements fallible
**File:** `src/ops/profile.rs`
**What:**
- [ ] After each `.replace()`, check if the result differs from the input
- [ ] If replacement had no effect and the feature was expected to be applied, warn the user explicitly: "could not wire shell.nix -- manual edit required"
- [ ] Do NOT write the file unchanged and report success
- [ ] Pattern: `let patched = content.replace(old, new); if patched == content { output::warn("..."); }`

### FIX-R2-8: Use toml crate for set_preference
**File:** `src/config.rs`
**What:**
- [ ] Replace hand-rolled line splitting with `toml::from_str` -> modify -> `toml::to_string` -> atomic_write
- [ ] Preserves tables, handles quoted values, doesn't corrupt comments (toml-edit crate for comment preservation, or accept comment loss)

### FIX-R2-10: Replace brittle multi-line string replacements in doctor
**File:** `src/ops/doctor.rs`
**What:**
- [ ] Replace the exact-whitespace `.replace()` patterns with line-by-line insertion logic
- [ ] For mac-app-util input: find the line containing `};` that closes inputs, insert before it
- [ ] For outputs parameter: find the `outputs =` line, add `mac-app-util` to the destructuring
- [ ] Validate the result structurally (matching braces) before writing

---

## Priority 3: Robustness

### FIX-R2-9: Use atomic_write_bytes for init scaffold files
**File:** `src/ops/init.rs`
**What:**
- [ ] Replace `std::fs::write` calls for scaffolded nix files with `crate::edit::atomic_write_bytes`
- [ ] Focus on the files that could be overwriting existing content (adopt path, clone path)
- [ ] For truly new files (fresh scaffold into empty dir), `std::fs::write` is acceptable but atomic is still better

### FIX-R2-12: Add partition readiness check in polymerize
**File:** `src/ops/polymerize.rs`
**What:**
- [ ] After parted, poll for partition device existence with a timeout instead of `sleep(1)`
- [ ] Check that the partition device node exists before calling mkfs
- [ ] Make `umount -R /mnt` failure a warning instead of silent ignore
