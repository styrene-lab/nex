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

- fragments have stable IDs
- fragments have validation semantics
- fragments can declare dependencies, conflicts, platforms, and safety markers
- Armory may index them as catalog entries if a repo opts in
- Nex does not install or materialize an individual fragment by itself
- fragments inherit distribution/version context from their containing repository/catalog unless a later spec adds independent versioning

## Canonical embedded metadata

Fragment metadata lives inside each fragment TOML file:

```toml
[fragment]
schema = "io.styrene.nex.profile-fragment.v1"
id = "gpu/amd"
name = "amd"
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
- `shell/bash`
- `role/gaming`

Initial categories:

- `core`
- `platform`
- `desktop`
- `gpu`
- `audio`
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

Hardware-driver fragments must require confirmation.

## Armory integration

If Armory indexes fragments, use:

```json
{
  "kind": "profile-fragment",
  "id": "gpu/amd",
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
