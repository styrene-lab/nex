+++
id = "nex-devenv-parallels"
kind = "design_node"

[data]
title = "Leverage devenv patterns for Nex machine profiles"
status = "exploring"
issue_type = "architecture"
priority = 2
parent = "hardware-purpose-profile-matrix"
dependencies = []
open_questions = [
  "Should Nex profile modules remain Pkl-native or generate/consume Nix module fragments directly?",
  "Should Nex expose a public task DAG command or keep task orchestration internal behind profile/apply/test commands?",
  "How much of devenv's process/service supervision belongs in Nex versus profile-generated systemd/nix-darwin services?",
  "Should Nex support a `devenv.nix` for its own repository separately from Nex machine-profile design?"
]
+++

## Overview

Research `devenv.sh` as a nearby system and identify patterns Nex can leverage without becoming a generic project dev-environment tool.

## What devenv is

`devenv.sh` describes itself as fast, declarative, reproducible, and composable developer environments. It provides a Nix-module-based UX around project-local environments:

- `devenv.nix` for declarative environment definition
- `packages` and language modules (`languages.rust.enable`, etc.)
- `tasks` as a dependency graph
- `processes` with supervision/readiness/dependencies
- prebuilt `services.*` modules
- `devenv test`
- `devenv info`
- generated containers and outputs
- binary cache integration
- option search/docs
- auto-activation hooks
- SecretSpec integration
- MCP server for option search

## Parallel with Nex

Devenv solves:

> Make this repository's development/runtime environment reproducible.

Nex solves or is moving toward:

> Make this machine's profile, install, and operational state reproducible.

The shared substrate is not just Nix; it is a UX pattern:

- declarative module composition
- explainable options
- generated docs and summaries
- task DAGs for lifecycle actions
- environment/profile tests
- outputs derived from one declarative source
- secrets declared separately from where they are provisioned

## What Nex should borrow

### Profile modules

Nex profiles should feel like composed modules rather than disconnected artifacts. A profile should compose packages, services, hardware fragments, purpose, safety policy, and materialization outputs.

Candidate shape:

```pkl
purpose = "gaming"
target = "existing-nixos"
imports = List(
  "hardware/gpu/amd",
  "purpose/gaming",
  "dev/rust",
  "security/yubikey"
)
packages = List("ripgrep", "steam", "mangohud")
services.ssh.enable = true
```

### Info/test/explain

Borrow `devenv info` and `devenv test` as Nex profile surfaces:

```text
nex profile info <profile>
nex profile explain <profile>
nex profile test <profile>
```

These should hide artifact complexity and answer:

- what does this profile do?
- what hardware does it assume?
- what secrets does it need?
- what will be built/applied?
- what safety gates exist?
- what tasks/checks pass or fail?

### Task DAG

Use a task DAG internally for profile lifecycle:

```text
hardware:scan -> profile:evaluate -> profile:match -> secrets:check -> forge:plan -> forge:preflight -> materialization:build -> apply
```

Expose later if useful:

```text
nex tasks run profile:test
nex tasks graph profile:apply
```

### Option schema and docs

Devenv's option docs/search are a major UX win. Nex should define profile options as structured metadata so it can generate:

- CLI option search
- docs
- profile editor/UI forms
- validation rules
- AI tool context

Candidate commands:

```text
nex profile options search gpu
nex profile options show hardware.gpu.amd
nex profile explain --option services.ssh.enable
```

### Outputs

Devenv outputs let one declarative environment produce build artifacts. Nex should do the same for machine profiles:

```text
nex profile build --output module
nex profile build --output installer
nex profile build --output sd-image
nex profile build --output vm
nex profile build --output container
```

## What Nex should not copy

- Nex should not become a generic project-local development environment manager.
- Nex should not duplicate devenv's service/process runner except where process orchestration is needed for preflight/test flows.
- Nex should not make every profile action an ad-hoc CLI option; ad-hoc experimentation should be convertible into explicit profile artifacts.

## Decisions

- Proposed: borrow devenv's module/options/tasks/info/test/explain patterns, not its exact domain boundary.
- Proposed: `nex profile explain` is the highest-leverage first operator-facing feature.
- Proposed: `nex profile test` should become the lifecycle gate before profile apply/materialization.
- Proposed: option metadata should be canonical enough to generate docs, validation, and UI/AI surfaces.
