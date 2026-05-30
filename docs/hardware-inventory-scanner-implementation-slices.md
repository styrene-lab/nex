+++
id = "hardware-inventory-scanner-implementation-slices"
kind = "design_node"

[data]
title = "Hardware inventory scanner implementation slices"
status = "exploring"
issue_type = "implementation-plan"
priority = 2
parent = "nex-hardware-inventory-scan"
dependencies = []
open_questions = [
  "[assumption] We can avoid privileged commands for v1 inventory on macOS and Linux, except where the platform naturally hides optional details.",
  "[assumption] `lsblk --json` output is stable enough across supported Linux distributions for v1 parsing.",
  "[assumption] `diskutil info -plist` provides enough signal for Thunderbolt-vs-USB classification on macOS.",
  "Should v1 include GPU/NIC details, or defer them until the matching engine needs them?",
  "Should hardware inventory be stored under `.nex/hardware/` by default or only emitted on request?"
]
+++

## Overview

Break the hardware scanner into reversible implementation slices that can ship independently while keeping safety-critical disk classification isolated and testable.

## Proposed slices

### 1. Schema and static parser tests

Add Rust data structures for `io.styrene.nex.hardware-inventory.v1` without collecting live data yet.

Scope:

- `src/hardware_inventory.rs`
- CLI enum wiring for `nex hardware scan --json` returning a degraded/stub report only if necessary.
- Fixture-driven tests for Darwin/Linux inventory JSON serialization.

Acceptance:

- Stable JSON schema shape exists.
- Unknown fields are either rejected or ignored by explicit decision.
- `cargo test hardware_inventory` passes.

### 2. macOS collector

Collect enough macOS evidence to identify Apple model, CPU/memory basics, and disks.

Candidate evidence commands:

- `system_profiler SPHardwareDataType -json`
- `system_profiler SPHardwareDataType -xml` as fallback only if needed
- `diskutil list -plist`
- `diskutil info -plist <whole-disk>`
- `ioreg` only if `diskutil`/`system_profiler` cannot classify the bus/transport safely

Preferred crates:

- `plist` for `diskutil` plist outputs and XML system profiler fallback
- `serde_json` for JSON system profiler output

Acceptance:

- Detects platform, arch, Apple model identifier, CPU/memory summary.
- Emits disks with path/name, internal/external signal, bus/protocol where available.
- Classifies internal Apple storage as destructive default `forbidden`.
- Classifies external USB/Thunderbolt SSD only when evidence supports it.
- Prefers per-disk `diskutil info -plist` over aggregate `diskutil list -plist` when fields disagree.

### 3. Linux collector

Collect Linux hardware and block device evidence.

Candidate evidence commands/files:

- `lsblk --json --bytes --output NAME,KNAME,PATH,TYPE,SIZE,MODEL,SERIAL,VENDOR,TRAN,ROTA,RM,HOTPLUG,MOUNTPOINTS,FSTYPE,PKNAME`
- `/sys/class/block/*`
- `/sys/block/*/queue/rotational`
- `/sys/class/dmi/id/*`
- `/sys/firmware/dmi/tables` through `dmidecode` crate if `/sys/class/dmi/id` is insufficient
- `/sys/bus/pci/devices` plus optional `pci-ids` for GPU/NIC names
- optional `udev` for richer properties if command/sysfs data is insufficient

Preferred approach:

- Start with `lsblk --json` plus `/sys/class/dmi/id` because both are common and easy to fixture-test.
- Add `dmidecode`, `udev`, or `pci-ids` only when a matching requirement proves they are needed.

Acceptance:

- Detects platform, arch, vendor/model when DMI is readable.
- Emits disks with path, transport, rotational/removable/hotplug signals.
- Classifies external USB disks as `external-usb-ssd` only when transport/hotplug/removable/rotational evidence supports it.
- Does not guess safety-critical attestation when evidence is incomplete; emits `unknown`/`requires-operator-attestation`.

### 4. Disk attestation classifier

Create a small classifier that converts normalized disk evidence into conservative target-attestation candidates.

Inputs:

- platform
- disk path/device identifier
- whole-disk flag
- internal/external/removable/ejectable flags
- bus/protocol/transport
- solid-state/rotational flag
- vendor/model/media name
- evidence source list

Outputs:

- attestation candidate
- destructive default
- confidence: `strong`, `weak`, or `unknown`
- reasons

Acceptance:

- Internal Apple NVMe / Apple Fabric / ANS storage maps to a forbidden internal Apple storage risk class.
- External USB SSD maps to `external-usb-ssd` only with strong evidence.
- External Thunderbolt SSD maps to `external-thunderbolt-ssd` only with strong evidence.
- Ambiguous disks never auto-satisfy Forge allowed-target policy.

### 5. Matching engine

Score an inventory against machine profiles and starter matrix rows.

Inputs:

- `hardware_inventory.v1`
- machine-profile metadata constraints
- hardware + purpose profile matrix

Outputs:

- match score
- satisfied constraints
- unsatisfied constraints
- warnings for missing evidence
- recommended profile fragments/starters

Acceptance:

- Matching is explainable; no opaque score without reasons.
- Missing evidence degrades confidence instead of pretending certainty.

### 6. Forge attestation integration

Use scan results to populate or validate Forge `target_attestation`.

Candidate flows:

```text
nex hardware attest --disk /dev/disk4 --json
nex forge plan --request request.pkl --inventory hardware-inventory.json
nex forge run --request request.pkl --inventory hardware-inventory.json
```

Acceptance:

- Internal Apple storage remains forbidden by default.
- External USB/Thunderbolt SSD can satisfy allowed target policy when evidence is strong.
- Ambiguous disks require explicit operator attestation rather than automatic approval.

## Decisions

- Proposed: ship `scan` before `match`.
- Proposed: ship disk classification before GPU/NIC matching because it directly supports destructive-operation safety.
- Proposed: represent insufficient evidence explicitly instead of producing a best-guess attestation.
- Proposed: treat scanner evidence collection and target-attestation classification as separate internal modules so classifier tests can use fixtures without running platform commands.
