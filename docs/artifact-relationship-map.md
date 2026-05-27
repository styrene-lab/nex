# Nex artifact relationship map

This document maps the relationship between Nex profile artifacts, materialization
artifacts, starter/profile fragments, delivery artifacts, and the resulting
machine state. It is a reasoning aid for keeping Nex, Armory, and downstream
profile catalogs from mixing semantic boundaries.

## Core principle

```text
catalog metadata discovers artifacts
Nex artifacts describe intent/material
Nex validation proves semantics
build outputs become delivery inputs
machine state is produced only after apply/delivery/boot
```

Armory may distribute and index artifacts. Nex owns artifact semantics,
validation, safety gates, materialization, and delivery behavior.

## Artifact layers

| Layer | Example files | Owner | Answers | Must not answer |
|---|---|---|---|---|
| Catalog metadata | `armory.toml`, API index entry | Armory | How is this artifact discovered, displayed, fetched, and related? | Whether the artifact is safe, valid Nex policy, or buildable. |
| Starter profile | `starters/amd64-server.toml` | Nex profile catalog | Which versioned fragments compose a reusable matrix row? | Machine safety policy or concrete Nix materialization. |
| Profile fragment | `role/server.toml`, `hardware/rpi4.toml` | Nex profile catalog | What reusable partial configuration is available and under what constraints? | Full-machine policy or standalone materialization. |
| Machine profile | `machine-profile.pkl` | Nex | What operation mode/target/safety policy is allowed? | Concrete flake inputs or NixOS module material. |
| Materialization payload | `payload.pkl` | Nex | What concrete Nix inputs/module fragments are needed to evaluate/build? | Safety policy, destructive-operation posture, or target attestation. |
| Build output | `sd-image`, `qcow2`, `raw-image`, `toplevel` | Nex/Nix | What deterministic artifact was built? | Whether it is safe to write/publish/apply. |
| Delivery operation | `write-image`, future publish/import/apply | Nex | Where does an artifact go and what side effects occur? | New artifact semantics. |
| Machine state | existing host config, booted VM, flashed SD card, hardware boot | Target environment | What actually runs? | Catalog/distribution metadata. |

## Relationship graph

```text
starter-profile
  └─ references versioned profile-fragments
       └─ contribute reusable config intent

machine-profile
  ├─ may depend on/recommend starter-profile or fragments by catalog convention
  ├─ defines mode, target, safety, secrets, allowed operations
  └─ gates materialization and delivery operations

materialization-payload
  ├─ declares flake_inputs and nixos_module material
  ├─ may recommend compatible machine profiles through Armory metadata
  └─ feeds deterministic materialization checks/builds

build-output
  └─ produced from materialization-payload under a selected target

machine-state
  ├─ existing-nixos apply: machine-profile gates applying config to current host
  ├─ image-build: payload produces artifact; profile gates operation class
  └─ physical delivery: build-output is written to USB/SD/block device under delivery safety gates
```

## Directionality

Relationships are directional. Do not reverse them casually.

| Relationship | Direction | Meaning |
|---|---|---|
| starter -> fragments | composition | Starter is a named matrix row composed from reusable fragments. |
| materialization-payload -> recommended machine profiles | recommendation | Payload is known or intended to work with these policy profiles. |
| machine-profile -> dependencies | requirement | Profile requires another Nex-owned artifact or template for its operation. |
| build-output -> delivery | input | A built artifact may be delivered to file/device/registry/cloud import. |
| delivery -> machine-state | side effect | Delivery/apply/boot creates or mutates actual machine state. |

## Machine state classes

| Machine state class | Produced by | Example | Safety posture |
|---|---|---|---|
| Existing host state | `apply-existing` / `existing-nixos` profile | `nex-jamkit` style low-latency audio layer applied to current NixOS host | System-mutating; confirmation may be required; no block-device attestation by default. |
| File artifact state | `build-materialization --output` | `qcow2`, `raw-image`, `sd-image` out-link/file | Non-destructive local build; validates reproducibility, not hardware safety. |
| Removable media state | future/interactive `write-image` delivery | USB installer, RPi SD card | Destructive; requires confirmation and usually device attestation. |
| Emulated boot state | emulator smoke test | QEMU boot of image | Runtime evidence; no physical hardware attestation. |
| Hardware boot state | manual or automated hardware boot | RPi4 boots flashed SD card | Requires external/target attestation. |
| Operational state | post-boot checks | SSH reachable, services healthy, mesh link formed | Requires service/health evidence. |

## Safety boundary examples

### Apply-existing profile

```text
machine-profile(mode=apply-existing,target=existing-nixos)
  + fragments/starter composition
  -> mutate current NixOS host configuration
```

This is not disk provisioning. It may still mutate services/packages and should
require confirmation when system-sensitive changes are involved.

### RPi4 SD image

```text
machine-profile(mode=image-build,target=physical-machine)
  + materialization-payload(target=sd-image material)
  -> build sd-image file
  -> write-image to SD card
  -> RPi4 boot state
```

The build step is not destructive. The SD-card write is destructive and must
carry delivery safety gates.

### VM/cloud image

```text
machine-profile(mode=image-build,target=vm/cloud-image class)
  + materialization-payload(target=qcow2/raw material)
  -> build local image artifact
  -> external image factory may consume artifact
```

Nex should not model Packer/cloud publication lifecycle unless a future design
explicitly adds a delivery backend. The deterministic image artifact remains the
Nex-owned output.

## Evidence mapping

| Evidence tier | Relationship proven | Current status |
|---|---|---|
| `evaluates` | Artifact source evaluates and passes boundary/typed validation. | Implemented by `nex artifact check`. |
| `materializes` | Payload/profile can produce a workspace or plan. | Future. |
| `builds-image` | Deterministic build output exists. | Future. |
| `boots-emulated` | Built artifact reaches boot smoke-test state in emulator. | Future. |
| `boots-hardware` | Built artifact boots on declared hardware. | Future; requires attestation/report. |
| `operational` | Post-boot service checks pass. | Future; target-specific. |

## Validation commands

Current first-pass commands:

```bash
nex artifact check ./machine-profiles/example --json
nex artifact check ./materialization-payloads/example --json
nex artifact check ./path --evidence evaluates --json
nex artifact check-relationship --profile ./profile --payload ./payload --json
```

The relationship checker currently proves that the pair is structurally valid:

- profile is a valid machine-profile artifact;
- payload is a valid materialization-payload artifact;
- each artifact respects its semantic boundary.

Deeper target/build/delivery compatibility should be added only after the target
vocabularies are stable enough to avoid inventing semantics.

## Non-goals

- Armory does not parse Pkl or run Nix.
- Materialization payloads do not define safety policy.
- Machine profiles do not define flake/module material.
- Starters are not full machine profiles.
- Fragments are not standalone materialization artifacts.
- Packer/cloud providers are external consumers unless a future delivery backend explicitly models them.
