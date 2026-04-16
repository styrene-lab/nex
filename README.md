# nex

Package manager UX for nix-darwin + homebrew.

## Overview

`nex` wraps the declarative nix-darwin workflow with imperative commands. Instead of editing `.nix` files and running `darwin-rebuild switch` manually, you run `nex install htop` and it handles the file edit, validation, rebuild, and rollback on failure.

## Usage

```
nex install htop              # Nix package (default)
nex install --cask slack      # Homebrew cask (GUI app)
nex install --brew rustup     # Homebrew formula
nex remove htop               # Remove from wherever it's declared
nex search "http"             # Search nixpkgs
nex list                      # Show all declared packages
nex update                    # Update flake inputs + switch
nex switch                    # Rebuild and activate
nex rollback                  # Revert to previous generation
nex try htop                  # Ephemeral nix shell (no install)
nex diff                      # Preview what would change
nex gc                        # Garbage collect nix store
```

Global flags: `--dry-run`, `--repo <path>`, `--hostname <name>`

## Installation

```bash
cargo install nex-pkg       # from crates.io
# or
cargo install --path .      # from source
```

## Configuration

`nex` auto-discovers the nix-darwin repo by walking up from CWD or checking well-known paths. Override with:

- `NEX_REPO` environment variable
- `--repo` flag
- `~/.config/nex/config.toml`:
  ```toml
  repo_path = "/path/to/macos-nix"
  hostname = "My-MacBook"
  ```

## Development

```bash
just validate   # format-check + lint + test
just test       # cargo test
just lint       # cargo clippy
just build      # debug build
```

## License

[MIT](LICENSE-MIT) OR [Apache-2.0](LICENSE-APACHE)
