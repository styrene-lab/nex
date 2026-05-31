+++
id = "nex-cli-command-surface-map"
kind = "design_node"

[data]
title = "Maintain CLI command-surface map for UI and internals"
status = "exploring"
issue_type = "architecture"
priority = 1
parent = "nex-devenv-parallels"
dependencies = []
open_questions = [
  "Should command metadata be authored manually first, generated from clap definitions, or both?",
  "What stable report schema should UI clients consume for command capabilities?",
  "Which commands are safe for UI one-click execution versus requiring explicit confirmation?",
  "Should command groups map to future UI navigation sections one-to-one?"
]
+++

# Maintain CLI command-surface map for UI and internals

## Overview

Nex is accumulating several related command families: hardware, secrets, profiles, forge, artifacts, config, packages, identity, and lifecycle operations. We need a living command-surface map that keeps the CLI understandable and gives future UI surfaces a stable model of capabilities, safety posture, inputs, outputs, and relationships.

This document is not just user-facing docs. It is an internal product architecture map: the CLI is the current API boundary, and future UI/MCP/agent surfaces should consume the same conceptual command metadata rather than rediscovering behavior from ad hoc command parsing.

## Design principle

Nex should be a machine/profile lifecycle superset of devenv's environment paradigm. The command map must preserve that identity:

- devenv-like environment concepts live under profile/environment capability groups
- Nex-specific safety, hardware, materialization, and artifact semantics remain first-class
- every command should be classified by safety and mutability
- JSON/report-producing commands should become UI-ready data sources

## Command family map

### Hardware

Purpose: discover and classify machine hardware, especially safety-relevant installation targets.

Current commands:

```text
nex hardware scan [--json] [--output <path>]
nex hardware attest --disk <disk> [--json]
nex hardware match [--inventory <path>] [--purpose <purpose>] [--json]
```

UI role:

- hardware inventory page
- disk safety/attestation panel
- profile recommendation entry point
- preflight evidence source for Forge/profile apply

Safety:

- read-only local inspection
- no mutation
- sensitive fields must remain redacted unless explicitly requested

Stable outputs:

- `io.styrene.nex.hardware-inventory.v1`
- `io.styrene.nex.disk-attestation.v1`
- `io.styrene.nex.hardware-profile-match.v1`

### Profile

Purpose: evaluate, explain, test, build, and apply machine profiles.

Current commands:

```text
nex profile apply <source> [--verify]
nex profile sign <source> [--detached]
nex profile verify <source>
nex machine-profile validate <path>
nex machine-profile inspect <path> [--json]
nex profile-fragment validate <path>
nex profile-fragment inspect <path> [--json]
```

Planned commands:

```text
nex profile info <source> [--json]
nex profile explain <source> [--json]
nex profile test <source> [--json] [--hardware-inventory <path>] [--secrets-provider <provider>]
nex profile outputs <source> [--json]
nex profile build <source> --output <name>
nex profile options search <query>
nex profile options show <path>
```

UI role:

- profile detail page
- explain/plan/test report
- profile wizard
- profile fragment browser
- option search/docs
- apply confirmation flow

Safety:

- validate/inspect/info/explain: read-only
- test/build: build/maybe network; no system mutation by default
- apply: mutating; may require privilege and confirmation
- hardware-driver or destructive actions require explicit safety gates

Stable outputs needed:

- `io.styrene.nex.profile-info.v1`
- `io.styrene.nex.profile-explain.v1`
- `io.styrene.nex.profile-test-report.v1`
- `io.styrene.nex.profile-options.v1`
- `io.styrene.nex.profile-outputs.v1`

### Secrets

Purpose: validate profile secret contracts and support provider-driven runtime injection without leaking values.

Current commands:

```text
# none yet
```

Planned commands:

```text
nex secrets list <profile> [--json]
nex secrets check <profile> [--provider <provider>] [--profile <secrets-profile>] [--json]
nex secrets generate <profile> [--provider <provider>] [--profile <secrets-profile>] [--json]
nex secrets run <profile> [--provider <provider>] -- <command...>
```

UI role:

- missing secret checklist
- provider selection
- generated local secret onboarding
- runtime launch with secrets

Safety:

- list/check: no secret values in output
- generate: mutates secret provider only; never overwrites existing values
- run: passes secret values only to target process; avoid global shell export

Stable outputs needed:

- `io.styrene.nex.secrets-contract.v1`
- `io.styrene.nex.secrets-check-report.v1`
- `io.styrene.nex.secrets-generation-report.v1`

### Forge

Purpose: build installer/materialization plans, validate safety, and run install/artifact workflows.

Current commands:

```text
nex forge plan --request <path> [--inventory <path>]
nex forge preflight --request <path> [--inventory <path>] [--json]
nex forge run --request <path> [--inventory <path>] [--events <human|jsonl>] [--dry-run]
nex forge check <template.pkl> [--json]
nex forge check-materialization --source <path> --hostname <host> [--json]
nex forge build-materialization <path> --hostname <host> --output <dir>
```

UI role:

- plan/preflight page
- destructive action confirmation
- installer artifact builder
- safety diagnostics
- event stream/log viewer

Safety:

- plan/check/preflight: read-only/buildless validation
- build-materialization: writes output directory and may run Nix builds
- run: mutating/destructive depending on request; requires policy and confirmation

Stable outputs:

- `ForgePlan`
- `ForgePreflightReport`
- `ForgeCheckReport`
- `ForgeEvent` JSONL

Important diagnostics:

- `TARGET_ATTESTATION_REQUIRED`
- `TARGET_ATTESTATION_NOT_ALLOWED`
- `TARGET_ATTESTATION_FORBIDDEN`
- `INTERNAL_APPLE_STORAGE_FORBIDDEN`
- `TARGET_ATTESTATION_CONFLICT`
- `DESTRUCTIVE_FLASH_NOT_ALLOWED`

### Artifacts / Armory

Purpose: validate artifact boundaries and relationships for distribution and materialization.

Current commands:

```text
nex artifact check <path> [--evidence <tier>] [--json]
nex artifact check-relationship --profile <path> --payload <path> [--json]
nex lock status
nex lock materialize ...
```

UI role:

- package/artifact validation page
- Armory package details
- relationship graph
- lock status and materialization state

Safety:

- check/status: read-only
- materialize: writes store/lock state

Stable outputs:

- artifact check reports
- relationship reports
- lock status reports

### Config / Identity / RBAC

Purpose: manage local Nex config, identities, signing, and roster access.

Current commands include:

```text
nex config export ...
nex config migrate ...
nex identity ...
nex rbac sync ...
```

UI role:

- settings
- identity and signing status
- RBAC roster management
- migration prompts

Safety:

- config export/status: read-only
- config migrate/update: mutates local config
- identity sign/key operations: secret-bearing; must avoid logging values
- rbac sync: network + config write

### Packages / system operations

Purpose: install/list/remove/update packages and apply system changes.

Current commands include:

```text
nex install <package>
nex remove <package>
nex list
nex update
nex switch
nex rollback
nex doctor
```

UI role:

- package search/install/remove
- doctor/remediation
- switch/apply status
- rollback UI

Safety:

- list/search/doctor without fix: read-only
- install/remove/update/switch/rollback: mutating
- doctor fixes may mutate shell/Homebrew/Nix config; require clear confirmation where destructive

## Command metadata model

Each command should eventually have machine-readable metadata:

```json
{
  "id": "hardware.scan",
  "path": ["hardware", "scan"],
  "summary": "Scan this host and emit a hardware inventory",
  "mutability": "read-only",
  "safety": ["local-inspection"],
  "requiresConfirmation": false,
  "requiresPrivilege": false,
  "network": false,
  "inputs": [
    { "name": "json", "type": "bool", "default": false },
    { "name": "output", "type": "path", "optional": true }
  ],
  "outputs": ["io.styrene.nex.hardware-inventory.v1"],
  "ui": {
    "section": "Hardware",
    "primaryView": "inventory"
  }
}
```

## Safety taxonomy

Use this taxonomy across CLI, UI, and reports:

```text
read-only
local-file-write
network-read
network-write
build
user-config-mutation
system-config-mutation
privileged-mutation
hardware-driver-mutation
destructive-disk-operation
secret-contract
secret-value-runtime
identity-signing
```

## UI architecture implications

Future UI should not shell out blindly and scrape human output. It should prefer stable report commands and event streams:

- use `--json` report commands for snapshots
- use `--events jsonl` for long-running workflows
- use command metadata for buttons/forms/safety prompts
- use shared diagnostics and safety taxonomy for warnings/blockers
- never ask users to manually classify hardware when inventory evidence can do it

## Implementation plan

### Phase 1 — living docs

- Keep this document updated as commands are added/renamed.
- Add stable schema names when report structs are introduced.

### Phase 2 — command metadata registry

- Add a Rust `command_surface` module with static metadata for major commands.
- Add `nex command-surface --json` or `nex internal command-surface --json`.
- Use metadata to validate docs and future UI forms.

### Phase 3 — UI-ready report hardening

- Ensure every UI-relevant command has a JSON report.
- Replace human-only output paths with human renderers over structured reports.
- Add tests for report schema stability.

### Phase 4 — UI/frontend consumption

- UI reads command metadata and report schemas.
- UI uses safety taxonomy for confirmations.
- UI subscribes to JSONL events for long-running operations.

## Decisions

- Proposed: command metadata should be canonical in Rust, with docs generated or checked against it later.
- Proposed: human output should always be a renderer over structured reports, not a separate code path.
- Proposed: UI one-click actions are allowed only for read-only and non-secret-contract checks by default.
- Proposed: mutating/destructive/secret-bearing commands must expose safety metadata before UI integration.
