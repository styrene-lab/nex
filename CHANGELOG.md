# Changelog

All notable changes to nex are documented here. Format follows [Keep a Changelog](https://keepachangelog.com/).

## [0.16.0] - 2026-04-29

### Added
- **Interactive `nex forge`** — run with no args to get walked through the entire process. Prompts for profile, hostname, target arch (x86_64/aarch64), USB device (lists removable disks), WiFi pre-config, and SSH key baking. All flags still work — if passed, they skip the corresponding prompt. Non-interactive (piped) mode falls back to current behavior.
- **`nex profile sign <source>`** — sign a profile with your StyreneIdentity. Resolves the full extends/compose chain, canonicalizes the merged TOML, signs with Ed25519. Embeds pubkey + source ref in the signed output. Supports `--detached` for separate .sig files.
- **`nex profile verify <source>`** — verify a signed profile using the embedded public key. No passphrase or identity file needed (public-key operation). Validates pubkey-to-hash binding and source ref match.
- **`nex profile apply <source>`** — explicit subcommand (previously `nex profile <source>`).
- **macOS disk discovery** — `diskutil list -plist external physical` with text fallback for forge USB picker.
- **Linux disk discovery** — `lsblk -d -J` with rm field string/bool compatibility for forge USB picker.
- **Arch selection** — forge now supports aarch64 targets (ARM ISO + binary).

### Changed
- `nex profile` is now a subcommand group (`apply`, `sign`, `verify`) — **breaking**: `nex profile <source>` becomes `nex profile apply <source>`.
- Profile canonicalization includes `nex-profile-sig-v1\nsource:{ref}\n` header to bind signatures to their source and prevent rebinding attacks.
- Signature verification uses `VerifyingKey::from_bytes()` directly (not `ed25519_verifying_key` which expects a seed).

### Security
- Profile signatures embed the signer's public key (`meta.pubkey`) — verification is a public-key operation, no private key access needed on the verifying machine.
- Pubkey-to-hash validation prevents substituting a different pubkey while keeping the same `signed_by` hash.
- Source ref binding prevents a signed profile from being presented as a different source.
- `signed_source` field embedded in signed output so verify can reconstruct identical canonical bytes.
- WiFi PSK written with 0o600 permissions in forge bundles.

## [0.15.0] - 2026-04-28

### Added
- **`nex identity list`** — scan for all identities on the machine (default path, additional `.key` files, env vars). No passphrase needed — shows file metadata, size, permissions.
- **`nex identity ssh <label>`** — export SSH pubkey to stdout (pipeable) with fingerprint. Derives per-label Ed25519 keys via two-level HKDF.
- **`nex identity ssh --list`** — show all registered SSH key labels and their fingerprints.
- **`nex identity ssh --add <label>`** — register a new SSH label in config and immediately export the key.
- **`nex identity git`** — configure git commit signing. Prompts for name/email (saved to config), derives signing key, applies via `git config --global`.
- **`nex identity git --show`** — display current git signing configuration.
- **`nex doctor` identity checks** — reports identity file existence, size (97 bytes), permissions (0o600), git signing status, and registered SSH labels.
- **Nested config support** — `~/.config/nex/config.toml` now supports `[identity.git]` (name, email) and `[identity.ssh]` (labels) sections via `set_nested_preference()` and `append_to_list()`.

### Changed
- Upgraded to `styrene-identity` 0.2.0 — uses canonical `identity_hash()`, `identity_pubkey()`, `format::ssh_pubkey()`, `format::ssh_pubkey_fingerprint()`, `format::git_signing_config()` from the crate instead of hand-rolled implementations.
- Replaced `ClosurePassphraseProvider` with `StaticPassphraseProvider` / `FileSigner::with_static_passphrase()`.
- Extracted `load_root()` helper for consistent passphrase zeroization across all identity operations.
- Dropped direct `sha2` dependency (using styrene-identity's `identity_hash()` instead).

## [0.14.0] - 2026-04-28

### Added
- **Styrene Identity integration** — `nex identity init`, `nex identity show`, `nex identity link` for creating, inspecting, and enrolling cryptographic mesh identities. Backed by `styrene-identity` 0.1.1 from crates.io (HKDF-SHA256 key hierarchy, argon2id file encryption, Ed25519/X25519 derivation).
- **Security hardware aliases** — `nex install ykman` resolves to `yubikey-manager`, plus aliases for `ykpers`, `piv-tool`, `fido2`, `opensc`, `pkcs11-tool`, `pcsc-lite`, `pcscd`, `pcsc-tools`, `pcsc_scan`. Brew↔nix cross-detection ensures `nex adopt` then `nex install` correctly catches duplicates across package manager boundaries.

### Changed
- Identity hash uses unified `KeyPurpose::Signing` (was split across deprecated `RnsSigning`/`GitSigning`)
- `nex identity show` displays `pubkey` + `ssh host` + `age key` (was showing redundant mesh/git keys before unification)
- `run_link` takes `&str` parameters instead of owned `String` (lipstyk finding)
- Error responses in `identity link` now propagate via `context()` instead of `unwrap_or_default()` (lipstyk finding)
- All passphrase and derived key seed material is explicitly zeroized after use

## [0.13.2] - 2026-04-24

### Added
- **Structured tracing** via `tracing` crate -- set `NEX_LOG=debug` (or `NEX_LOG=nex::edit=trace`) for full diagnostic output across config resolution, package resolution, file editing, and command execution
- Checksum verification in `install.sh` -- first-time installs now verify SHA256 before extracting

### Changed
- Checksum verification in `nex self-update` is now **mandatory** (was optional/warn-only)
- `shasum` falls back to `sha256sum` for Alpine/musl/NixOS compatibility
- `find_nix`, `find_darwin_rebuild`, `find_nixos_rebuild` now check well-known paths before PATH (prevents PATH injection)
- Release workflow GitHub Actions pinned to commit SHAs
- `set_preference` uses `toml` crate for proper parse/modify/serialize instead of hand-rolled line splitting
- Doctor mac-app-util patching uses line-by-line insertion instead of brittle multi-line string replacement
- Profile string replacements detect and warn on silent no-ops instead of writing unchanged files
- All `std::fs::write` calls in `init.rs` scaffold replaced with `atomic_write_bytes`
- Polymerize partition sequence polls for device readiness (10s timeout) instead of fixed `sleep(1)`

### Fixed
- `parse_item` quote parsing: `strip_prefix('"')` replaces `trim_start_matches('"')` (no longer eats all leading quotes)
- `validate_pkg_name` no longer allows dots (consistent with `parse_item` rejection of dotted identifiers)
- `validate_pkg_name` now called on `remove()` too, not just `insert()`
- Polymerize `umount` failures now warned instead of silently ignored

## [0.13.1] - 2026-04-24

### Changed
- **Security hardening** across the codebase:
  - `nex self-update` now verifies SHA256 checksums when available, validates download URLs, and guards against tar path traversal
  - WiFi credentials in `nex polymerize` are written with mode 0600, values are escaped, and cleanup is guaranteed via drop guard
  - Package names are validated against a safe character set before nix file insertion
  - `--nix`, `--cask`, and `--brew` flags are now mutually exclusive
  - Hostnames are validated for safe characters (alphanumeric + hyphens)
- **Data integrity** improvements:
  - All atomic writes now `fsync` before rename -- safe against power loss
  - 15+ bare `std::fs::write` calls on config files replaced with atomic temp+fsync+rename
  - Backup restore now errors when the backup file is missing instead of silently succeeding
  - Config `set_preference` uses atomic writes
- **Correctness** fixes:
  - Version comparison uses semver parsing with lenient fallback instead of string equality
  - Nix list range detection uses exact indent match (removed fragile `+2` tolerance)
  - All silent `let _ = git` patterns replaced with a helper that warns on failure
  - Module discovery now globs `nix/modules/home/*.nix` instead of hardcoding `kubernetes.nix`
- `is_already_declared` reads each file once via `contains_any()` instead of N times per alias
- Critical `let _ =` patterns in forge/polymerize (nix copy, mkfs, cp, umount) now warn on failure
- Release workflow now generates and attaches `checksums.sha256` to GitHub releases

## [0.13.0] - 2026-04-24

### Added
- `nex relocate` command -- moves a system-owned config (e.g. `/etc/nixos`) into a user-writable directory so nex no longer needs sudo to read or edit it
- Alias expansion for `nex install` -- more shorthand names resolve correctly

### Fixed
- `home.sessionPath` now includes cargo, nix-profile, and homebrew paths

## [0.12.0] - 2026-04-23

### Added
- Profile composition: `compose` field in profile.toml allows combining multiple fragments from the same repo
- All clippy lints resolved (zero errors, zero warnings)

## [0.11.0] - 2026-04-22

### Changed
- Profile system rewritten with collect/merge/render architecture for cleaner layering
- Profiles deduplicate `profileExtra` and `initExtra` on re-apply

## [0.10.2] - 2026-04-21

### Fixed
- Profile `profileExtra`/`initExtra` no longer duplicates content when re-applying a profile

## [0.10.1] - 2026-04-21

### Fixed
- Shell config generation and wiring in profile apply
- History settings use native bash options, warn on wire failure
- Init flow verifies git is available before scaffolding
- Init enables bash in scaffold, adopts brew packages before first switch

## [0.10.0] - 2026-04-20

### Added
- **NixOS support** -- `nex init`, `nex install`, `nex switch` and all core commands now work on NixOS in addition to macOS
- `nex develop` -- enter a flake dev shell (`nix develop` wrapper)
- `nex dev` -- dev shell with [omegon](https://github.com/styrene-lab/omegon) AI coding agent layered in
- `nex build-image` -- build OCI container images from profiles (podman or docker)
- `nex forge` test coverage -- 25 new tests covering forge, polymerize, and discover
- devShell in flake.nix for contributor workflow
- Install script detects NixOS and uses `nix profile install` as primary method

### Changed
- Install/remove use platform-aware rebuild (nixos-rebuild on Linux, darwin-rebuild on macOS)
- Config detects flat vs scaffolded repo layout automatically
- `nex dev` split from `nex develop` -- `develop` is pure nix, `dev` requires omegon

### Fixed
- Install script PATH not added on fresh macOS
- `nex init` falls back to `.pkg` installer when nix shell installer fails
- Home-manager profile dirs created before first switch
- Install script no longer creates `~/.local` as root
- Init flow calls `ensure_profile_dirs` before first switch

## [0.9.4] - 2026-04-19

### Fixed
- Minor formatting and lint fixes

## [0.9.3] - 2026-04-19

### Fixed
- `nex doctor` falsely reporting `~/.local/bin` on PATH when it was only configured in nix but not yet active
- All clippy warnings (needless lifetimes, needless borrows, collapsible if, useless format, dead code, too many arguments)

## [0.2.0] - 2026-04-17

### Added
- `nex init` command -- bootstraps nix-darwin + homebrew from scratch on a fresh Mac
  - Installs Determinate Nix and Homebrew if missing
  - Scaffolds a minimal nix-darwin + home-manager config, or clones an existing one with `--from <url>`
  - Runs the first `darwin-rebuild switch`
  - Writes `~/.config/nex/config.toml` for future commands
- Smart package resolution -- `nex install slack` checks nixpkgs, brew casks, and brew formulae
  - Compares versions across sources
  - Detects GUI apps (cask presence) vs CLI tools
  - Auto-installs CLI tools from nix with no prompt
  - Interactive prompt for conflicts showing versions and recommendation
- `--nix` flag to force nix source, bypassing resolution
- Version queries via `nix eval` and `brew info --json=v2`

### Changed
- Default `nex install` (no flags) now uses auto-resolution instead of assuming nix
- Error messages for missing binaries now explain the cause instead of showing raw OS errors

### Fixed
- Removed vestigial dead match arm in main.rs
- `resolve.rs` lint compliance with `unwrap_used` clippy setting

## [0.1.0] - 2026-04-17

### Added
- Initial release
- `nex install` / `nex remove` with `--cask` and `--brew` flags
- `nex search`, `nex list`, `nex update`, `nex switch`, `nex rollback`
- `nex try` (ephemeral nix shell), `nex diff`, `nex gc`
- Line-based nix file editing with atomic backup/revert on failed switch
- Duplicate detection across multiple nix module files
- Auto-discovery of nix-darwin repo (CWD walk-up, well-known paths, config file)
- Auto-detection of hostname via `scutil`
- `--dry-run` global flag
- Astro site at nex.styrene.io with install script
- CI: Rust (format + clippy + test), Cloudflare Pages deploy, release workflow
- Cross-platform prebuilt binaries (aarch64-darwin, x86_64-darwin, aarch64-linux, x86_64-linux)
- Published to crates.io as `nex-pkg`

[0.13.2]: https://github.com/styrene-lab/nex/compare/v0.13.1...v0.13.2
[0.13.1]: https://github.com/styrene-lab/nex/compare/v0.13.0...v0.13.1
[0.13.0]: https://github.com/styrene-lab/nex/compare/v0.12.0...v0.13.0
[0.12.0]: https://github.com/styrene-lab/nex/compare/v0.11.0...v0.12.0
[0.11.0]: https://github.com/styrene-lab/nex/compare/v0.10.2...v0.11.0
[0.10.2]: https://github.com/styrene-lab/nex/compare/v0.10.1...v0.10.2
[0.10.1]: https://github.com/styrene-lab/nex/compare/v0.10.0...v0.10.1
[0.10.0]: https://github.com/styrene-lab/nex/compare/v0.9.4...v0.10.0
[0.9.4]: https://github.com/styrene-lab/nex/compare/v0.9.3...v0.9.4
[0.9.3]: https://github.com/styrene-lab/nex/compare/v0.2.0...v0.9.3
[0.2.0]: https://github.com/styrene-lab/nex/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/styrene-lab/nex/releases/tag/v0.1.0
