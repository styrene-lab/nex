# Changelog

All notable changes to nex are documented here. Format follows [Keep a Changelog](https://keepachangelog.com/).

## [0.13.0] - 2026-04-24

### Added
- `nex relocate` command -- moves a system-owned config (e.g. `/etc/nixos`) into a user-writable directory so nex no longer needs sudo to read or edit it
- Alias expansion for `nex install` -- more shorthand names resolve correctly

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
