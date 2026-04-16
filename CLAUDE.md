# CLAUDE.md

## What This Is

Rust CLI that provides imperative package management UX (`nex install foo`) on top of declarative nix-darwin configuration. Edits `.nix` files, runs `darwin-rebuild switch`, and reverts on failure.

## Architecture

Single crate, no workspace. Key modules:

- `cli.rs` — clap derive structs, all subcommands
- `config.rs` — config resolution: CLI flags → env vars → config file → auto-discovery
- `discover.rs` — find the nix-darwin repo and hostname
- `nixfile.rs` — `NixList` model describing editable lists in nix files (open/close patterns, indent, quoting)
- `edit.rs` — line-based nix file editing: contains, insert, remove, list_packages, backup/restore
- `exec.rs` — subprocess wrappers for nix and darwin-rebuild
- `ops/` — one module per subcommand (install, remove, list, search, switch, update, rollback, try, diff, gc)
- `output.rs` — colored terminal output helpers

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
- **Synchronous.** No tokio. File I/O and subprocesses are blocking.
- **No smart routing.** Default is nix. Use `--cask` or `--brew` explicitly.

## Nix File Editing Contracts

The edit engine depends on these exact patterns in the target nix-darwin repo:

| List | Open pattern | Item indent | Quoting |
|------|-------------|-------------|---------|
| nix packages | `home.packages = with pkgs; [` | 4 spaces | bare |
| brews | `brews = [` | 6 spaces | quoted |
| casks | `casks = [` | 6 spaces | quoted |

## Related Repos

- **macos-nix** — the nix-darwin config repo that nex edits
