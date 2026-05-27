---
id: hardware-purpose-profile-matrix
title: "Hardware + purpose profile matrix"
status: exploring
parent: forge-materialization-delivery-split
tags: [nex-profiles, matrix, hardware, purpose, starters]
open_questions:
  - "Should hardware class names use `amd64` or Nix system names like `x86_64-linux`? Proposed: use matrix-friendly `amd64-*` labels and record Nix system separately."
  - "Should `jamkit` remain a purpose class or be generalized as `low-latency-audio` with `jamkit` as a descriptive profile name?"
  - "Which Styrene mesh/MANET hardware assumptions are stable enough for first-pass public starters: generic Linux mesh node, Raspberry Pi gateway, Reticulum/LXMF node, or radio-attached field node?"
  - "Should Raspberry Pi targets split by board generation immediately (`arm64-rpi4`, `arm64-rpi5`) or start with `arm64-rpi` plus optional board-specific fragments?"
  - "Do we add `hardware` as a supported ProfileFragmentCategory in Nex 0.21.0 before adding `hardware/rpi4`, or do we use `platform/rpi4` temporarily in nex-profiles?"
dependencies: []
related: []
---

# Hardware + purpose profile matrix

## Overview

Define a prescriptive hardware + purpose matrix for Nex starter profiles and profile fragments. The matrix should consolidate personal/free-floating profiles into a repeatable upstream pattern while allowing descriptive repository/profile names to vary.

## Research

### Future matrix rows — workstation and laptop

| Hardware class | Purpose | Starter ID | Nix system | Notes |
|---|---|---|---|---|
| `amd64-generic` | `cli` | `starter/amd64-cli` | `x86_64-linux` | Minimal Linux baseline for unknown PC/VM/server. |
| `amd64-generic` | `server` | `starter/amd64-server` | `x86_64-linux` | Headless generic server. |
| `amd64-generic` | `dev` | `starter/amd64-dev` | `x86_64-linux` | Generic developer machine without desktop/GPU assumption. |
| `amd64-generic` | `vm-base` | `starter/amd64-vm-base` | `x86_64-linux` | Generic VM guest; suitable base for qcow2/raw builds. |
| `amd64-generic` | `cloud-base` | `starter/amd64-cloud-base` | `x86_64-linux` | Generic cloud image guest; no provider publication semantics. |
| `amd64-amd-desktop` | `desktop` | `starter/amd64-amd-desktop` | `x86_64-linux` | AMD GPU graphical workstation. |
| `amd64-amd-desktop` | `gaming` | `starter/amd64-amd-gaming` | `x86_64-linux` | Upstream pattern from `nex-gamingpc`. |
| `amd64-amd-desktop` | `low-latency-audio` | `starter/amd64-amd-low-latency-audio` | `x86_64-linux` | Network music/audio rig pattern from `nex-jamkit`. |
| `amd64-intel-desktop` | `desktop` | `starter/amd64-intel-desktop` | `x86_64-linux` | Intel desktop/NUC graphical workstation. |
| `amd64-intel-desktop` | `server` | `starter/amd64-intel-server` | `x86_64-linux` | NUC/minipc server baseline. |
| `amd64-laptop` | `cli` | `starter/amd64-laptop-cli` | `x86_64-linux` | Laptop baseline without desktop choice. |
| `amd64-laptop` | `desktop` | `starter/amd64-laptop-desktop` | `x86_64-linux` | Generic NixOS laptop desktop. |
| `amd64-laptop` | `dev` | `starter/amd64-laptop-dev` | `x86_64-linux` | Developer laptop with laptop power/network defaults. |
| `amd64-apple-t2` | `desktop` | `starter/amd64-apple-t2-desktop` | `x86_64-linux` | High-risk future Intel/T2 MacBook Linux profile. |
| `amd64-apple-t2` | `dev` | `starter/amd64-apple-t2-dev` | `x86_64-linux` | T2 MacBook developer workstation after hardware support is proven. |

### Future matrix rows — generic ARM and edge

| Hardware class | Purpose | Starter ID | Nix system | Notes |
|---|---|---|---|---|
| `amd64-edge-box` | `edge-node` | `starter/amd64-edge-box-edge-node` | `x86_64-linux` | Rugged/minipc field node. |
| `amd64-edge-box` | `mesh-node` | `starter/amd64-edge-box-mesh-node` | `x86_64-linux` | x86 mesh participant. |
| `amd64-edge-box` | `mesh-gateway` | `starter/amd64-edge-box-mesh-gateway` | `x86_64-linux` | Mesh-to-LAN/WAN bridge on mini PC. |
| `amd64-edge-box` | `router` | `starter/amd64-edge-box-router` | `x86_64-linux` | Routing/firewall appliance. |
| `amd64-edge-box` | `field-diagnostics` | `starter/amd64-edge-box-field-diagnostics` | `x86_64-linux` | Portable diagnostics and recovery kit. |
| `arm64-generic` | `cli` | `starter/arm64-cli` | `aarch64-linux` | Generic ARM Linux baseline. |
| `arm64-generic` | `server` | `starter/arm64-server` | `aarch64-linux` | Generic ARM server baseline. |
| `arm64-generic` | `vm-base` | `starter/arm64-vm-base` | `aarch64-linux` | ARM VM guest baseline. |
| `arm64-generic` | `cloud-base` | `starter/arm64-cloud-base` | `aarch64-linux` | ARM cloud image baseline. |
| `arm64-cloud` | `cloud-server` | `starter/arm64-cloud-server` | `aarch64-linux` | ARM cloud/server image with server role. |
| `arm64-sbc` | `edge-node` | `starter/arm64-sbc-edge-node` | `aarch64-linux` | Generic non-Pi SBC field node. |
| `arm64-sbc` | `sensor-node` | `starter/arm64-sbc-sensor-node` | `aarch64-linux` | Sensor/telemetry SBC. |
| `arm64-sbc` | `mesh-node` | `starter/arm64-sbc-mesh-node` | `aarch64-linux` | Generic SBC mesh participant. |
| `arm64-edge-gateway` | `mesh-gateway` | `starter/arm64-edge-gateway-mesh-gateway` | `aarch64-linux` | Dedicated ARM mesh gateway appliance. |
| `arm64-edge-gateway` | `router` | `starter/arm64-edge-gateway-router` | `aarch64-linux` | ARM routing/firewall gateway. |

### Future matrix rows — Raspberry Pi family

| Hardware class | Purpose | Starter ID | Nix system | Notes |
|---|---|---|---|---|
| `arm64-rpi` | `edge-node` | `starter/arm64-rpi-edge-node` | `aarch64-linux` | Board-family generic Pi edge node if board split is deferred. |
| `arm64-rpi` | `kiosk` | `starter/arm64-rpi-kiosk` | `aarch64-linux` | Board-family generic Pi kiosk if board split is deferred. |
| `arm64-rpi4` | `cli` | `starter/arm64-rpi4-cli` | `aarch64-linux` | Minimal Pi 4 baseline. |
| `arm64-rpi4` | `edge-node` | `starter/arm64-rpi4-edge-node` | `aarch64-linux` | First RPi4 field-node starter. |
| `arm64-rpi4` | `mesh-node` | `starter/arm64-rpi4-mesh-node` | `aarch64-linux` | RPi4 Styrene mesh/MANET participant pattern. |
| `arm64-rpi4` | `mesh-gateway` | `starter/arm64-rpi4-mesh-gateway` | `aarch64-linux` | RPi4 mesh-to-LAN/WAN gateway. |
| `arm64-rpi4` | `sensor-node` | `starter/arm64-rpi4-sensor-node` | `aarch64-linux` | Pi 4 telemetry/sensor node. |
| `arm64-rpi4` | `kiosk` | `starter/arm64-rpi4-kiosk` | `aarch64-linux` | Pi 4 display/appliance image; aligns with current RPi image work. |
| `arm64-rpi4` | `field-diagnostics` | `starter/arm64-rpi4-field-diagnostics` | `aarch64-linux` | Portable Pi diagnostics kit. |
| `arm64-rpi4` | `airgap-handoff` | `starter/arm64-rpi4-airgap-handoff` | `aarch64-linux` | Offline/portable transfer or install-prep node. |
| `arm64-rpi5` | `cli` | `starter/arm64-rpi5-cli` | `aarch64-linux` | Minimal Pi 5 baseline. |
| `arm64-rpi5` | `edge-node` | `starter/arm64-rpi5-edge-node` | `aarch64-linux` | Pi 5 field-node starter. |
| `arm64-rpi5` | `mesh-node` | `starter/arm64-rpi5-mesh-node` | `aarch64-linux` | Pi 5 mesh participant. |
| `arm64-rpi5` | `mesh-gateway` | `starter/arm64-rpi5-mesh-gateway` | `aarch64-linux` | Pi 5 mesh gateway. |
| `arm64-rpi5` | `kiosk` | `starter/arm64-rpi5-kiosk` | `aarch64-linux` | Pi 5 kiosk/appliance. |

### Future matrix rows — macOS and special-purpose

| Hardware class | Purpose | Starter ID | Nix system | Notes |
|---|---|---|---|---|
| `macos-arm64` | `cli` | `starter/macos-arm64-cli` | `aarch64-darwin` | Apple Silicon CLI baseline. |
| `macos-arm64` | `dev` | `starter/macos-arm64-dev` | `aarch64-darwin` | Apple Silicon developer workstation. |
| `macos-arm64` | `desktop` | `starter/macos-arm64-desktop` | `aarch64-darwin` | macOS defaults and desktop/userland tooling. |
| `macos-amd64` | `cli` | `starter/macos-amd64-cli` | `x86_64-darwin` | Intel macOS CLI baseline. |
| `macos-amd64` | `dev` | `starter/macos-amd64-dev` | `x86_64-darwin` | Intel macOS developer workstation. |
| `macos-amd64` | `desktop` | `starter/macos-amd64-desktop` | `x86_64-darwin` | Intel macOS desktop/userland tooling. |
| `amd64-generic` | `airgap-handoff` | `starter/amd64-airgap-handoff` | `x86_64-linux` | Portable/offline handoff image. |
| `arm64-generic` | `airgap-handoff` | `starter/arm64-airgap-handoff` | `aarch64-linux` | ARM offline handoff image. |
| `amd64-generic` | `field-diagnostics` | `starter/amd64-field-diagnostics` | `x86_64-linux` | Generic diagnostic/recovery image. |
| `arm64-generic` | `field-diagnostics` | `starter/arm64-field-diagnostics` | `aarch64-linux` | ARM diagnostic/recovery image. |

## Decisions

### Make the matrix prescriptive and names descriptive

**Status:** proposed

**Rationale:** The operator wants naming conventions to describe entries, but the reusable structure should come from a hardware + purpose matrix. This prevents personal repo names like nex-gamingpc or nex-jamkit from becoming the taxonomy.

### Define initial hardware-class axis

**Status:** proposed

**Rationale:** This covers current personal profiles (`nex-gamingpc`, `nex-jamkit`) as amd64/AMD desktop patterns, separates macOS, and leaves room for Raspberry Pi and T2 MacBook targets without overfitting to one device.

### Define initial purpose axis

**Status:** proposed

**Rationale:** This separates `gamingpc` and `jamkit` as purposes that can share the same hardware class, and it provides public reusable starters for common workloads.

### Treat edge and mesh devices as first-class matrix targets

**Status:** proposed

**Rationale:** Styrene mesh/MANET devices and recent Raspberry Pi 4 work require repeatable upstream patterns for constrained, networked, and field-deployed nodes.

### Add explicit ARM hardware classes

**Status:** proposed

**Rationale:** The operator wants ARM included and specifically called out for Raspberry Pi and edge-device work. `aarch64-linux` alone is too coarse for hardware behavior.

### Add edge and device purpose classes

**Status:** proposed

**Rationale:** This captures Styrene mesh/MANET node patterns, Raspberry Pi appliance/kiosk patterns, and diagnostic field devices without conflating them with generic servers.

### Implement matrix in nex-profiles first

**Status:** proposed

**Rationale:** The matrix is prescriptive catalog structure. Nex CLI support can follow once the data shape proves useful.

### Use simple starter manifest schema v1

**Status:** proposed

**Rationale:** A standalone starter schema avoids overloading machine profiles or materialization payloads. It lets Armory/Nex index composition intent without implying materialization semantics.

### Select first starter rows

**Status:** proposed

**Rationale:** These cover current cleaned-up profiles, deterministic image-building direction, and near-term RPi/mesh work without implementing the entire future matrix.

### Add first supporting fragments

**Status:** proposed

**Rationale:** Starters should compose versioned fragments rather than embedding the whole profile logic. RPi support needs a hardware-specific fragment boundary.

## Open Questions

- Should hardware class names use `amd64` or Nix system names like `x86_64-linux`? Proposed: use matrix-friendly `amd64-*` labels and record Nix system separately.
- Should `jamkit` remain a purpose class or be generalized as `low-latency-audio` with `jamkit` as a descriptive profile name?
- Which Styrene mesh/MANET hardware assumptions are stable enough for first-pass public starters: generic Linux mesh node, Raspberry Pi gateway, Reticulum/LXMF node, or radio-attached field node?
- Should Raspberry Pi targets split by board generation immediately (`arm64-rpi4`, `arm64-rpi5`) or start with `arm64-rpi` plus optional board-specific fragments?
- Do we add `hardware` as a supported ProfileFragmentCategory in Nex 0.21.0 before adding `hardware/rpi4`, or do we use `platform/rpi4` temporarily in nex-profiles?
