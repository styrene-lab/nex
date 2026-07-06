---
id: machine-capsule-backends
title: "Machine capsule backend architecture"
status: exploring
tags: [nex, machine-profile, nix, backends]
open_questions:
  - "[assumption] Nex should use ordinary flakes as the applied config substrate rather than requiring flake-parts, Snowfall Lib, Blueprint, deploy-rs, or Colmena."
  - "[assumption] A machine capsule should be one checkout per concrete machine containing both profile.toml and a config/ flake tree."
  - "[assumption] Backend execution should remain an internal enum/command registry initially, not a dynamic plugin ABI."
dependencies: []
related: []
---

# Machine capsule backend architecture

## Overview

Define how Nex machine capsules plug into existing Nix ecosystem backends after the `nex` binary is installed.

The boundary is:

> Nex owns intent, capsule metadata, provider-target mutation, and reviewable adoption plans. Existing Nix ecosystem tools own build, activation, deployment, bootstrap, disk layout, and secrets.

Nex should not become a replacement for NixOS modules, Home Manager, nix-darwin, `nixos-rebuild`, `darwin-rebuild`, `nixos-anywhere`, `disko`, `deploy-rs`, Colmena, `sops-nix`, or agenix. It should select and invoke those tools only where their lifecycle phase applies.

## Core decisions

### Decision: plain flakes are the default applied substrate

Use ordinary flakes for machine capsule applied config:

```text
machine-capsule/
  profile.toml
  nex.lock                  # future: pinned profile/materialization refs
  .nex/machine.toml         # future/current capsule metadata
  config/
    flake.nix
    flake.lock
    hosts/<hostname>/default.nix
    modules/home/packages.nix
    modules/nixos/packages.nix
    modules/darwin/homebrew.nix
```

Default flake outputs should be standard Nix outputs:

```nix
nixosConfigurations.<hostname>
darwinConfigurations.<hostname>
homeConfigurations."<user>@<hostname>"  # only for user-only mode later
```

`flake-parts`, Snowfall Lib, and Blueprint are not default backends. They may be supported later as optional render styles/import targets, but the first machine capsule implementation should be readable plain flakes.

Rationale:

- plain flakes are the ecosystem compatibility layer;
- users can debug and run them manually;
- `nixos-rebuild`, `darwin-rebuild`, `nixos-anywhere`, `deploy-rs`, and Colmena can all consume normal flake outputs;
- framework-specific layouts would stack abstractions before Nex has proven the capsule model.

### Decision: Forge is capsule birth/adoption, not a deployment framework

Forge should create or update a machine capsule from a reviewable plan. It should not become the long-term executor of the capsule.

Minimal Forge lifecycle:

```text
nex forge plan --mode adopt-existing --target-host <host>
  -> inspect target and emit plan

nex forge apply --plan <plan>
  -> create/update local capsule files only

nex deploy / nex switch
  -> apply an existing capsule through selected backend
```

Forge may inspect a target over SSH and create local files. For the first implementation it should not run `nixos-anywhere`, `disko`, `deploy-rs`, Colmena, secret provisioning, or destructive remote mutation.

For an existing NixOS host such as `nucleus`, the default mode is:

```text
mode = adopt-existing
bootstrap = none
disk = none
deployment = nixos-rebuild --target-host
homebrew = disabled unless explicitly configured
```

Forge must explicitly distinguish `adopt-existing` from `install/reimage`. Any destructive disk action requires a plan that names the disks and an explicit approval gate.

### Decision: backend execution starts as an internal enum/command registry

Do not introduce a dynamic backend plugin ABI yet. Start with typed internal variants and direct command construction.

Initial conceptual categories:

```rust
enum ActivationBackend {
    NixosRebuildLocal,
    DarwinRebuildLocal,
}

enum DeploymentBackend {
    NixosRebuildTargetHost,
}

enum BootstrapBackend {
    None,
    DarwinBootstrap,
}

enum PackageProvider {
    Nixpkgs,
    HomebrewFormula,
    HomebrewCask,
}
```

Future variants may add `NixosAnywhere`, `DeployRs`, `Colmena`, `Disko`, `SopsNix`, or `Agenix`, but only after the capsule/adoption path proves itself.

## Backend redundancy assessment

The backend plan should stay deliberately small. Many candidate tools overlap at the Nex abstraction level even though they remain useful in the broader Nix ecosystem.

### Keep in the first machine-capsule slice

| Category | Backend/tool | Why keep |
|---|---|---|
| Flake substrate | plain flakes | Ecosystem-compatible baseline; no extra framework |
| Local activation | `nixos-rebuild switch --flake .#host` | Native local NixOS activation |
| Local activation | `darwin-rebuild switch --flake .#host` | Native local macOS/nix-darwin activation |
| Remote deployment | `nixos-rebuild --target-host` | Simplest remote NixOS deployment for one machine |
| Bootstrap | existing Darwin bootstrap path | Needed for macOS setup path already in Nex |
| Bootstrap | `none` | Correct default for adopt-existing NixOS machines |
| Package provider | Nixpkgs | Primary package provider |
| Package provider | Homebrew formula/cask | Valid when explicit provider targets exist, especially macOS |

### Defer from the first slice

| Candidate | Overlapped by | Verdict |
|---|---|---|
| `deploy-rs` | `nixos-rebuild --target-host` for one-machine remote deploys | Defer until flake-native deploy metadata/checks are needed |
| Colmena | `nixos-rebuild --target-host` / `deploy-rs` for one-machine deploys | Defer until Nex has a fleet/capsule-group concept |
| `nixos-anywhere` | none for fresh install, but not needed for adopt-existing | Defer until install/reimage flows exist |
| `disko` | none for disk layout, but only relevant to install/reimage | Defer until destructive/bootstrap flows exist |
| standalone Home Manager | Home Manager as NixOS/nix-darwin module for system capsules | Defer until user-only/non-NixOS Linux mode exists |
| `sops-nix` | agenix at Nex abstraction level | Defer; choose one default later if secrets are needed |
| agenix | `sops-nix` at Nex abstraction level | Defer; do not support both in v1 |
| `flake-parts` | plain flakes | Optional later; not default |
| Snowfall Lib | plain flakes / flake-parts | Do not make default |
| Blueprint | plain flakes / flake-parts | Do not make default |

### Redundancy conclusions

- `deploy-rs` and Colmena are redundant for one-machine capsules; keep `nixos-rebuild --target-host` as the first remote deploy backend.
- `nixos-anywhere` and `disko` are not conceptually redundant, but they belong to install/reimage, not adopt-existing. They are out of scope for the first Forge implementation.
- `sops-nix` and agenix overlap strongly. Do not support both initially; prefer no secrets backend until a concrete requirement appears.
- `flake-parts`, Snowfall Lib, and Blueprint all overlap with plain flakes as capsule render frameworks. Use plain flakes first.
- standalone Home Manager is redundant for system capsules where Home Manager is imported as a NixOS/nix-darwin module. Add it only for user-only mode.

## Machine capsule shape

A concrete machine should have one capsule checkout:

```text
nex-nucleus/
  profile.toml
  .nex/machine.toml
  config/
    flake.nix
    flake.lock
    hosts/nucleus/default.nix
    hosts/nucleus/hardware-configuration.nix
    modules/home/packages.nix
    modules/nixos/packages.nix
```

Possible `.nex/machine.toml` shape:

```toml
[machine]
hostname = "nucleus"
platform = "nixos"
arch = "x86_64-linux"
mode = "adopt-existing"

[capsule]
profile = "../profile.toml"
config = "../config"

[providers.nix]
home_packages_target = "config/modules/home/packages.nix"
system_packages_target = "config/modules/nixos/packages.nix"

[providers.homebrew]
enable = false

[activation]
backend = "nixos-rebuild-local"

[deployment]
backend = "nixos-rebuild-target-host"
target_host = "wilson@192.168.0.100"
use_remote_sudo = true

[bootstrap]
backend = "none"

[disk]
backend = "none"
```

This config is intentionally boring. It records what Nex needs to know without replacing the flake or deployment tool.

## Forge plan shape

For `nucleus`, Forge should emit a reviewable plan like:

```text
Forge plan: nucleus

Target:
  host: wilson@192.168.0.100
  platform: NixOS
  arch: x86_64-linux
  mode: adopt-existing

Capsule:
  path: ~/workspace/pig/nex-nucleus
  profile: profile.toml
  flake: config/flake.nix

Backends:
  activation: nixos-rebuild-local
  deployment: nixos-rebuild-target-host
  bootstrap: none
  disk: none
  secrets: none

Provider targets:
  nix home packages: config/modules/home/packages.nix
  nix system packages: config/modules/nixos/packages.nix
  homebrew: disabled

Will create:
  profile.toml
  .nex/machine.toml
  config/flake.nix
  config/hosts/nucleus/default.nix
  config/modules/home/packages.nix
  config/modules/nixos/packages.nix

Will not:
  repartition disks
  overwrite /etc/nixos
  enable Homebrew
  deploy without approval
```

## Phased implementation

### Phase 1: plan-only Forge adoption

Command:

```bash
nex forge plan nucleus --target-host wilson@192.168.0.100 --mode adopt-existing
```

Responsibilities:

- SSH probe target hostname, platform, arch, NixOS marker, and basic config presence;
- produce human-readable plan;
- produce machine-readable JSON plan;
- no local file mutation;
- no remote mutation.

### Phase 2: apply plan to create local capsule

Command:

```bash
nex forge apply --plan forge-plan.json
```

Responsibilities:

- create local capsule files;
- scaffold plain flake;
- write provider target modules;
- write `.nex/machine.toml`;
- no remote activation.

### Phase 3: deploy existing capsule

Command:

```bash
nex deploy
```

Initial backend:

```bash
nixos-rebuild switch --flake .#<hostname> --target-host <host> --use-remote-sudo
```

### Phase 4: future install/reimage backends

Only after adoption/deploy is working:

- add `nixos-anywhere`;
- add `disko`;
- add explicit destructive approval gates.

### Phase 5: optional ecosystem integrations

Only when requirements justify them:

- `deploy-rs` for flake-native deploy metadata/checks;
- Colmena for fleet/capsule groups;
- `sops-nix` or agenix for secrets;
- `flake-parts` as optional render style.

## Open Questions

- [assumption] Nex should use ordinary flakes as the applied config substrate rather than requiring flake-parts, Snowfall Lib, Blueprint, deploy-rs, or Colmena.
- [assumption] A machine capsule should be one checkout per concrete machine containing both profile.toml and a config/ flake tree.
- [assumption] Backend execution should remain an internal enum/command registry initially, not a dynamic plugin ABI.
- What assumptions is this design making that haven't been stated?
