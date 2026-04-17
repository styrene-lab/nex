# CLAUDE.md

## What This Is

Rust CLI that provides imperative package management UX (`nex install foo`) on top of declarative nix-darwin configuration. Edits `.nix` files, runs `darwin-rebuild switch`, and reverts on failure. Published as `nex-pkg` on crates.io; binary name is `nex`.

## Architecture

Single crate, no workspace. Key modules:

- `cli.rs` — clap derive structs, all subcommands
- `config.rs` — config resolution: CLI flags -> env vars -> config file -> auto-discovery
- `discover.rs` — find the nix-darwin repo and hostname
- `resolve.rs` — multi-source resolution: checks nixpkgs + brew cask + brew formula, recommends best source, interactive prompt for conflicts
- `nixfile.rs` — `NixList` model describing editable lists in nix files (open/close patterns, indent, quoting)
- `edit.rs` — line-based nix file editing: contains, insert, remove, list_packages, backup/restore
- `exec.rs` — subprocess wrappers for nix, darwin-rebuild, and brew version queries
- `output.rs` — colored terminal output helpers
- `ops/init.rs` — bootstrap: installs nix + homebrew, scaffolds or clones nix-darwin config, runs first switch
- `ops/install.rs` — install with auto-resolution, duplicate detection, atomic revert on failure
- `ops/` — remaining subcommands: remove, list, search, switch, update, rollback, try, diff, gc

## Commands

```bash
just validate     # format-check + lint + test
just test         # cargo test
just lint         # cargo clippy -- -D warnings
just format       # cargo fmt
just build        # debug build
just install      # cargo install --path .
```

## Key Design Decisions

- **Line-based editing, not a nix parser.** Matches on known list patterns (`home.packages = with pkgs; [` ... `];`). Works because we control the file format.
- **Atomic edits.** Backup before edit, revert all on failed switch, delete backups on success.
- **Smart resolution.** `nex install slack` checks nixpkgs and brew, detects GUI apps (cask presence = GUI), compares versions, recommends. CLI tools auto-install from nix with no prompt.
- **Synchronous.** No tokio. File I/O and subprocesses are blocking.
- **`nex init` bootstraps everything.** Installs Determinate Nix + Homebrew if missing, scaffolds a minimal nix-darwin config (or clones with `--from`), runs first `darwin-rebuild switch`.

## Nix File Editing Contracts

The edit engine depends on these exact patterns in the target nix-darwin repo:

| List | Open pattern | Item indent | Quoting |
|------|-------------|-------------|---------|
| nix packages | `home.packages = with pkgs; [` | 4 spaces | bare |
| brews | `brews = [` | 6 spaces | quoted |
| casks | `casks = [` | 6 spaces | quoted |

The scaffold in `ops/init.rs` generates files that match these patterns exactly.

## Site

`site/` contains an Astro project deployed to `nex.styrene.io` via Cloudflare Pages. The install script at `site/public/install.sh` is served at `https://nex.styrene.io/install.sh`.

## Related Repos

- **macos-nix** — the nix-darwin config repo that nex edits
