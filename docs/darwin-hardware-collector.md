+++
id = "darwin-hardware-collector"
kind = "design_node"

[data]
title = "Implement Darwin hardware collector"
status = "exploring"
issue_type = "collector"
priority = 2
parent = "nex-hardware-inventory-scan"
dependencies = ["hardware-inventory-schema-v1"]
open_questions = [
  "Which macOS versions must support `system_profiler -json`?",
  "Can `diskutil info -plist` reliably distinguish Thunderbolt external SSD from USB external SSD?",
  "Do we need `ioreg` for external Thunderbolt storage, or is `diskutil` enough?"
]
+++

## Overview

Collect Darwin/macOS host evidence for the hardware inventory scanner.

## Evidence sources

Primary:

- `system_profiler SPHardwareDataType -json`
- `diskutil list -plist`
- `diskutil info -plist <whole-disk>`

Fallback/conditional:

- `system_profiler SPHardwareDataType -xml`
- `system_profiler SPStorageDataType -json`
- `ioreg` for cases where `diskutil` cannot prove bus/transport

## Local probe findings

On current Apple Silicon hardware, `system_profiler SPHardwareDataType -json` exposes model and memory basics but includes sensitive identifiers. The collector should redact serial/provisioning/platform UUID fields unless explicitly requested.

On current Apple Silicon hardware, `diskutil info -plist disk0` exposes safety-relevant fields:

- `Internal = true`
- `SolidState = true`
- `BusProtocol = Apple Fabric`
- `MediaName = APPLE SSD ...`
- `DeviceTreePath` containing `AppleANS...Controller`
- `RemovableMediaOrExternalDevice = false`

`diskutil list -plist` can show misleading `OSInternal` values for APFS/container entries, so safety classification should be based on `diskutil info -plist` for whole disks.

## Decisions

- Proposed: use command execution with strict argument arrays; never shell-interpolate device paths.
- Proposed: parse plist output with the Rust `plist` crate.
- Proposed: only run `ioreg` if evidence is incomplete after `diskutil info -plist`.
