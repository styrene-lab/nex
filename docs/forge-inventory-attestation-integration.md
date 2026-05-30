+++
id = "forge-inventory-attestation-integration"
kind = "design_node"

[data]
title = "Integrate hardware inventory with Forge target attestation"
status = "exploring"
issue_type = "integration"
priority = 2
parent = "nex-hardware-inventory-scan"
dependencies = ["disk-attestation-classifier"]
open_questions = [
  "Should Forge accept an inventory path directly, or should operators run `nex hardware attest` separately?",
  "Should request files be mutated with inferred attestation, or should inference remain an ephemeral planning input?",
  "How should conflicts between request attestation and scan evidence be reported?"
]
+++

## Overview

Bridge `nex hardware scan` and the v0.23.0 Forge target-attestation safety policy.

## Candidate flows

```text
nex hardware attest --disk /dev/disk4 --json
nex forge plan --request request.pkl --inventory hardware-inventory.json
nex forge run --request request.pkl --inventory hardware-inventory.json
```

## Safety behavior

- If request attestation conflicts with strong scan evidence, planning blocks with a conflict diagnostic.
- If scan evidence is strong and request attestation is missing, planning may suggest the attestation but should not silently mutate the request.
- If scan evidence is weak/unknown, existing `requires_target_attestation` behavior remains: operator attestation is required.
- Internal Apple storage remains forbidden by default.

## Decisions

- Proposed: inventory is a planning input, not a request mutator.
- Proposed: Forge should report `TARGET_ATTESTATION_CONFLICT` when request attestation contradicts strong hardware evidence.
- Proposed: automatic satisfaction of allowed targets requires strong classifier confidence.
