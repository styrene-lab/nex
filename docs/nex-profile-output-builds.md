+++
id = "nex-profile-output-builds"
kind = "design_node"

[data]
title = "Model profile build outputs like devenv outputs"
status = "exploring"
issue_type = "architecture"
priority = 3
parent = "nex-devenv-parallels"
dependencies = []
open_questions = [
  "Which outputs are canonical for v1: module, installer, sd-image, vm, container?",
  "Should outputs be declared by profiles or inferred from target/purpose?",
  "How do output artifacts map to Armory package/artifact metadata?"
]
+++

## Overview

Devenv outputs let one declarative environment produce build artifacts. Nex profiles should similarly define or imply named outputs for machine/application lifecycle artifacts.

## Candidate outputs

```text
module        NixOS/nix-darwin module/config fragment
installer     bootable installer media plan/artifact
sd-image      Raspberry Pi / appliance image
vm            VM image for test or deployment
container     OCI container for process/profile subsets
activation    local apply/switch closure
```

## Candidate commands

```text
nex profile outputs <profile>
nex profile build <profile> --output module
nex profile build <profile> --output installer
nex profile build <profile> --output sd-image
nex profile build <profile> --all
```

## Relationship to existing Nex pieces

- `machine-profile.pkl` owns policy/defaults/safety.
- `payload.pkl` owns materialization source/module content.
- Forge builds installer/media style outputs.
- Armory distributes profile/materialization artifacts.

A named-output model can unify these into a clearer user model.

## Decisions

- Proposed: expose outputs as named artifacts derived from profiles.
- Proposed: output builds should be visible in `profile explain` and validated in `profile test`.
- Proposed: output artifacts should use existing Armory artifact boundary validation.
