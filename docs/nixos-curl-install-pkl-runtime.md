+++
id = "nixos-curl-install-pkl-runtime"
kind = "design_node"

[data]
title = "Fix NixOS curl install Pkl runtime failures"
status = "implementing"
issue_type = "bug"
priority = 1
parent = "nex-pkl-local-config"
dependencies = []
open_questions = []
+++

## Overview

`curl -fsSL https://nex.styrene.io/install.sh | sh` on NixOS can install Nex through the flake path but leave runtime Pkl evaluation dependent on an ambient `pkl` binary or a working `nix shell nixpkgs#pkl` fallback. That is fragile for NixOS machines such as `nex-gaminpc`; the installed Nex package should carry the Pkl evaluator in its runtime closure.

## Evidence

- The installer detects NixOS and prefers `nix profile add github:styrene-lab/nex --refresh`.
- `flake.nix` includes `pkl` only in `devShells.default.buildInputs`; the package does not wrap `nex` with `pkl` on `PATH`.
- `src/pkl.rs` runtime evaluator tries `$NEX_PKL`, then `pkl`, then `nix shell nixpkgs#pkl -c pkl ...`.
- Release artifacts are Linux `*-unknown-linux-gnu`, but `site/public/install.sh` initially selects `*-unknown-linux-musl` and only falls back to GNU after a failed HEAD request.
- On NixOS, the installer refuses GNU fallback because dynamically linked GNU tarballs are not NixOS-compatible, so the Nix profile path is the only intended path.

## Decisions

- Wrap the flake-built `nex` binary with `pkgs.pkl` on `PATH` so Pkl evaluation works after Nix profile installation without requiring ambient `pkl`.
- Do not include `pkgs.nix` in the wrapper path for now. NixOS already has Nix in the environment; the critical missing runtime dependency is Pkl.
- Change Linux installer targets to the release artifacts actually produced: `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu`.
- Keep the existing NixOS guard that refuses GNU prebuilt binaries; NixOS should install through the flake package.
- Add a Nix build validation that runs a command requiring Pkl evaluation with an empty ambient `PATH` except for the wrapped binary.

## Implementation Plan

1. Patch `flake.nix` package:
   - add `nativeBuildInputs = [ pkgs.makeWrapper ];`
   - add `postInstall` wrapping `$out/bin/nex` with `--prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.pkl ]}`

2. Patch installer scripts:
   - `site/public/install.sh`
   - `site/dist/install.sh`
   - Linux targets should select GNU artifacts directly.

3. Validate:
   - `nix build .#default --show-trace --print-build-logs`
   - create a temporary `.pkl` forge request
   - run `env -i HOME=$HOME PATH=$(pwd)/result/bin result/bin/nex forge plan --request <temp.pkl>` to prove wrapped Pkl is available
   - run shell syntax checks for installer scripts
   - run `cargo fmt -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`

## Release Assessment Checklist

- Nix profile package includes Pkl runtime closure.
- NixOS curl install still prefers Nix and does not try GNU tarballs.
- Non-NixOS Linux installer points at artifacts that actually exist.
- Pkl runtime validation passes without ambient `pkl` on PATH.
- No regressions in Rust validation.
