+++
id = "machine-profile-matching-engine"
kind = "design_node"

[data]
title = "Implement machine profile matching engine"
status = "exploring"
issue_type = "matching"
priority = 3
parent = "nex-hardware-inventory-scan"
dependencies = ["hardware-inventory-schema-v1"]
open_questions = [
  "Where do machine-profile hardware requirements live in the canonical schema?",
  "Should matching target concrete machine profiles, starter matrix rows, or both?",
  "How should purpose (`dev`, `gaming`, `mesh-node`) be supplied when hardware scan cannot infer it?"
]
+++

## Overview

Match a hardware inventory against machine profiles, profile fragments, and starter matrix rows.

## Non-goals for v1

- Do not infer user purpose from hardware alone.
- Do not automatically apply a matched profile.
- Do not hide missing evidence behind a single opaque score.

## Candidate output

```json
{
  "schema": "io.styrene.nex.hardware-match-report.v1",
  "matches": [
    {
      "profile_ref": "starter/arm64-rpi4-edge-node",
      "score": 0.84,
      "confidence": "medium",
      "satisfied": ["arch", "board_family"],
      "unsatisfied": ["purpose not supplied"],
      "warnings": ["GPU evidence not collected"]
    }
  ]
}
```

## Decisions

- Proposed: matching should be explainable first and numerically scored second.
- Proposed: hardware scan identifies hardware class; operator or profile selection supplies purpose.
- Proposed: matrix matching can ship before arbitrary machine-profile matching if the matrix metadata is simpler and better controlled.
