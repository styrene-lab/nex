+++
id = "nex-repo-devenv-shell"
kind = "design_node"

[data]
title = "Use devenv for Nex repository development parity"
status = "exploring"
issue_type = "tooling"
priority = 3
parent = "nex-devenv-parallels"
dependencies = []
open_questions = [
  "Should the repo require devenv for contributors or keep it optional alongside flake devShell?",
  "Should CI run devenv tasks or keep direct cargo/nix commands?",
  "Can devenv run Linux/NixOS runtime validation from macOS builders, or do we still need Linux CI?"
]
+++

## Overview

Separately from Nex product design, the Nex repo itself can use `devenv.nix` to provide a reproducible contributor environment and validation task graph.

## Candidate devenv.nix

```nix
{ pkgs, ... }:

{
  languages.rust = {
    enable = true;
    toolchainFile = ./rust-toolchain.toml;
  };

  packages = [
    pkgs.pkl
    pkgs.jq
    pkgs.shellcheck
    pkgs.nix
  ];

  tasks."check:rust".exec = ''
    cargo fmt -- --check
    cargo clippy --all-targets -- -D warnings
    cargo test
  '';

  tasks."check:installer".exec = ''
    sh -n site/public/install.sh
    sh -n site/dist/install.sh
  '';

  tasks."check:nix-package".exec = ''
    nix build .#default --show-trace --print-build-logs
  '';
}
```

## Value

- Makes `pkl`, `jq`, shell validators, Rust toolchain, and Nix available together.
- Encodes project validation as tasks.
- Reduces ambient dependency bugs like the Pkl runtime issue.
- Gives agents/operators a single command surface for checks.

## Constraint

This is useful but not sufficient for the NixOS Pkl runtime bug. We still need a Linux/NixOS executable validation target because macOS cannot execute Linux `result/bin/nex` outputs.

## Decisions

- Proposed: add devenv support as optional contributor tooling, not a hard requirement.
- Proposed: CI can continue direct commands initially; after stabilization, a devenv task can mirror CI locally.
