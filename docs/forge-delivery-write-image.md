---
id: forge-delivery-write-image
title: "Add forge write-image delivery primitive"
status: implemented
parent: forge-materialization-delivery-split
tags: [nex, forge, delivery, usb, sd-card, safety]
open_questions: []
dependencies: []
related: []
---

# Add forge write-image delivery primitive

## Overview

Add an explicit delivery primitive for writing an already-built image artifact to a USB/SD/block device with destructive-operation confirmation and device validation. This becomes the hardware delivery backend used by interactive forge.

## Decisions

### Create explicit flash-image command

**Status:** implemented

**Rationale:** `nex forge flash-image` isolates the destructive hardware operation from deterministic artifact building and gives VM/cloud workflows a natural stopping point at file output. It accepts a raw image or a Nix output containing exactly one `.img`, `.img.zst`, or `.img.gz` artifact.

### Apply delivery safety at write time

**Status:** implemented

**Rationale:** Building an image file is not the risky step; overwriting removable media is. Flashing requires an exact repeated whole-disk attestation, removable/external-media verification, sufficient-capacity validation, and `--yes`. Without `--yes`, the command performs validation only.

### Support streamed compressed images

**Status:** implemented

**Rationale:** NixOS SD-image outputs are commonly compressed. Nex validates and sizes the decompressed stream, writes it directly without a large temporary raw image, flushes the device, then performs SHA-256 verification across the full written range.

### Emit a machine-readable receipt

**Status:** implemented

**Rationale:** Successful delivery prints a JSON receipt and can persist it with `--receipt`. The receipt records the resolved source image, target disk, bytes written, verification mode, and completion time.

## Usage

```text
nex forge build-materialization materialization.pkl \
  --hostname network-core \
  --target sd-image \
  --output ./result-sd-image

nex forge flash-image ./result-sd-image \
  --disk /dev/disk4 \
  --attest-disk /dev/disk4

nex forge flash-image ./result-sd-image \
  --disk /dev/disk4 \
  --attest-disk /dev/disk4 \
  --yes \
  --receipt ./dist/network-core-flash.json
```

The first `flash-image` invocation is a non-destructive preflight. The second performs the write.

## Constraints

- Only removable or externally attached whole disks are accepted.
- Non-removable block-device override is intentionally absent from v1.
- Symlinks encountered while discovering images are ignored.
- Ambiguous build outputs containing multiple image artifacts are rejected.
- Image writes are never performed through a shell command string.
- Successful completion requires a full written-range SHA-256 comparison.
