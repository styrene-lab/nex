+++
id = "nex-devenv-container-strategy"
kind = "design_node"

[data]
title = "Leverage devenv containers for Nex validation and profile artifacts"
status = "exploring"
issue_type = "architecture"
priority = 2
parent = "nex-devenv-parallels"
dependencies = ["nex-profile-output-builds", "nex-repo-devenv-shell"]
open_questions = [
  "Should Nex containers be primarily repo validation containers, profile output containers, or both?",
  "Which container runtime targets must be supported first: Docker, Podman, OCI registry copy?",
  "Can profile containers safely represent only process/application subsets, or should full-machine profiles always stay VM/image/module outputs?",
  "How should secrets be injected into containers without embedding them in images?"
]
+++

## Overview

Devenv can build OCI containers from a declarative development environment:

```text
devenv container build shell
devenv container build processes
devenv container run shell
devenv container run processes
devenv container --registry docker://ghcr.io/org copy <name>
```

Nex should leverage this pattern in two ways:

1. **Repository validation containers**: reproducible containers for Nex's own validation/runtime tests.
2. **Profile output containers**: optional OCI outputs derived from Nex profiles where the target is a service/process subset rather than a whole machine.

## Use case 1 — Nex repository validation containers

The immediate value is validation parity. A `devenv.nix` for this repo can define containers that exercise the same toolchain used by contributors and CI.

Candidate containers:

```nix
containers."nex-dev-shell" = {
  name = "nex-dev-shell";
  startupCommand = "devenv shell";
};

containers."nex-validate" = {
  name = "nex-validate";
  startupCommand = "devenv tasks run check";
};

containers."nex-pkl-runtime" = {
  name = "nex-pkl-runtime";
  startupCommand = "devenv tasks run check:pkl-runtime";
};
```

This helps catch failures like:

- ambient `pkl` exists locally but not in packaged/runtime environment
- shell scripts parse locally but not in minimal Linux environment
- Nix package closure misses runtime tools

## Use case 2 — Profile output containers

Some Nex profiles will represent services/app bundles that can be containerized. Devenv's output/container model suggests adding:

```text
nex profile build <profile> --output container
nex profile container build <profile>
nex profile container run <profile>
nex profile container publish <profile> --registry ghcr.io/org/name
```

Examples:

- local service stack for a workstation profile
- agent/daemon component of a machine profile
- preflight/test harness for a profile
- reproducible environment for a remote apply operation

## Boundary: containers are not machines

Containers should not pretend to replace machine profiles. Whole-machine concerns still need NixOS/nix-darwin modules, VM images, SD images, or installer media.

Container output is appropriate for:

- process subsets
- validation harnesses
- development shells
- service stacks
- portable CLI/runtime bundles

Container output is not appropriate for:

- disk partitioning
- bootloader installation
- kernel/driver-level hardware configuration
- destructive install flows

## Secrets interaction

Containers must follow the same SecretSpec-style rule:

- secret contracts can be in images
- secret values must not be baked into images
- runtime should use provider injection, environment files, mounted secret files, or orchestrator-native secrets

Candidate flow:

```text
nex profile container build service-profile
nex secrets check service-profile --provider keyring
nex secrets run service-profile -- docker run ...
```

Or with generated runtime guidance:

```text
nex profile container run service-profile --secrets-provider keyring
```

## OCI conventions

Use existing project OCI conventions:

- `Containerfile` naming when authoring explicit files
- Podman-compatible commands where possible
- immutable tags
- no secrets in layers
- multi-arch where release needs it
- attach SBOM/signature later

## Proposed implementation phases

### Phase 1 — repo validation devenv containers

- Add optional `devenv.nix` to the Nex repo.
- Define tasks for Rust checks, installer shell checks, Nix package build, Pkl runtime validation.
- Define at least one container that runs validation tasks.

### Phase 2 — profile output vocabulary

- Add `container` as a named profile output in design/report schemas.
- `nex profile explain` should show when a profile has a containerizable process subset.
- `nex profile outputs` should list container outputs separately from machine outputs.

### Phase 3 — generated container builds

- Start with container generation for process/profile subsets only.
- Do not support full-machine containerization claims.
- Wire SecretSpec-style secret contracts into runtime docs/checks, not image layers.

## Decisions

- Proposed: use devenv containers first for repo validation parity, then adopt the pattern for Nex profile outputs.
- Proposed: add `container` as a first-class profile output type, but clearly mark it as process/service scoped rather than machine scoped.
- Proposed: container builds must never include secret values; only secret names/contracts may be embedded.
- Proposed: profile container outputs should integrate with Armory artifact metadata eventually.
