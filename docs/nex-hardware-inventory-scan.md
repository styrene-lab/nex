+++
id = "nex-hardware-inventory-scan"
kind = "design_node"

[data]
title = "Add live hardware inventory scanning and profile matching"
status = "exploring"
issue_type = "architecture"
priority = 2
parent = "hardware-purpose-profile-matrix"
dependencies = []
open_questions = [
  "[assumption] `system_profiler -json` is available on every supported macOS version Nex targets and contains enough hardware and storage detail for v1 inventory.",
  "[assumption] `diskutil info -plist` exposes enough bus/protocol/internal/external evidence to classify Apple internal NVMe vs external USB/Thunderbolt SSD safely.",
  "[assumption] Linux hosts targeted by Nex have `lsblk` available or can accept a degraded scanner when it is missing.",
  "Should `nex hardware scan --json` emit raw evidence hashes plus normalized fields, or embed selected raw command payloads for auditability?",
  "Should disk attestation classification be a separate command (`nex hardware attest --disk`) or part of every scan result?",
  "Should hardware matching live under `nex hardware match` or under `nex machine-profile match`?",
  "What is the minimum schema needed for profile matching without overfitting to disk install safety?"
]
+++

## Overview

Define Nex's live hardware discovery capability: collect host hardware evidence, normalize it into a stable hardware-inventory schema, classify install-relevant devices such as disks, and use the inventory to support machine-profile matching and forge target attestation.

## Research

### Existing Nex capability baseline

Nex currently validates declared machine profiles and artifacts; it does not yet scan live host hardware.

Existing surfaces:

- `nex machine-profile validate <path>` and `nex machine-profile inspect <path> --json` validate/inspect declared `machine-profile.pkl` documents.
- `nex profile-fragment validate/inspect` supports a `hardware` category and safety checks for hardware-driver-mutating fragments.
- `nex artifact check` and `nex artifact check-relationship` validate machine-profile and materialization payload artifacts.
- `nex forge plan/preflight/check` can evaluate forge requests and safety policy.
- v0.23.0 added operator-declared target attestation safety for destructive USB install operations.

Missing surfaces:

- No `nex hardware scan` command.
- No generated `hardware-inventory` document.
- No live host collection from macOS `system_profiler` / `ioreg` / `diskutil` or Linux `/sys` / `lsblk` / `udev` / `dmidecode` / PCI IDs.
- No automatic inventory-to-machine-profile matching.
- No disk classifier that can suggest `external-usb-ssd`, `external-thunderbolt-ssd`, or `internal-apple-nvme` attestations.

### FOSS Rust crate survey — cross-platform base layer

- `sysinfo` — actively used cross-platform crate for CPU, memory, disks/components/networks/processes. Good for baseline OS/CPU/memory visibility, but likely too generic for disk bus/transport and Apple-specific install risk classification.
- `sys-info` — older cross-platform system info crate. Covers basic kernel/CPU/OS information. Lower value than `sysinfo` for a new Nex scanner.
- `systeminfo` — small crate for hardware/OS information; appears shallow and less established.

Decision pressure: use `sysinfo` only as a low-risk baseline supplement, not as the authoritative scanner.

### FOSS Rust crate survey — Linux hardware/disks

- `lsblk` — crate for listing block devices including disks and partitions. Useful if it exposes the fields Nex needs, but the CLI's `lsblk --json --output ...` is also stable and may be more complete.
- `blockdev` — parses and models `lsblk` JSON output into typed block devices. Strong candidate if maintained enough, because it keeps Nex close to Linux userspace truth while avoiding bespoke JSON structs.
- `udev` — safe wrapper around native `libudev`. Good for Linux device properties and events; runtime dependency on libudev is acceptable on many Linux systems but should not become mandatory for macOS builds.
- `udevrs` — pure Rust user-land udev implementation. Interesting for avoiding native libudev, but needs maturity review before depending on it.
- `drives` — Linux mounted/mountable drive enumeration through virtual kernel filesystems. Could help with removable media but likely too narrow.
- `dmidecode` — reads/decodes SMBIOS/DMI tables from `/sys/firmware/dmi/tables` on Linux or memory dumps. Useful for vendor/model/chassis/BIOS/baseboard evidence.
- `pci-ids` — vendors PCI ID database. Useful for mapping PCI vendor/device IDs to human names after reading PCI devices from `/sys/bus/pci` or `lspci`.

Decision pressure: for Linux v1, prefer command-backed collectors (`lsblk --json`, `/sys`, optional `dmidecode`) plus small parsers. Add `udev`/`blockdev` only if they reduce code without introducing portability pain.

### FOSS Rust crate survey — macOS hardware/disks

- `plist` — Serde-capable plist parser. Strong candidate for parsing `system_profiler -xml` output and `diskutil` plist output.
- `IOKit-sys` — raw bindings to Apple's IOKit C APIs. Useful for future direct I/O registry collection but likely too low-level for v1.
- `iokit` — safe Rust bindings for Apple's IOKit user-space APIs with Swift bridge. Interesting but likely higher dependency/complexity risk than shelling out to `system_profiler`/`diskutil` initially.

Decision pressure: macOS v1 should use built-in commands (`system_profiler -json` or `-xml`, `diskutil list -plist`, `diskutil info -plist`, maybe `ioreg`) parsed with `serde_json`/`plist`; direct IOKit can wait.

### Local macOS evidence probe — 2026-05-30

`system_profiler SPHardwareDataType -json` on the current Apple Silicon host emits a compact `SPHardwareDataType` array containing usable hardware overview fields:

- `chip_type`: `Apple M5 Max`
- `machine_model`: `Mac17,7`
- `machine_name`: `MacBook Pro`
- `model_number`
- `number_processors`
- `physical_memory`
- `platform_UUID`
- serial/provisioning identifiers that should be treated as sensitive unless explicitly requested

`diskutil info -plist disk0` emits safety-relevant disk evidence:

- `DeviceNode`: `/dev/disk0`
- `DeviceIdentifier`: `disk0`
- `WholeDisk`: `true`
- `Internal`: `true`
- `SolidState`: `true`
- `BusProtocol`: `Apple Fabric`
- `MediaName`: `APPLE SSD AP4096Z`
- `IORegistryEntryName`: `APPLE SSD AP4096Z Media`
- `DeviceTreePath`: `IODeviceTree:/arm-io@.../AppleANS3CGv2Controller`
- `RemovableMediaOrExternalDevice`: `false`
- `RemovableMedia`: `false`
- `Ejectable`: `false`

This is enough evidence to conservatively classify the current internal disk as internal Apple solid-state storage. For Forge policy, the first implementation should probably generalize the attestation enum beyond `internal-apple-nvme` or treat Apple Silicon `Apple Fabric`/ANS storage as the same destructive-risk class.

Caution: `diskutil list -plist` showed `OSInternal: false` for some physical/APFS entries while `diskutil info -plist disk0` showed `Internal: true`. The scanner should prefer per-disk `diskutil info -plist <whole-disk>` over `diskutil list -plist` for safety-critical internal/external classification.

## Candidate CLI

```text
nex hardware scan --json
nex hardware scan --output hardware-inventory.json
nex hardware attest --disk /dev/disk4 --json
nex hardware match --inventory hardware-inventory.json --profile machine-profile.pkl --json
```

## Candidate schema

```json
{
  "schema": "io.styrene.nex.hardware-inventory.v1",
  "platform": "darwin",
  "arch": "aarch64",
  "vendor": "Apple",
  "model": "Mac17,7",
  "cpu": {
    "summary": "Apple M5 Max"
  },
  "memory": {
    "summary": "128 GB"
  },
  "disks": [
    {
      "id": "disk0",
      "path": "/dev/disk0",
      "whole_disk": true,
      "internal": true,
      "bus": "Apple Fabric",
      "vendor": "Apple",
      "solid_state": true,
      "target_attestation": "internal-apple-storage",
      "destructive_default": "forbidden",
      "evidence": ["diskutil-info-plist"]
    }
  ],
  "network": [],
  "gpus": [],
  "evidence": {
    "commands": ["system_profiler SPHardwareDataType -json", "diskutil list -plist", "diskutil info -plist <disk>"]
  }
}
```

## Decisions

- Proposed: v1 should be evidence-first and command-backed rather than depend directly on low-level platform APIs.
- Proposed: classify disks as a first-class v1 output because it directly closes the loop with Forge target attestation safety.
- Proposed: use `sysinfo` only for generic baseline facts, not for safety-critical disk classification.
- Proposed: prefer per-disk `diskutil info -plist` over `diskutil list -plist` for macOS destructive-operation safety classification.
- Proposed: add a generalized Apple internal storage class or alias because Apple Silicon internal disks report `BusProtocol = Apple Fabric`, not NVMe, despite having equivalent destructive-risk semantics.
