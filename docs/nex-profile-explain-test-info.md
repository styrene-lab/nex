+++
id = "nex-profile-explain-test-info"
kind = "design_node"

[data]
title = "Add profile info, explain, and test workflows"
status = "exploring"
issue_type = "feature"
priority = 1
parent = "nex-devenv-parallels"
dependencies = ["nex-secretspec-integration"]
open_questions = [
  "Should `profile explain` accept incomplete profiles and explain missing fields, or require full validation first?",
  "Should `profile test` run hardware scan by default or require an explicit `--hardware-inventory` input in CI?",
  "What output format should be stable first: JSON report or human explanation?"
]
+++

## Overview

Borrow `devenv info` and `devenv test` as Nex profile workflows that reduce operator cognitive load.

The goal is to make the profile layer explain itself:

- what it will do
- what it assumes
- what secrets it needs
- what hardware it expects
- what outputs it can build
- what safety gates block apply/install

## Candidate commands

```text
nex profile info <source>
nex profile explain <source>
nex profile test <source>
nex profile test <source> --hardware-inventory inventory.json
nex profile test <source> --secrets-provider keyring
```

## `profile info`

A compact summary:

- profile ID/name/version
- target kind (`existing-nixos`, `nix-darwin`, installer media, etc.)
- purpose
- imports/fragments
- packages/services summary
- outputs
- required secrets count
- safety posture

## `profile explain`

A narrative/structured explanation:

- imported fragments and what each contributes
- resolved package/service deltas
- hardware assumptions and match status
- destructive operations and confirmations required
- secret requirements without values
- generated outputs and artifact relationships
- warnings/blockers with remediation

## `profile test`

A lifecycle gate. Proposed DAG:

```text
profile:evaluate
profile:validate-schema
profile:validate-imports
hardware:scan-or-load
hardware:match
secrets:check
forge:plan
forge:preflight
materialization:dry-build-or-check
```

The first implementation can skip expensive build steps by default and expose `--full` later.

## Report schema

```json
{
  "schema": "io.styrene.nex.profile-test-report.v1",
  "profile": { "id": "gaming-workstation" },
  "valid": false,
  "summary": [],
  "checks": [
    { "id": "profile:evaluate", "status": "passed" },
    { "id": "secrets:check", "status": "blocked", "missing": ["CACHE_TOKEN"] }
  ],
  "warnings": [],
  "blockers": []
}
```

## Decisions

- Proposed: implement JSON report first, then human rendering from the same report.
- Proposed: treat missing secrets and hardware mismatch as blockers for `apply`, but as explainable findings for `explain`.
- Proposed: use the same check/result vocabulary across profile test, forge preflight, and artifact validation.
