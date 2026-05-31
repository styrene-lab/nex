+++
id = "devenv-surface-catalog-implementation"
kind = "design_node"

[data]
title = "Implement devenv surface catalog and drift checks"
status = "exploring"
issue_type = "implementation-plan"
priority = 2
parent = "devenv-surface-awareness-catalog"
dependencies = ["devenv-import-migration-path"]
open_questions = [
  "Can CI reliably run devenv to generate optionsJSON, or should it use a vendored upstream snapshot?",
  "Should mapping catalog be Pkl-only, or should Rust embed a generated JSON form for release binaries?",
  "What stale-catalog threshold should warn vs block release?"
]
+++

## Overview

Implementation plan for making Nex's devenv adapter catalog-driven and drift-aware.

## Phase 1 — Commit catalog skeleton

Files:

```text
data/devenv/nex-mapping.v1.pkl
data/devenv/upstream/source.json
data/devenv/upstream/devenv.schema.json
```

Tasks:

- Encode current hardcoded mapping in Pkl.
- Vendor current `devenv.schema.json` from upstream.
- Record upstream repo/rev/review metadata.
- Add parser tests that validate all mapping entries can be read.

Acceptance:

```text
cargo test devenv_surface
```

## Phase 2 — Catalog loader

Files:

```text
src/devenv_surface.rs
src/devenv_import.rs
```

Tasks:

- Add typed mapping structs.
- Load mapping via Pkl evaluator or generated JSON fallback.
- Implement glob-ish matching:
  - exact: `packages`
  - prefix wildcard: `languages.*`, `services.*`, `containers.*`
- Keep deterministic ordering.

Acceptance:

- `packages` maps to portable profile fragment.
- `languages.rust.enable` maps to portable dev fragment.
- `services.postgres.enable` maps to machine-scoped review.
- `enterShell` maps to requires-review/manual-review.
- `containers.shell` maps to profile output.

## Phase 3 — Replace hardcoded import tuples

Tasks:

- Remove hardcoded `(needle, kind, bucket, target, safety)` tuples from `inspect_devenv_nix`.
- Use catalog patterns to classify detected option-like tokens.
- Continue supporting the current simple textual scanner until a real Nix option extractor exists.

Acceptance:

```text
nex devenv inspect sample --json
nex devenv plan sample --json
```

still produce the same ready/blocked counts for existing fixture samples.

## Phase 4 — Catalog CLI

Add commands:

```text
nex devenv catalog list
nex devenv catalog list --json
nex devenv catalog check
nex devenv catalog check --json
```

Initial `check` may be offline-only:

- validates source metadata
- validates mapping entries
- reports stale review date
- reports unmapped vendored top-level YAML schema keys

Later `check --upstream <path>` can compare a local devenv checkout.

## Phase 5 — Upstream update script

Add:

```text
scripts/update-devenv-surface-catalog.sh
```

Behavior:

```text
--rev <sha>       fetch exact upstream rev
--branch main     fetch upstream branch
--check-only      do not write, report drift
--fallback-md     allow options.md fallback when optionsJSON unavailable
```

Generation path:

1. clone/fetch `cachix/devenv`
2. copy `docs/src/devenv.schema.json`
3. in `docs/gen`, run `devenv build outputs.devenv-docs-options-json`
4. read `$result/share/doc/nixos/options.json`
5. normalize to `data/devenv/upstream/options.summary.json`

Fallback path:

- parse `docs/src/reference/options.md` headings
- mark source confidence as `markdown-fallback`

## Phase 6 — Scheduled drift workflow

Add workflow:

```text
.github/workflows/devenv-surface-drift.yml
```

Runs weekly and on manual dispatch.

Output:

- upload drift report artifact
- create/update GitHub issue if unknown surfaces changed

Do not hard-fail unrelated PRs.

## Risks and mitigations

### Risk: devenv optionsJSON generation requires devenv itself

Mitigation:

- vendor snapshots
- support Markdown fallback
- run live generation only in scheduled maintainer workflow

### Risk: Pkl mapping adds runtime dependency

Mitigation:

- parse mapping in build/update tooling and commit generated JSON, or
- use existing Nex Pkl fallback only in maintainer commands

### Risk: false confidence from text scanning devenv.nix

Mitigation:

- report classification as static heuristic
- do not auto-migrate requires-review or machine-scoped items
- eventually add evaluator-backed option extraction

## Non-goals

- Do not execute arbitrary devenv projects during normal `inspect`.
- Do not claim full devenv compatibility.
- Do not migrate services/tasks/shell hooks without explicit review.
