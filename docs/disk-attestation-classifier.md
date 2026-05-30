+++
id = "disk-attestation-classifier"
kind = "design_node"

[data]
title = "Implement disk attestation classifier"
status = "exploring"
issue_type = "safety"
priority = 1
parent = "nex-hardware-inventory-scan"
dependencies = ["hardware-inventory-schema-v1", "darwin-hardware-collector", "linux-hardware-collector"]
open_questions = [
  "Should Forge rename `internal-apple-nvme` to `internal-apple-storage`, or accept both as aliases?",
  "What confidence threshold is required to auto-satisfy `allowed_targets`?",
  "How should removable SD cards be classified relative to USB SSDs?"
]
+++

## Overview

Convert normalized disk evidence into conservative target-attestation candidates for destructive Forge operations.

## Classification principles

- Safety-critical classifications require strong evidence.
- Ambiguity must produce `unknown`, not a convenient guess.
- Internal Apple storage is forbidden by default regardless of whether the bus reports NVMe, Apple Fabric, or ANS.
- External USB/Thunderbolt SSDs can satisfy Forge allowed-target policy only with strong evidence.

## Candidate classes

- `external-usb-ssd`
- `external-thunderbolt-ssd`
- `internal-apple-nvme` / proposed alias `internal-apple-storage`
- `internal-non-apple-storage`
- `removable-sd-card`
- `unknown`

## Outputs

- `target_attestation`
- `destructive_default`: `allowed-with-attestation`, `forbidden`, or `requires-operator-attestation`
- `confidence`: `strong`, `weak`, `unknown`
- `reasons`: structured strings/enums citing evidence fields

## Decisions

- Proposed: add `internal-apple-storage` as a generalized risk class, then map it to existing Forge `internal-apple-nvme` policy until Forge enum naming is revised.
- Proposed: only `strong` confidence classifications can auto-fill or validate a Forge target attestation.
- Proposed: weak evidence may pre-fill UI text but must still require explicit operator attestation.
