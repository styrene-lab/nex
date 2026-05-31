+++
id = "nex-devenv-import-migration"
kind = "design_node"

[data]
title = "Import and migrate devenv projects into Nex profiles"
status = "exploring"
issue_type = "architecture"
priority = 2
parent = "nex-devenv-parallels"
dependencies = ["nex-profile-explain-test-info", "nex-profile-task-dag", "nex-secretspec-integration", "nex-devenv-container-strategy"]
open_questions = [
  "Does `devenv info` expose stable JSON sufficient for import, or do we need a custom evaluator adapter?",
  "Should migration generation require `devenv` to be installed, or should static partial import always work?",
  "How should arbitrary Nix imports/overlays be preserved without making generated Nex artifacts unsafe?",
  "Can Nex consume SecretSpec contracts without resolving secret values?",
  "What is the UI flow for reviewing requires-review migrated items?"
]
+++

# Import and migrate devenv projects into Nex profiles

## Overview

Nex should consume `devenv` projects for cross-compatibility and migration by treating devenv as an import/source format, not as Nex's internal model.

The importer should ingest a devenv project, explain what it contains, classify each feature by portability and safety, and optionally generate Nex profile artifacts plus a migration report.

```text
devenv.nix / devenv.yaml / devenv.lock / secretspec.toml
        ↓
Nex devenv importer
        ↓
io.styrene.nex.devenv-import-report.v1
        ↓
Nex profile candidate + fragments + payload + review notes
```

## Product stance

Devenv solves:

```text
project → developer environment → shell/processes/services/tasks/outputs
```

Nex solves:

```text
machine/profile → hardware + secrets + safety + materialization + apply/install + artifacts
```

Therefore, Nex can import devenv but must reclassify every devenv feature through Nex's machine/profile/safety semantics before anything is applied or materialized.

## Candidate commands

```text
nex devenv inspect <path> [--json]
nex devenv explain <path> [--json]
nex devenv migrate-plan <path> [--target <target>] [--json]
nex devenv migrate <path> --output <dir> [--target <target>]
```

Optional later:

```text
nex devenv containerize <path> --output <dir>
nex devenv compare <path> --profile <nex-profile>
```

## File discovery

A devenv project may contain:

```text
devenv.nix
devenv.yaml
devenv.lock
devenv.local.nix
devenv.local.yaml
.envrc
secretspec.toml
```

Nex should discover all of these and classify local-only files separately from committed/shared files.

## Import modes

### Static mode

No external `devenv` command required. Detect files, parse `devenv.yaml`, parse `secretspec.toml`, and perform conservative text/Nix-shape inspection where possible.

Benefits:

- works everywhere
- safe and fast
- no Nix evaluation surprises

Limitations:

- cannot reliably know computed package lists, module defaults, imports, or enabled services when arbitrary Nix is involved

### Evaluated mode

If `devenv` is available, run a structured/evaluated command such as `devenv info --json` if supported, or another stable machine-readable interface.

Benefits:

- reflects actual devenv semantics
- can capture resolved packages/services/processes/tasks

Risks:

- evaluates arbitrary project Nix
- may require network/Nix cache
- output stability must be verified
- may execute hooks if wrong command is used; importer must avoid shell activation commands during inspection

### Hybrid mode

Default to static discovery, enrich with evaluated metadata when available and explicitly safe.

Recommended first implementation:

```text
static discovery always
optional evaluated metadata with --evaluate or auto if safe command exists
```

## Import report schema

```json
{
  "schema": "io.styrene.nex.devenv-import-report.v1",
  "root": "/path/to/project",
  "detected": {
    "devenvNix": true,
    "devenvYaml": true,
    "devenvLock": true,
    "devenvLocal": false,
    "envrc": true,
    "secretspecToml": true
  },
  "mode": "static",
  "portable": [],
  "projectScoped": [],
  "machineScopedCandidates": [],
  "requiresReview": [],
  "unsupported": [],
  "secrets": [],
  "outputs": [],
  "warnings": []
}
```

## Classification buckets

Each discovered item should land in exactly one bucket:

- `portable`: can map cleanly into Nex profile/artifact model
- `projectScoped`: valid but should remain project/devshell/container scoped by default
- `machineScopedCandidate`: could become a NixOS/nix-darwin machine service/package/setting with explicit confirmation
- `requiresReview`: arbitrary command/Nix/hook/side effect that needs human review
- `unsupported`: cannot be represented safely

## Mapping table

| devenv concept | Nex mapping | Default bucket | Notes |
|---|---|---|---|
| `packages` | profile packages / payload package set | portable | Preserve package refs and pins when possible |
| `languages.*` | dev profile fragments | portable | Preserve toolchain versions; warn when implicit defaults are used |
| `services.*` | service fragments, process stack, or container output | machineScopedCandidate | Need target decision |
| `processes` | profile task/process graph | projectScoped | Not promoted to system service automatically |
| `tasks` | profile task DAG | portable | Arbitrary exec may require review |
| `enterTest` | profile test task | portable | Good fit for validation-container |
| `enterShell` | shell hook | requiresReview | Often side-effectful or project-local |
| `outputs` | profile outputs | portable | Need output type classification |
| `containers` | container outputs | portable | Process/service scoped only |
| `secretspec` | Nex secret contracts | portable | Values never imported |
| dotenv integration | secret provider hint | projectScoped | Avoid embedding values |
| git hooks | project hygiene task | projectScoped | Not machine policy by default |
| overlays/overrides | payload source refs | requiresReview | Arbitrary Nix; preserve not execute blindly |
| imports | Nex imports/source refs | requiresReview | Depends on imported content |
| binary caching | cache config/secret contract | machineScopedCandidate | Secret-bearing if push token involved |

## Migration output

`nex devenv migrate` should generate:

```text
nex-profile/
  machine-profile.pkl
  payload.pkl
  fragments/
  devenv-import-report.json
  README.md
```

Generated artifacts must include review comments for non-portable items. The migration report remains part of the artifact boundary so UI can show review state.

## Safety requirements

- Never execute `enterShell`, tasks, processes, or hooks during inspect/explain/migrate-plan.
- Never import secret values from `.env` or providers.
- Never promote project processes to machine services without explicit target decision.
- Mark arbitrary shell commands as `requiresReview` even when preserved as tasks.
- Preserve provenance: every generated Nex item should know which devenv source/path produced it.

## Second-order effects

### Scope drift

If Nex imports devenv too seamlessly, users may expect Nex to be a project-shell manager. The UI and docs must frame imported devenv content as project-environment material within a broader machine/profile lifecycle.

Mitigation: imported reports should label items as `projectScoped`, `machineScopedCandidate`, etc.

### Unsafe promotion

A project-local Postgres service in devenv is not automatically a system Postgres service. Promotion changes lifecycle, persistence, network exposure, security posture, and backup expectations.

Mitigation: services default to project/container scoped unless target profile explicitly promotes them.

### Secret leakage

`.env` and SecretSpec providers can tempt migration tooling to copy values. That would leak into generated artifacts or git.

Mitigation: import contracts only. Values are provider/runtime concern.

### Arbitrary Nix evaluation

Evaluating devenv imports may fetch network resources, run impure logic, or depend on local state.

Mitigation: static inspect by default; evaluated inspect opt-in or only via known-safe `devenv info` path after research.

### Version pin divergence

A migrated Nex profile can drift from `devenv.lock`, causing different package/service versions.

Mitigation: migration report includes source lock metadata and warns when pins cannot be preserved.

### UI false confidence

A polished migration UI can hide `requiresReview` risks.

Mitigation: UI must preserve bucket states and require acknowledgement before generating/applying machine-scoped outputs.

## Implementation phases

### Phase 1 — Inspect report

Implement:

```text
nex devenv inspect <path> --json
```

Scope:

- discover files
- parse `devenv.yaml`
- parse `secretspec.toml` enough to list secret contracts
- classify presence of `devenv.nix`, local files, `.envrc`
- no Nix evaluation

### Phase 2 — Explain renderer

Implement:

```text
nex devenv explain <path>
```

Human renderer over import report.

### Phase 3 — Evaluated enrichment research/adapter

Test `devenv info`/MCP/options surfaces. Add optional evaluated metadata if stable.

### Phase 4 — Migration plan

Implement:

```text
nex devenv migrate-plan <path> --target existing-nixos --json
```

No files written; produces candidate Nex artifact map.

### Phase 5 — Artifact generation

Implement:

```text
nex devenv migrate <path> --output <dir>
```

Write candidate profile artifacts plus report and README.

## Decisions

- Proposed: start with static inspect/report, not full migration.
- Proposed: every imported item receives bucket + safety classification + provenance.
- Proposed: SecretSpec import is contract-only; never values.
- Proposed: process/service promotion requires explicit operator target decision.
- Proposed: generated Nex artifacts keep the migration report as first-class review evidence.
