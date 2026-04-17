# nex

Package manager for nix-darwin + homebrew. [nex.styrene.io](https://nex.styrene.io)

## Quick Start

```bash
# Install nex
curl -fsSL https://nex.styrene.io/install.sh | sh

# Bootstrap nix-darwin + homebrew on a fresh Mac
nex init

# Start using it
nex install htop
```

Or clone an existing config: `nex init --from https://github.com/your-org/macos-nix`

## Usage

```
nex init                      # Bootstrap nix + homebrew + nix-darwin config
nex install htop              # Install (auto-resolves nix vs cask vs brew)
nex install --cask slack      # Force Homebrew cask (GUI app)
nex install --brew rustup     # Force Homebrew formula
nex install --nix htop        # Force Nix (skip resolution)
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

## How It Works

`nex install slack` checks nixpkgs and Homebrew, compares versions, detects GUI apps vs CLI tools, and recommends the right source:

```
  slack found in multiple sources:

     nixpkgs         4.41.97
  *  brew cask       4.42.3

  recommended: GUI app — cask is 4.42.3 (nix has 4.41.97), with native .app bundle
```

For CLI tools found only in nixpkgs, it just installs — no prompt needed.

Every edit is backed up before `darwin-rebuild switch`. If the build fails, your config is restored automatically.

## Installation

```bash
curl -fsSL https://nex.styrene.io/install.sh | sh   # auto-detects best method
cargo install nex-pkg                                # from crates.io
nix profile install github:styrene-lab/nex           # from flake
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
