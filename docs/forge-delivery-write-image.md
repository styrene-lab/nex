---
id: forge-delivery-write-image
title: "Add forge write-image delivery primitive"
status: exploring
parent: forge-materialization-delivery-split
tags: [nex, forge, delivery, usb, sd-card, safety]
open_questions:
  - "Should `write-image` accept only removable disks by default, with an explicit override for non-removable block devices?"
  - "Should artifact decompression be in scope for v1 delivery (`.img.zst`, `.img.gz`) or should v1 require a raw artifact file?"
dependencies: []
related: []
---

# Add forge write-image delivery primitive

## Overview

Add an explicit delivery primitive for writing an already-built image artifact to a USB/SD/block device with destructive-operation confirmation and device validation. This becomes the hardware delivery backend used by interactive forge.

## Decisions

### Create explicit write-image command

**Status:** proposed

**Rationale:** This isolates the destructive hardware operation from deterministic artifact building and gives VM/cloud workflows a natural stopping point at file output.

### Apply delivery safety at write time

**Status:** proposed

**Rationale:** Building an image file is not the risky step; overwriting a removable/block device is.

## Open Questions

- Should `write-image` accept only removable disks by default, with an explicit override for non-removable block devices?
- Should artifact decompression be in scope for v1 delivery (`.img.zst`, `.img.gz`) or should v1 require a raw artifact file?
