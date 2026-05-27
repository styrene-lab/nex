---
id: nex-profiles-starter-bases
title: "Add starter base profiles and VM/cloud fragments"
status: exploring
parent: forge-materialization-delivery-split
tags: [nex-profiles, starters, fragments, vm, cloud]
open_questions:
  - "What starter manifest schema should nex-profiles use initially: simple TOML with `fragments = [\\"id@version\\"]`, or reuse/extend machine-profile/materialization-payload documents?"
  - "Should `role/dev` become platform-agnostic, or should it split into `role/linux-dev` and `role/macos-dev` before publishing macOS starter profiles?"
dependencies: []
related: []
---

# Add starter base profiles and VM/cloud fragments

## Overview

Add starter manifests and VM/cloud guest fragments to nex-profiles so users can compose base profiles for CLI Linux, desktop, gaming, server, edge, VM, and cloud image builds.

## Decisions

### Starter profiles are fragment compositions

**Status:** proposed

**Rationale:** This turns cleaned-up free-floating/personal profile knowledge into reusable public entry points without embedding machine-specific state.

### Model guest configuration, not image factory publication

**Status:** proposed

**Rationale:** This supports Packer/AMI/VMDK workflows indirectly by producing suitable deterministic guest images while avoiding Packer/provider coupling.

## Open Questions

- What starter manifest schema should nex-profiles use initially: simple TOML with `fragments = [\"id@version\"]`, or reuse/extend machine-profile/materialization-payload documents?
- Should `role/dev` become platform-agnostic, or should it split into `role/linux-dev` and `role/macos-dev` before publishing macOS starter profiles?
