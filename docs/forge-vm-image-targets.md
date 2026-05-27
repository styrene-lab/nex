---
id: forge-vm-image-targets
title: "Add VM/cloud image materialization targets"
status: exploring
parent: forge-materialization-delivery-split
tags: [nex, forge, materialization, vm-image, cloud-image]
open_questions:
  - "Which NixOS build attributes should map to `qcow2`, `raw-image`, and `iso-image` in the supported nixpkgs baseline?"
  - "Should the next release include `export-materialization` to emit an inspectable workspace without building, or is `build-materialization --output` sufficient for the first VM/cloud slice?"
dependencies: []
related: []
---

# Add VM/cloud image materialization targets

## Overview

Extend deterministic materialization targets beyond `toplevel` and `sd-image` to support VM/cloud-friendly local image artifacts that Packer or other external tools can consume without Nex integrating with those tools.

## Decisions

### Do not add Packer integration

**Status:** proposed

**Rationale:** The operator wants Nex profiles as source of truth for repeatable deterministic Nix image builds, not a Packer orchestration layer.

### Build local deterministic artifacts, not cloud resources

**Status:** proposed

**Rationale:** This keeps Nex reproducible and locally inspectable while allowing external tools to publish/import artifacts.

## Open Questions

- Which NixOS build attributes should map to `qcow2`, `raw-image`, and `iso-image` in the supported nixpkgs baseline?
- Should the next release include `export-materialization` to emit an inspectable workspace without building, or is `build-materialization --output` sufficient for the first VM/cloud slice?
