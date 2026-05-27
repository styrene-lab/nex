---
id: artifact-check-boundary-validator
title: "Add artifact check boundary validator"
status: implementing
parent: forge-materialization-delivery-split
tags: [nex, armory, artifact-check, pkl, boundary-validation]
open_questions: []
dependencies: []
related: []
---

# Add artifact check boundary validator

## Overview

Implement `nex artifact check <PATH> [--json]` for Armory-distributed Nex artifacts. Checker evaluates Pkl to raw JSON, inspects semantic-boundary fields before typed deserialization, validates machine-profile/materialization-payload semantics, and optionally checks armory.toml artifact metadata.

## Decisions

### Inspect evaluated Pkl before deserialization

**Status:** decided

**Rationale:** Preserves Pkl's value as structured semantic source and prevents materialization payloads from smuggling machine-profile policy or machine profiles from smuggling concrete Nix material.

### Use generic artifact check command

**Status:** decided

**Rationale:** Armory needs one CI entrypoint for Nex-owned artifact semantics. Nex can dispatch by entrypoint and add future artifact kinds without multiplying top-level commands.

### Validate only boundary fields in armory.toml

**Status:** decided

**Rationale:** Keeps Nex responsible for semantic artifact ownership without becoming Armory's catalog validator.

## Implementation Notes

### File Scope

- `src/cli.rs` — 
- `src/main.rs` — 
- `src/artifact.rs` — 
- `src/ops/artifact.rs` — 
- `src/ops/mod.rs` — 
- `tests/e2e.rs` —
