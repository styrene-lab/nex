+++
id = "nex-observation-only-uart"
kind = "design_node"

[data]
title = "Add observation-only UART evidence capture"
status = "exploring"
issue_type = "hardware-backend"
priority = 1
parent = "nex-hardware-evidence-acquisition"
dependencies = []
related = ["hardware-inventory-schema-v1", "artifact-evidence-tiers"]
open_questions = [
  "[assumption] Serial capture can initially use the host termios API without requiring a bundled third-party serial program.",
  "[assumption] v1 supports receive-only TTL UART and USB CDC/ACM but no transmit path.",
  "How should Nex distinguish USB-to-TTL adapters from unrelated serial ports without unsafe guessing?",
  "Which baud/framing search strategy is useful without fabricating confidence?",
  "Should raw capture timestamps be stored inline, in a sidecar event stream, or both?"
]
+++

## Overview

Implement the first physical-evidence backend as receive-only UART capture. The initial acceptance case is differentiating a known-good OEM RG35XXSP boot from an experimental open-source boot chain, but the backend and schemas must remain target-neutral.

This slice must not add a generic transmit command, power control, or implicit reset operation.

## Scope

### Included

- enumerate candidate serial adapters with stable identity evidence;
- create or validate an operator-reviewed target connection profile;
- open a serial endpoint receive-only;
- record raw bytes, timestamps, configuration, interruptions, and SHA-256;
- produce a derived text transcript without mutating raw data;
- label sessions as control or experiment;
- compare the earliest divergent marker between two captures;
- emit `io.styrene.nex.hardware-evidence-result.v1`.

### Excluded

- UART transmit;
- bootloader interaction;
- reset or boot-select control;
- target power supply;
- automatic voltage detection unless a supported instrument supplies measured evidence;
- product-specific runtime health checks;
- SPI, I²C, or GPIO drive.

## Safety Contract

The connection profile must state:

```text
target GND → adapter GND
target TX  → adapter RX
adapter TX disconnected
adapter VCC disconnected
```

Capture must refuse to start unless:

- mode is `observe-only`;
- the connection profile is reviewed;
- target logic level is known or explicitly marked as measured by an external instrument;
- adapter receive voltage is compatible;
- TX and VCC are declared disconnected;
- an immutable target/artifact/session identity is available.

The CLI must not infer wiring from adapter enumeration.

## Proposed CLI

```bash
nex hardware adapters list --interface uart --json

nex hardware evidence capture-uart \
  --target anbernic-rg35xxsp-h700-v1 \
  --connection rg35xxsp-console-v1 \
  --adapter usb:VID:PID:SERIAL \
  --baud 115200 \
  --data-bits 8 \
  --parity none \
  --stop-bits 1 \
  --role control \
  --stimulus cold-boot \
  --artifact OEM-CARD-ID

nex hardware evidence compare \
  --control EVIDENCE-ID \
  --experiment EVIDENCE-ID \
  --decoder boot-console-v1 \
  --json
```

Names are proposed; the implementation should fit the existing Nex command registry rather than create a separate executable.

## Raw Capture Semantics

A session should preserve:

- exact bytes in arrival order;
- monotonic timestamps or timestamped chunks;
- wall-clock start/end timestamps;
- serial configuration;
- adapter identity and observed host path;
- read errors and dropped-byte indicators;
- disconnect/reconnect events;
- operator-declared stimulus timing;
- termination reason.

Text rendering is derived. Invalid UTF-8 must not be discarded.

## Decoder Semantics

The first decoder may extract conservative markers such as:

```text
allwinner-spl
u-boot-banner
dram-size
boot-target
kernel-load
linux-version
kernel-panic
nixos-stage-1
nixos-stage-2
login-prompt
```

Markers require matched evidence spans and decoder-version provenance. Absence of a marker is `not-observed`, not proof that the stage did not execute.

## RG35XXSP Acceptance Fixture

Required sessions:

1. `oem-cold-boot`
2. `styrene-cold-boot`
3. `styrene-reset`
4. `styrene-second-cold-boot`

The OEM session is the control. Nex may classify experimental silence as meaningful only if the control capture proves the same adapter, connection profile, and serial configuration can observe a readable stream.

The current physical symptom to investigate is:

- first experimental boot: normal solid green LED with no display progression;
- reset and second cold boot: brief green LED only;
- flashed media boot region read back byte-identically;
- FAT boot partition remained structurally clean;
- no device-written log was found.

These are external observations, not a diagnosis.

## Decisions

### Receive-only first

**Status:** proposed

**Rationale:** It yields the highest-value stage evidence while minimizing electrical and target-state risk.

### Preserve bytes independently from text

**Status:** proposed

**Rationale:** Boot consoles may emit binary prefixes, malformed text, or alternate encodings; evidence must survive decoder changes.

### Require an OEM control capture

**Status:** proposed

**Rationale:** It validates the complete observation path before interpreting experimental silence.

### Keep active UART outside v1

**Status:** proposed

**Rationale:** Transmit enables boot interruption and arbitrary bootloader commands, requiring a separate authorization and recipe model.

## Implementation Plan

1. Add serial-adapter projection to hardware inventory without granting use authority.
2. Add connection-profile and session/result schemas.
3. Implement fixture-driven serialization and safety-policy tests.
4. Implement Darwin and Linux receive-only serial opening.
5. Add raw capture hashing and immutable finalization.
6. Add transcript derivation and conservative boot marker extraction.
7. Add control/experiment comparison.
8. Bind qualifying results to `boots-hardware` evidence evaluation.

## Test Requirements

- Reject unknown/unreviewed connection profiles.
- Reject profiles with TX or VCC connected in `observe-only` mode.
- Reject voltage incompatibility.
- Preserve null bytes and invalid UTF-8.
- Record disconnect and partial-session termination.
- Deterministically regenerate transcript and markers from a fixture capture.
- Never classify silence without a qualifying control session.
- Never emit `boots-hardware` from UART evidence lacking declared required markers.

## Open Questions

- What assumptions is this design making that have not been stated?
- Is host termios sufficient for stable adapter identity, or should enumeration use USB metadata first and map to tty paths second?
- What timestamp resolution is required for boot-stage diagnosis?
- Should a target connection profile permit multiple known board revisions, or require one exact board revision per profile?
