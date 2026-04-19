# Changelog

All notable changes to nex are documented here. Format follows [Keep a Changelog](https://keepachangelog.com/).

## [0.9.3] - 2026-04-19

### Fixed
- `nex doctor` falsely reporting `~/.local/bin` on PATH when it was only configured in nix but not yet active
- All clippy warnings (needless lifetimes, needless borrows, collapsible if, useless format, dead code, too many arguments)

## [0.2.0] - 2026-04-17

### Added
- `nex init` command — bootstraps nix-darwin + homebrew from scratch on a fresh Mac
  - Installs Determinate Nix and Homebrew if missing
  - Scaffolds a minimal nix-darwin + home-manager config, or clones an existing one with `--from <url>`
  - Runs the first `darwin-rebuild switch`
  - Writes `~/.config/nex/config.toml` for future commands
- Smart package resolution — `nex install slack` checks nixpkgs, brew casks, and brew formulae
  - Compares versions across sources
  - Detects GUI apps (cask presence) vs CLI tools
  - Auto-installs CLI tools from nix with no prompt
  - Interactive prompt for conflicts showing versions and recommendation
- `--nix` flag to force nix source, bypassing resolution
- Version queries via `nix eval` and `brew info --json=v2`

### Changed
- Default `nex install` (no flags) now uses auto-resolution instead of assuming nix
- Error messages for missing binaries now explain the cause (e.g., "darwin-rebuild not found") instead of showing raw OS errors

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

[0.2.0]: https://github.com/styrene-lab/nex/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/styrene-lab/nex/releases/tag/v0.1.0
