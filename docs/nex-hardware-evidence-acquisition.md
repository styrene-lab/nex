+++
id = "nex-hardware-evidence-acquisition"
kind = "design_node"

[data]
title = "Acquire reproducible physical hardware evidence"
status = "exploring"
issue_type = "hardware-evidence"
priority = 1
dependencies = ["nex-hardware-inventory-scan", "forge-materialization-delivery-split"]
related = ["artifact-evidence-tiers", "forge-delivery-write-image", "forge-inventory-attestation-integration"]
open_questions = [
  "[assumption] Nex should persist evidence bundles under a project-local .nex/evidence directory by default.",
  "[assumption] A first release may invoke external acquisition tools while Nex owns manifests, safety policy, and evidence lifecycle.",
  "Which adapter and probe capabilities must be represented in v1 without coupling the schema to vendor-specific devices?",
  "Which evidence fields are sensitive enough to require default redaction or encryption?",
  "Should evidence IDs be content-derived, UUID-based, or both?"
]
+++

## Overview

Nex needs a first-class, reproducible way to acquire and retain evidence from physical targets after materialization and delivery. This closes the gap between a built or flashed image and claims such as `boots-hardware` and `operational`.

Nex owns this layer because it understands adapters, electrical interfaces, physical target profiles, destructive operations, and deployment lifecycle. Product repositories may declare acceptance predicates and consume a bounded result projection, but they must not own adapter drivers, wiring profiles, capture orchestration, or hardware safety policy.

## Ownership Boundary

Nex owns:

- adapter and instrument inventory;
- target hardware profiles and board revisions;
- electrical and wiring profiles;
- UART, SPI, I²C, GPIO, SWD/JTAG, logic-analyzer, and oscilloscope acquisition backends;
- capture-session lifecycle and immutable raw evidence;
- safety authorization for observation, control, power, and destructive operations;
- correlation of evidence with materialization and delivery artifacts;
- attested promotion to `boots-hardware` and `operational` evidence tiers.

A product repository owns:

- product/runtime acceptance predicates;
- protocol- or product-specific health semantics;
- declarations of required evidence markers;
- interpretation of the bounded result projection in release policy.

The interface is directional:

```text
product acceptance declaration
            ↓
Nex acquisition/validation plan
            ↓
physical adapter + target session
            ↓
immutable evidence bundle
            ↓
Nex evidence result projection
            ↓
product acceptance evaluation
```

## Common Model

### Adapter

Stable adapter identity and capabilities, separate from an ephemeral host path such as `/dev/cu.usbserial-*` or `/dev/ttyUSB0`.

Required concepts:

- backend and driver;
- VID/PID and serial identity when available;
- supported interface families;
- voltage and direction capabilities;
- whether the adapter can source power or drive outputs;
- sample/baud limits;
- current host attachment path as observed evidence.

### Target connection

A versioned physical connection profile binds a target hardware revision to named signals and safety constraints.

Required concepts:

- target profile and revision constraints;
- connector/pad references and photographs;
- signal map and direction;
- measured or declared electrical levels;
- common-ground requirement;
- level shifting and current limiting;
- safe disconnected/default state;
- operator-reviewed status.

### Capture session

A bounded operation records:

- immutable session ID and timestamps;
- adapter, target, and connection profile references;
- safety mode;
- materialization artifact and delivery event under test;
- stimulus such as cold boot or reset;
- raw capture paths and hashes;
- tool/backend versions;
- dropped samples, interruption, and termination reason;
- control or experiment role.

### Interpretation

Derived outputs must reference immutable raw hashes and decoder versions. A transcript or decoded transaction stream may be regenerated; raw evidence is never overwritten by a cleaned interpretation.

## Safety Modes

Nex must model authority explicitly:

| Mode | Permitted behavior |
|---|---|
| `observe-only` | Inputs/high-impedance capture only; no target power, transmit, reset, or bus drive. |
| `passive-bus` | Passive multi-channel observation of an already-driven bus. |
| `interactive` | Reviewed bounded transmit/control operations according to the connection profile. |
| `power-capable` | Rail supply or modification; independent authorization from ordinary interaction. |

`observe-only` is the default. Adapter detection never grants authority to drive a target.

## Evidence Bundle v1

Proposed project-local representation:

```text
.nex/evidence/<target>/<session-id>/
├── session.toml
├── adapter.toml
├── connection.toml
├── raw/
│   ├── capture.bin
│   └── capture.bin.sha256
├── derived/
│   ├── transcript.txt
│   ├── decode.jsonl
│   └── annotations.md
├── attachments/
│   └── README.md
└── result.json
```

`result.json` is the bounded downstream projection. It must not expose adapter-control internals or make a product repository parse raw electrical traces.

Example:

```json
{
  "schema": "io.styrene.nex.hardware-evidence-result.v1",
  "evidence_id": "...",
  "kind": "uart-boot-observation",
  "target_profile": "anbernic-rg35xxsp-h700-v1",
  "artifact_digest": "sha256:...",
  "result": "observed",
  "markers": ["u-boot-banner", "linux-version"],
  "raw_sha256": "...",
  "summary_sha256": "..."
}
```

## Differential Evidence

Bring-up should compare a known-good control with an experiment wherever possible. For a UART boot investigation:

1. capture a cold boot using known-good OEM media;
2. confirm adapter, wiring, electrical level, and decoder parameters;
3. capture the experimental image under the same profile;
4. compare the earliest divergent marker;
5. retain both raw sessions and the comparison result.

Silence is not diagnostic evidence until the control proves the observation path.

## Integration With Existing Nex Surfaces

- `nex hardware scan` inventories host-side adapters and candidate target media without granting interaction authority.
- Forge materialization records the artifact digest.
- `forge write-image` records the delivery event and target attestation.
- Hardware evidence sessions reference both artifact and delivery IDs.
- `nex artifact check --evidence boots-hardware` validates a qualifying attested evidence result rather than inferring boot from a successful write.
- `operational` additionally requires product/service health predicates supplied by the product contract.

## Decisions

### Nex owns physical evidence acquisition

**Status:** proposed

**Rationale:** Acquisition requires hardware identity, electrical safety, adapter control, and deployment lifecycle. Those concerns are Nex's domain and are independent of any one product runtime.

### Keep product acceptance declarative

**Status:** proposed

**Rationale:** Products should describe required outcomes and consume bounded results, not embed host serial tools or wiring knowledge.

### Preserve raw evidence and derive interpretations

**Status:** proposed

**Rationale:** Decoder changes must be replayable and auditable without repeating a physical experiment.

### Require control evidence before interpreting silence

**Status:** proposed

**Rationale:** A silent capture may indicate wrong wiring, voltage, baud, adapter, or permissions rather than target boot failure.

## Implementation Slices

1. Define schemas for adapter, connection, session, raw artifact, and result projection.
2. Implement observation-only serial capture as the first backend.
3. Add control-versus-experiment comparison and marker extraction.
4. Bind evidence to Forge artifact and delivery records.
5. Enable attested `boots-hardware` evaluation.
6. Extend to passive SPI/I²C/GPIO and external analyzer imports.
7. Add separately authorized interactive and power-capable operations.

## Open Questions

- What assumptions is this design making that have not been stated?
- Should `.nex/evidence` be the canonical local store, an export format, or both?
- How are photographs and potentially sensitive serial identifiers redacted?
- What minimum adapter capability vocabulary survives UART, SPI, I²C, and logic-analyzer use without becoming an unbounded instrumentation ontology?
- Which evidence markers belong to Nex decoders versus product acceptance contracts?
