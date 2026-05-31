+++
id = "devenv-surface-awareness-catalog"
kind = "design_node"

[data]
title = "Maintain devenv surface awareness without becoming devenv"
status = "exploring"
issue_type = "compatibility"
priority = 2
parent = "nex-devenv-parallels"
dependencies = ["devenv-import-migration-path"]
open_questions = [
  "Should Nex vendor upstream options JSON snapshots, or generate them only in maintainer tooling?",
  "Should the Nex policy mapping live in Pkl, JSON, or Rust include_str data?",
  "How strict should release gating be when the upstream devenv catalog is stale?"
]
+++

## Overview

Nex should maintain awareness of devenv's real configuration surface while preserving Nex's own model: machine/profile lifecycle, safety gates, Forge planning, Armory distribution, hardware inventory, containers as outputs, and explicit secret contracts.

This requires a compatibility-awareness layer, not a reimplementation of devenv.

```text
devenv upstream surface
  -> normalized upstream catalog snapshot
  -> Nex policy mapping
  -> inspect / explain / plan / migrate behavior
```

## Upstream sources of truth

### devenv.nix options

Primary source:

```text
devenv docs/gen output: outputs.devenv-docs-options-json
```

In upstream `docs/gen/devenv.nix`, devenv builds docs with:

```nix
pkgs.nixosOptionsDoc {
  options = builtins.removeAttrs project.options [ "_module" ];
}
```

and exposes:

```nix
outputs = {
  devenv-docs-options = allOptions.optionsCommonMark;
  devenv-docs-options-json = allOptions.optionsJSON;
};
```

The generated JSON lands as a NixOS options document, normally under:

```text
share/doc/nixos/options.json
```

This is also what devenv's own search/MCP paths consume via `build_devenv(["optionsJSON"])`.

### devenv.yaml options

Primary source:

```text
https://devenv.sh/devenv.schema.json
```

Repo source:

```text
docs/src/devenv.schema.json
```

Current top-level schema surface includes:

```text
inputs, imports, nixpkgs, allowUnfree, allowBroken,
permittedInsecurePackages, clean, impure, backend,
profile, secretspec
```

## Nex-owned artifacts

### Upstream snapshots

```text
data/devenv/upstream/options.summary.json
data/devenv/upstream/devenv.schema.json
data/devenv/upstream/source.json
```

These are generated/updated by maintainer tooling and committed for deterministic tests.

`source.json` should record:

```json
{
  "schema": "io.styrene.nex.devenv-upstream-source.v1",
  "repo": "https://github.com/cachix/devenv",
  "rev": "<git-sha>",
  "reviewed_at": "<iso-date>",
  "options_source": "docs/gen outputs.devenv-docs-options-json",
  "yaml_schema_source": "docs/src/devenv.schema.json"
}
```

### Nex policy mapping

```text
data/devenv/nex-mapping.v1.pkl
```

This is the key file. It separates awareness from adoption.

Candidate structure:

```pkl
schema = "io.styrene.nex.devenv-mapping.v1"

mappings {
  ["packages"] {
    kind = "package"
    bucket = "portable"
    target = "profile.packages"
    safety = List("build")
    action = "generate-profile-fragment"
  }

  ["languages.*"] {
    kind = "language"
    bucket = "portable"
    target = "profile.fragments.dev"
    safety = List("build")
    action = "generate-profile-fragment"
  }

  ["services.*"] {
    kind = "service"
    bucket = "machine-scoped-candidate"
    target = "profile.services"
    safety = List("system-config-mutation")
    action = "manual-review"
    rationale = "devenv services are project-local; Nex services may become machine lifecycle state"
  }

  ["enterShell"] {
    kind = "shell-hook"
    bucket = "requires-review"
    target = "profile.shellHooks"
    safety = List("arbitrary-command")
    action = "manual-review"
  }

  ["containers.*"] {
    kind = "container"
    bucket = "portable"
    target = "profile.outputs.container"
    safety = List("build")
    action = "generate-profile-output"
  }

  ["secretspec"] {
    kind = "secret-contract"
    bucket = "portable"
    target = "profile.secrets"
    safety = List("secret-contract")
    action = "generate-secret-contract"
  }
}
```

## Unknown and unsupported surfaces

Nex must distinguish three states:

1. **Mapped** — known upstream surface with a Nex policy mapping.
2. **Known unsupported** — known upstream surface intentionally not migrated.
3. **Unknown** — upstream surface not classified by Nex yet.

Unknowns indicate drift and should be visible in maintainer tools and migration reports.

Example diagnostic:

```text
DEVENV_SURFACE_UNKNOWN: upstream option languages.nim.enable has no Nex mapping
```

## Commands

### Catalog inspection

```text
nex devenv catalog list
nex devenv catalog list --json
```

Shows Nex's current mapping catalog.

### Drift check

```text
nex devenv catalog check
nex devenv catalog check --upstream .scratch/devenv-src
nex devenv catalog check --json
```

Compares:

```text
upstream options.summary.json + devenv.schema.json
against
Nex nex-mapping.v1.pkl
```

Reports:

```text
mapped: count
known unsupported: count
unknown: count
removed upstream: count
stale review: yes/no
```

### Maintainer update script

```text
scripts/update-devenv-surface-catalog.sh
```

Responsibilities:

1. Fetch/clone `https://github.com/cachix/devenv` at a pinned rev or branch.
2. Copy `docs/src/devenv.schema.json`.
3. Try to generate `outputs.devenv-docs-options-json` from `docs/gen` using devenv.
4. If generation is unavailable, parse `docs/src/reference/options.md` as a fallback with lower confidence.
5. Emit normalized snapshots under `data/devenv/upstream/`.
6. Preserve the Nex policy mapping separately.

## CI posture

### PR CI

Validate:

```text
cargo test devenv_surface
nex devenv catalog list --json
nex devenv catalog check --offline --json
```

No network required.

### Scheduled CI

Weekly or manual workflow:

```text
scripts/update-devenv-surface-catalog.sh --check-only
```

If drift exists, create or update an issue. Do not fail unrelated PRs.

### Release gate

Warn if:

- upstream catalog review is older than 30 days
- unknown count is non-zero

Block release only if:

- mapping file is invalid
- current adapter tests fail
- known Nex migration behavior contradicts mapping policy

## Implementation slices

### Slice 1 — Static catalog files

Add:

```text
data/devenv/nex-mapping.v1.pkl
data/devenv/upstream/source.json
data/devenv/upstream/devenv.schema.json
```

Keep current Rust hardcoded detection temporarily, but add tests proving the catalog parses.

### Slice 2 — Catalog loader

Add:

```text
src/devenv_surface.rs
```

Responsibilities:

- parse mapping catalog
- expose pattern matching (`packages`, `languages.*`, `services.*`)
- expose mapping entries to CLI

### Slice 3 — Use mapping in inspect/plan

Replace hardcoded mapping tuples in `src/devenv_import.rs` with catalog-driven classification.

### Slice 4 — Drift tooling

Add:

```text
nex devenv catalog list
nex devenv catalog check
scripts/update-devenv-surface-catalog.sh
```

### Slice 5 — CI issue/report loop

Add scheduled workflow that reports upstream drift.

## Decisions

- Use devenv's generated `optionsJSON` as the primary `devenv.nix` surface source.
- Use `https://devenv.sh/devenv.schema.json` / `docs/src/devenv.schema.json` as the primary `devenv.yaml` surface source.
- Keep upstream surface knowledge separate from Nex policy mapping.
- Treat unknown upstream surfaces as drift, not as automatic migration candidates.
- Do not claim Nex executes devenv projects identically; claim Nex can inspect, explain, plan, and migrate the safe subset with explicit review boundaries.
