---
id: nex-core-capability-model
title: "Core Nex capability model"
status: decided
tags: [nex, profiles, capabilities, services, validation]
open_questions:
  - "[assumption] TOML profile syntax remains the compatibility surface while Pkl-first profile modeling evolves."
  - "[assumption] Capability compilation can be introduced inside the current profile apply path before a full task DAG exists."
  - "Which first capabilities prove the model with minimal platform-specific risk?"
  - "Should `nex profile explain` ship before `nex profile test`, or should the first slice include both?"
dependencies:
  - nex-profile-explain-test-info
related:
  - nex-capability-service-schema
  - nex-profile-task-dag
  - nex-profile-option-schema-docs
---

# Core Nex capability model

## Overview

Define the portable Nex capability/service model that compiles machine intent into packages, services/config, validation, and explain output without becoming a GUI app ontology.

The central contract is:

```text
profile intent → capability plan → concrete effects → validation evidence
```

A capability is not a GUI app category. A capability is a supported machine affordance whose concrete operating-system effects can be named, applied, explained, and tested.

## Decisions

### Decision: Capability effects must be explicit

**Status:** 

**Rationale:** Status: proposed

A capability is allowed only if its package, service/config, and validation effects can be listed. If the effects cannot be listed, the field belongs in descriptive affordance metadata or documentation, not executable schema.

### Decision: CLI + SSH are the first vertical slice

**Status:** 

**Rationale:** Status: proposed

Start with `cli.editor`, `cli.multiplexer`, and `services.ssh.enable` because they exercise the model without requiring desktop handler backend decisions.

### Decision: Desktop default apps are out-of-scope for the core model

**Status:** 

**Rationale:** Status: proposed

Desktop defaults belong to a later scoped desktop affordance module. The core model must not grow broad GUI app categories.

### Capability effects must be explicit

**Status:** proposed

**Rationale:** A capability belongs in executable schema only when its package, service/config, validation, and explain effects can be listed. Fuzzy categories belong in descriptive affordances or docs.

### CLI plus SSH are the first vertical slice

**Status:** proposed

**Rationale:** These capabilities exercise package, shell environment, service/config, validation, and explain output without forcing desktop default-app backend choices.

### Desktop default apps are out of scope for the core model

**Status:** proposed

**Rationale:** Desktop handlers are useful for GUI workstations but must remain a scoped desktop affordance rather than central Nex capability architecture.

### First slice includes CLI plus SSH

**Status:** accepted

**Rationale:** Operator has no contrary preference; CLI plus SSH is the best judgment path because it proves package, shell, service/config, validation, and explain effects in one coherent vertical slice.

### Explain ships with the first capability slice

**Status:** accepted

**Rationale:** Capability effects must not become hidden magic. Including explain early provides the evidence surface before broader mutation and validation workflows expand.

### Compile capabilities inside current profile apply path initially

**Status:** accepted

**Rationale:** A full task DAG is not required for the first slice. Integrating a bounded CapabilityPlan inside the existing apply path is the simplest reversible implementation route.

## Open Questions

- [assumption] TOML profile syntax remains the compatibility surface while Pkl-first profile modeling evolves.
- [assumption] Capability compilation can be introduced inside the current profile apply path before a full task DAG exists.
- Which first capabilities prove the model with minimal platform-specific risk?
- Should `nex profile explain` ship before `nex profile test`, or should the first slice include both?
