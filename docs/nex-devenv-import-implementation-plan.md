+++
id = "nex-devenv-import-implementation-plan"
kind = "design_node"

[data]
title = "Implementation plan for devenv import support"
status = "exploring"
issue_type = "implementation-plan"
priority = 2
parent = "nex-devenv-import-migration"
dependencies = ["nex-devenv-import-report-schema"]
open_questions = [
  "Which TOML/YAML parser dependencies should Nex add for devenv.yaml and secretspec.toml?",
  "Should static devenv.nix inspection start with regex/heuristics or use a Nix parser crate?",
  "Can we vendor fixtures from devenv examples for tests, or should we author minimal fixtures?"
]
+++

# Implementation plan for devenv import support

## Phase 1 — Static inspect

Command:

```text
nex devenv inspect <path> --json
```

Files:

- `src/devenv_import.rs`
- `src/ops/devenv.rs`
- `src/cli.rs`
- `src/main.rs`

Capabilities:

- discover devenv files
- parse `devenv.yaml` for version/secretspec config if present
- parse `secretspec.toml` contracts without values
- detect local-only files (`devenv.local.*`, `.envrc`)
- detect broad patterns in `devenv.nix`:
  - `packages =`
  - `languages.`
  - `services.`
  - `processes.`
  - `tasks.`
  - `enterShell`
  - `enterTest`
  - `outputs`
  - `containers`

Do not evaluate Nix.

Tests:

- minimal devenv project
- project with SecretSpec
- project with local files
- project with arbitrary enterShell requiring review

## Phase 2 — Human explain

Command:

```text
nex devenv explain <path>
```

Render grouped report:

```text
Portable
Project-scoped
Machine-scoped candidates
Requires review
Unsupported
Secrets
Outputs
```

Human renderer must be over `DevenvImportReport`.

## Phase 3 — Migration plan

Command:

```text
nex devenv migrate-plan <path> --target existing-nixos --json
```

Produces candidate file map, not written files.

Report includes:

- candidate `machine-profile.pkl`
- candidate `payload.pkl`
- fragments to write
- unresolved review items
- warnings

## Phase 4 — Migration generation

Command:

```text
nex devenv migrate <path> --output <dir> --target existing-nixos
```

Writes:

```text
machine-profile.pkl
payload.pkl
fragments/
devenv-import-report.json
README.md
```

Refuse by default when unresolved `requiresReview` items would be dropped or promoted silently.

## Phase 5 — Evaluated enrichment

Research and optionally use:

- `devenv info --json` if stable
- `devenv mcp` option search for option docs, not project evaluation
- Nix evaluation only under explicit flag

## Parser strategy

### devenv.yaml

Use `serde_yaml` or `serde_yml` if already acceptable in dependency policy. Only need shallow fields initially:

```yaml
secretspec:
  enable: true
  provider: keyring
  profile: default
```

### secretspec.toml

Use existing `toml` dependency. Parse project metadata and `[profiles.*]` keys. Do not read provider values.

### devenv.nix

Initial static detection can be conservative text scanning. Full semantic conversion should wait for evaluated metadata or a Nix parser/evaluator decision.

## Acceptance criteria for Phase 1

- `nex devenv inspect fixtures/simple --json` emits valid report.
- SecretSpec contract names are listed without values.
- `enterShell` is classified as `requiresReview`.
- Local files are detected and marked local-only.
- No command execution beyond reading files.

## Decisions

- Proposed: Phase 1 uses conservative static scanning, not Nix evaluation.
- Proposed: add richer parsing only when a concrete migration quality gap appears.
- Proposed: migration generation is blocked until inspect/explain reports are useful enough for UI review.
