# CLAUDE.md

## What This Is

Rust CLI that provides imperative package management UX (`nex install foo`) on top of declarative nix-darwin configuration. Edits `.nix` files, runs `darwin-rebuild switch`, and reverts on failure. Published as `nex-pkg` on crates.io; binary name is `nex`.

## Architecture

Single crate, no workspace. Key modules:

- `aliases.rs` — package name alias table (rg->ripgrep, zed->zed-editor, ykman->yubikey-manager, etc.) with brew↔nix cross-detection for adopt/install duplicate prevention
- `cli.rs` — clap derive structs, all subcommands
- `config.rs` — config resolution: CLI flags -> env vars -> config file (~/.config/nex/config.toml) -> auto-discovery; persistent preferences (prefer_nix_on_equal)
- `discover.rs` — find the nix-darwin repo and hostname
- `resolve.rs` — multi-source resolution: checks nixpkgs (with alias lookup) + brew cask + brew formula, compares versions, recommends best source, interactive prompt with "always nix" option for equal versions
- `nixfile.rs` — `NixList` model describing editable lists in nix files (open/close patterns, indent, quoting)
- `edit.rs` — line-based nix file editing: contains, insert, remove, list_packages, backup/restore
- `exec.rs` — subprocess wrappers for nix (with well-known path fallback), darwin-rebuild, brew; LaunchServices re-registration after switch
- `output.rs` — colored terminal output helpers
- `ops/init.rs` — bootstrap: installs nix + homebrew, scaffolds or clones nix-darwin config (with mac-app-util for Spotlight), detects existing configs, warns about existing brew packages
- `ops/adopt.rs` — safe onboarding: captures all installed brew packages into the config, detects PATH collisions with manually installed binaries, offers to pin
- `ops/install.rs` — install with auto-resolution, alias-aware duplicate detection, atomic revert on failure, prefer-nix-on-equal preference
- `ops/identity.rs` — StyreneIdentity lifecycle: `init` (generate encrypted key), `show` (display hash + pubkeys), `link` (enroll with Signum hub). Uses `styrene-identity` crate for HKDF derivation, Ed25519 signing, argon2id file encryption. All secrets zeroized after use.
- `ops/profile.rs` — apply a machine profile from a GitHub repo
- `ops/profile.rs` — apply, sign, and verify machine profiles. `apply` resolves profile chain (extends/compose), merges layers, renders to nix modules. `sign` canonicalizes merged TOML + source ref, signs with Ed25519 identity, embeds pubkey. `verify` checks signature using embedded pubkey (no private key needed).
- `ops/forge.rs` — build a bootable NixOS installer USB, optionally with a baked-in profile. Interactive mode prompts for profile, hostname, arch, disk, WiFi, SSH key when flags are omitted.
- `ops/polymerize.rs` — interactive NixOS installer (runs on target after booting from USB)
- `ops/build_image.rs` — build an OCI container image from a profile
- `ops/develop.rs` — enter a dev shell from a flake (wraps `nix develop`)
- `ops/dev.rs` — open a project with omegon AI coding agent
- `ops/relocate.rs` — move a system-owned config (e.g. /etc/nixos) into a user-writable directory
- `ops/migrate.rs` — report: identifies brew packages that could move to nix
- `ops/doctor.rs` — config health checks: patches in mac-app-util if missing
- `ops/self_update.rs` — downloads latest release binary from GitHub, replaces self in-place
- `ops/` — remaining subcommands: remove, list, search, switch, update, rollback, try, diff, gc

## Commands

```bash
just validate     # format-check + lint + test
just test         # cargo test
just lint         # cargo clippy --all-targets --no-deps -- -D warnings
just format       # cargo fmt
just build        # debug build
just install      # cargo install --path .
just integration  # containerized integration tests (docker/podman)
```

## Key Design Decisions

- **Line-based editing, not a nix parser.** Matches on known list patterns (`home.packages = with pkgs; [` ... `];`). Works because we control the file format.
- **Atomic edits.** Backup before edit, revert all on failed switch, delete backups on success.
- **Smart resolution with aliases.** `nex install zed` resolves `zed-editor` in nixpkgs; `nex install rg` catches existing `ripgrep`. Versions compared across sources.
- **Prefer-nix-on-equal.** When nix and brew have the same version, defaults to nix. Operator can opt into "always nix" which persists to config.toml and silences future prompts for equal versions. Brew-newer always prompts regardless.
- **Safe onboarding.** `nex adopt` captures existing brew state before first switch so `cleanup = "zap"` doesn't nuke packages. PATH collision detection with version comparison and pin option.
- **Synchronous.** No tokio runtime. File I/O and subprocesses are blocking. (tokio exists as a transitive dep via styrene-identity's async trait, but nex never spawns a runtime — it uses the sync `FileSigner::load()` API directly.)
- **`nex init` bootstraps everything.** Installs Determinate Nix + Homebrew if missing, scaffolds a minimal nix-darwin config with mac-app-util (or clones with `--from`), detects existing configs, warns about existing brew packages.
- **Spotlight integration.** Scaffold includes mac-app-util for Finder-alias-based .app bundles. After each switch, re-registers apps with LaunchServices for icon display.
- **Nix binary fallback.** exec.rs resolves nix from well-known paths (`/nix/var/nix/profiles/default/bin/nix`) when it's not yet in PATH (fresh init, same shell).

## Nix File Editing Contracts

The edit engine depends on these exact patterns in the target nix-darwin repo:

| List | Open pattern | Item indent | Quoting |
|------|-------------|-------------|---------|
| nix packages | `home.packages = with pkgs; [` | 4 spaces | bare |
| brews | `brews = [` | 6 spaces | quoted |
| casks | `casks = [` | 6 spaces | quoted |

The scaffold in `ops/init.rs` generates files that match these patterns exactly.

## Integration Tests

`tests/integration/` contains a containerized test suite (Docker/Podman) with mock binaries for nix, brew, darwin-rebuild, scutil, and sudo. 102 assertions across 10 test suites covering init, list, install, remove, revert, migrate, config resolution, and edge cases. Run via `just integration`.

## Site

`site/` contains an Astro project deployed to `nex.styrene.io` via Cloudflare Pages. The install script at `site/public/install.sh` is served at `https://nex.styrene.io/install.sh`. Install script is POSIX sh (no bashisms), installs prebuilt binary to `/usr/local/bin` (or `~/.local/bin` with auto PATH patching), with nix and cargo fallbacks. `_headers` disables CDN cache on install.sh.

## Release Flow

Tag `vX.Y.Z` triggers `.github/workflows/release.yml`: builds binaries for 4 targets (aarch64-darwin, x86_64-darwin, aarch64-linux, x86_64-linux), publishes to crates.io, creates GitHub release with tarballs.

## Identity

`nex identity` subcommand manages StyreneIdentity — a deterministic key hierarchy where one root secret derives SSH, git signing, age, and agent delegation keys via HKDF-SHA256. Depends on `styrene-identity` 0.1.1 from crates.io.

- `nex identity init` — generate `~/.config/styrene/identity.key` (argon2id + ChaCha20Poly1305 encrypted)
- `nex identity show` — display identity hash, signing pubkey, SSH host key, age key
- `nex identity link <url>` — enroll with a Signum hub (browser or invite code flow)

Identity hash = SHA-256(RNS-signing-Ed25519-pubkey) truncated to 16 bytes (32 hex chars). This is the canonical mesh identity used across Signum, styrened, and cross-service attribution.

Security hardware aliases in `aliases.rs` ensure `nex install ykman` (→ yubikey-manager), `fido2` (→ libfido2), `opensc`, `pcsc-tools`, etc. resolve correctly and cross-detect with brew formula names during adopt.

## Related Repos

- **macos-nix** — the nix-darwin config repo that nex edits
- **styrene-rs** — canonical home for `styrene-identity` and all `styrene-*` Rust crates
