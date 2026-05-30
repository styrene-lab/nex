+++
id = "hardware-inventory-schema-v1"
kind = "design_node"

[data]
title = "Define hardware inventory schema v1"
status = "exploring"
issue_type = "schema"
priority = 2
parent = "nex-hardware-inventory-scan"
dependencies = []
open_questions = [
  "Should the schema embed selected raw command payloads, payload hashes, or both?",
  "Should serial numbers and platform UUIDs be redacted by default?",
  "Should target attestation use the Forge enum directly or a broader hardware-risk enum with Forge-specific mapping?"
]
+++

## Overview

Define `io.styrene.nex.hardware-inventory.v1`, the normalized output contract for `nex hardware scan --json`.

## Requirements

- Stable JSON output for automation and fixture tests.
- Platform-neutral top-level shape with platform-specific evidence fields isolated.
- Conservative disk classification that can explain its evidence.
- Redaction policy for serial numbers, UUIDs, and other host identifiers.

## Candidate top-level fields

- `schema`
- `platform`
- `arch`
- `vendor`
- `model`
- `model_name`
- `cpu`
- `memory`
- `disks`
- `network`
- `gpus`
- `evidence`
- `warnings`

## Candidate disk fields

- `id`
- `path`
- `whole_disk`
- `size_bytes`
- `internal`
- `removable`
- `ejectable`
- `solid_state`
- `rotational`
- `bus`
- `transport`
- `vendor`
- `model`
- `media_name`
- `target_attestation`
- `destructive_default`
- `classification_confidence`
- `classification_reasons`
- `evidence_sources`

## Decisions

- Proposed: redact serial numbers and platform UUIDs by default; provide an explicit future `--include-sensitive` flag if needed.
- Proposed: include classifier reasons as data, not only human text.
- Proposed: include `warnings` for degraded collectors and missing commands.
