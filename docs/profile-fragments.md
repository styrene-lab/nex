# Profile Fragments

Nex 0.18.0 defines **profile fragments** as reusable partial machine configuration documents that can be composed into a larger Nex machine profile or materialization flow.

A profile fragment answers:

> What reusable partial machine/profile configuration can be composed, and under what constraints?

It does not answer:

> What full machine should Nex materialize?

That remains the role of machine profiles, forge templates, and workstation handoffs.

## Artifact decision

Fragments are **first-class semantic objects inside a fragment catalog/repository**, but they are **not standalone materialization artifacts** in 0.18.0.

Implications:

- fragments carry explicit SemVer versions for catalog compatibility
- fragments have validation semantics
- fragments can declare dependencies, conflicts, platforms, and safety markers
- Armory may index them as catalog entries if a repo opts in
- Nex does not install or materialize an individual fragment by itself
- fragments are versioned individually but can still be distributed inside a containing repository/catalog unless a later spec adds independent artifact publishing

## Canonical embedded metadata

Fragment metadata lives inside each fragment TOML file:

```toml
[fragment]
schema = "io.styrene.nex.profile-fragment.v1"
id = "gpu/amd"
name = "amd"
version = "0.1.0"
description = "AMD GPU — amdgpu, mesa, Vulkan, VA-API, 32-bit"
category = "gpu"
requires = ["platform/linux"]
conflicts = ["gpu/nvidia", "gpu/intel"]
platforms = ["linux"]
visibility = "public"

[fragment.safety]
mutates_system_services = false
mutates_hardware_drivers = true
requires_confirmation = true
```

## Versioning

`version` is required and must be valid SemVer. Initial migrated fragments should start at `0.1.0`.

Version increments follow ordinary SemVer discipline:

- `PATCH`: metadata, docs, or package-list corrections that should not alter broad behavior
- `MINOR`: new options, packages, services, supported platforms, or compatibility expansion
- `MAJOR`: breaking config behavior, removed fields/options, changed safety assumptions, or incompatible merge behavior

## IDs and categories

Fragment IDs use path-like two-part slugs:

```text
<category>/<slug>
```

Examples:

- `core/essentials`
- `platform/linux`
- `desktop/cosmic`
- `gpu/amd`
- `audio/pipewire`
- `hardware/rpi4`
- `shell/bash`
- `role/gaming`

Initial categories:

- `core`
- `platform`
- `desktop`
- `gpu`
- `audio`
- `hardware`
- `shell`
- `role`

The ID category must match the declared `category` field. When validating a repository directory, Nex also checks that the metadata ID matches the path-derived ID, e.g. `gpu/amd.pkl` must declare `id = "gpu/amd"`.

## Dependency and conflict semantics

`requires` lists other fragment IDs that must be present in the composed set.

`conflicts` lists fragment IDs that must not be composed with this fragment.

0.18.0 defines validation vocabulary and metadata shape. Full compose-graph solving is intentionally separate from individual fragment validation.

## Platform and safety semantics

`platforms` must contain at least one of:

- `any`
- `linux`
- `macos`

`any` cannot be combined with specific platforms.

Safety metadata is required for system-sensitive categories:

- `platform`
- `desktop`
- `gpu`
- `audio`
- `hardware`

Hardware fragments describe device/board-specific assumptions such as boot, firmware, kernel module, and hardware-driver requirements. They are intended for targets like Raspberry Pi boards, Apple T2 machines, and edge appliances.

Hardware-driver fragments must require confirmation.

## Armory integration

If Armory indexes fragments, use:

```json
{
  "kind": "profile-fragment",
  "id": "gpu/amd",
  "version": "0.1.0",
  "sourcePath": "gpu/amd.pkl",
  "artifactType": "application/vnd.styrene.nex.profile-fragment.v1+toml",
  "dependencies": [
    { "kind": "profile-fragment", "id": "platform/linux", "required": true }
  ],
  "conflicts": ["gpu/nvidia", "gpu/intel"],
  "platforms": ["linux"]
}
```

Armory may index fragment metadata and relationships. Nex owns validation, dependency/conflict semantics, merge semantics, and materialization behavior.
