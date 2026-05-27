---
id: forge-materialization-delivery-split
title: "Split forge materialization from delivery"
status: exploring
tags: [nex, forge, materialization, delivery, release-target]
open_questions:
  - "Which version should carry this release target: 0.21.0 as a forward-compatible minor, or a larger schema-changing release if machine-profile target vocabulary changes are required?"
  - "Should the first implementation add explicit machine-profile fields for allowed_build_targets/allowed_delivery_targets, or document the split while preserving the current v1 `allowed_targets` field?"
  - "Which artifact build targets are in-scope for the next release beyond existing `toplevel` and `sd-image`: `iso-image`, `raw-image`, `qcow2`, or only one initial VM-friendly target?"
dependencies: []
related: []
---

# Split forge materialization from delivery

## Overview

Plan the next Nex release target around separating deterministic artifact build/check semantics from side-effectful delivery to USB/SD/block devices or external consumers. Nex forge should keep the interactive UX while internally composing materialization and delivery phases.

## Research

### Current release context

Nex v0.20.0 is released. Current forge/materialization commands already include deterministic `check-materialization` and `build-materialization` for `toplevel` and `sd-image`. Existing interactive forge historically handles hardware-oriented workflows such as ISO/IMG to USB/SD-card, where build and write/delivery concerns are coupled. The next release target should preserve existing UX while making the internal boundary explicit.

## Decisions

### Separate build artifacts from side-effect delivery

**Status:** proposed

**Rationale:** This keeps Nex as the source of truth for deterministic Nix image builds while avoiding Packer/cloud-provider coupling and isolating destructive hardware operations.

### Use separate materialization-target and delivery-target vocabularies

**Status:** proposed

**Rationale:** A single `target` field conflates artifact format with operational risk. The same artifact can be safely built to a file or destructively written to hardware.

### Preserve interactive forge UX as orchestration

**Status:** proposed

**Rationale:** Existing users keep the simple hardware workflow while API/CLI primitives become reusable for VM/cloud/file-only builds.

## Open Questions

- Which version should carry this release target: 0.21.0 as a forward-compatible minor, or a larger schema-changing release if machine-profile target vocabulary changes are required?
- Should the first implementation add explicit machine-profile fields for allowed_build_targets/allowed_delivery_targets, or document the split while preserving the current v1 `allowed_targets` field?
- Which artifact build targets are in-scope for the next release beyond existing `toplevel` and `sd-image`: `iso-image`, `raw-image`, `qcow2`, or only one initial VM-friendly target?
