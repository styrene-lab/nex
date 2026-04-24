# nex

Package manager for nix-darwin, NixOS, and homebrew. [nex.styrene.io](https://nex.styrene.io)

## Quick Start

```bash
# Install nex
curl -fsSL https://nex.styrene.io/install.sh | sh

# Bootstrap nix-darwin + homebrew (macOS) or NixOS (Linux)
nex init

# Start using it
nex install htop
```

Or clone an existing config: `nex init --from https://github.com/your-org/macos-nix`

## Usage

### Packages

```
nex install htop              # Auto-resolves nix vs cask vs brew
nex install --cask slack      # Force Homebrew cask (GUI app)
nex install --brew rustup     # Force Homebrew formula
nex install --nix htop        # Force Nix (skip resolution)
nex remove htop               # Remove from wherever it's declared
nex search "http"             # Search nixpkgs
nex list                      # Show all declared packages
```

### System

```
nex init                      # Bootstrap nix + homebrew + system config
nex switch                    # Rebuild and activate
nex update                    # Update flake inputs + switch
nex rollback                  # Revert to previous generation
nex doctor                    # Check and fix config issues
nex relocate                  # Move system config to user-writable directory
```

### Profiles

```
nex profile apply user/repo   # Apply a machine profile from GitHub
nex forge user/profile        # Burn a bootable NixOS USB with profile baked in
nex polymerize                # Interactive NixOS installer (on target machine)
nex build-image user/profile  # Build an OCI container from a profile
```

### Development

```
nex develop ./path/to/flake   # Enter a flake dev shell
nex dev user/repo             # Dev shell + omegon AI agent
nex try htop                  # Ephemeral nix shell (no install)
nex diff                      # Preview what would change
nex gc                        # Garbage collect nix store
nex self-update               # Update nex itself
```

Global flags: `--dry-run`, `--repo <path>`, `--hostname <name>`

## How It Works

`nex install slack` checks nixpkgs and Homebrew, compares versions (semver-aware), detects GUI apps vs CLI tools, and recommends the right source:

```
  slack found in multiple sources:

     nixpkgs         4.41.97
  *  brew cask       4.42.3

  recommended: brew cask is 4.42.3 (nix has 4.41.97)
```

For CLI tools found only in nixpkgs, it just installs -- no prompt needed.

Every edit is backed up before rebuild. If the build fails, your config is restored automatically.

## Profiles

Define a machine with a TOML file:

```toml
[packages]
nix = ["htop", "bat", "eza", "ripgrep"]
casks = ["firefox", "kitty"]

[shell]
aliases = { ll = "eza -la", cat = "bat" }

[git]
name = "Your Name"
email = "you@example.com"

[macos]
dock_autohide = true
tap_to_click = true
```

Profiles compose via `extends` for team/personal layering. Apply with `nex profile apply user/repo`.

## Installation

```bash
curl -fsSL https://nex.styrene.io/install.sh | sh   # auto-detects best method
cargo install nex-pkg                                # from crates.io
nix profile install github:styrene-lab/nex           # from flake
```

## Configuration

`nex` auto-discovers the config repo by walking up from CWD or checking well-known paths. Override with:

- `NEX_REPO` environment variable
- `--repo` flag
- `~/.config/nex/config.toml`:
  ```toml
  repo_path = "/path/to/config-repo"
  hostname = "My-MacBook"
  prefer_nix_on_equal = true
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
