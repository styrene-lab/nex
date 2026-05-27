---
id: apply-existing-machine-profile-targets
title: "Define apply-existing machine-profile targets"
status: exploring
parent: hardware-purpose-profile-matrix
tags: [nex, machine-profile, target-vocabulary, apply-existing, jamkit]
open_questions: []
dependencies: []
related: []
---

# Define apply-existing machine-profile targets

## Overview

Add Nex machine-profile vocabulary for apply-existing/existing-NixOS profiles so Armory can index profiles like nex-jamkit without inventing target fields.

## Decisions

### Add apply-existing/existing-nixos vocabulary

**Status:** decided

**Rationale:** nex-jamkit and similar workstation-layer profiles need a Nex-owned target that is system-mutating but not disk-provisioning.

### Separate existing-host confirmation from physical attestation

**Status:** decided

**Rationale:** Apply-existing workflows can mutate services/packages but should not inherit disk-provisioning safety requirements.
