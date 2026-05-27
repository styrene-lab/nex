# Machine Profiles

Nex 0.18.0 defines **machine profiles** as first-class materialization policy. This deliberately separates Nex machine policy from Omegon/Armory **agent profiles**.

## Boundary

An **agent profile** is owned by Omegon/Armory. It answers what an agent runtime should load:

- persona and tone
- skills and extensions
- tools
- model and default-session policy

A **machine profile** is owned by Nex. It answers what kind of machine or environment Nex may plan, build, provision, or materialize, and what safety/default policy applies:

- identity and version metadata
- minimum Nex version
- default operation mode and target class
- destructive-operation safety posture
- target attestation requirements
- required and optional secret names
- dependencies on Nex-owned artifacts such as forge templates

Machine profiles do **not** define agent persona, skills, model policy, or tool loading.

## Canonical manifest

The canonical file name is:

```text
machine-profile.pkl
```

The top-level schema table is `[machine_profile]`. The v1 schema id is:

```text
io.styrene.nex.machine-profile.v1
```

Example:

```toml
[machine_profile]
schema = "io.styrene.nex.machine-profile.v1"
id = "io.styrene.nex.machine-profile.feature-a-infra"
slug = "feature-a-infra"
name = "Feature A Infra Machine Profile"
version = "1.0.0"
description = "Machine materialization policy for Feature A infrastructure work."
license = "MIT"
min_nex = "0.18.0"

[machine_profile.defaults]
mode = "plan-only"
target = "oci-image"

[machine_profile.safety]
default_destructive = false
requires_confirmation = true
requires_target_attestation = true
allowed_targets = ["nix-devshell", "oci-image", "vm", "physical-machine"]

[machine_profile.secrets]
required = ["GITHUB_TOKEN", "KUBECONFIG"]
optional = ["AWS_PROFILE"]

[[dependencies]]
kind = "forge-template"
id = "nixos-workstation"
version = ">=1.0.0"
required = true
```

## Vocabularies

Initial `mode` values:

- `plan-only`
- `image-build`
- `vm-build`
- `provision`
- `apply-existing`

Initial `target` values:

- `nix-devshell`
- `oci-image`
- `vm`
- `physical-machine`
- `existing-nixos`

Initial dependency kinds:

- `forge-template`

Unknown enum values fail closed.

## Apply-existing target

`apply-existing` with target `existing-nixos` represents profiles that apply package, configuration, and service changes to an already-installed NixOS host. This is the correct target for workstation layers such as low-latency audio or development overlays that do not build installer media, write block devices, or provision a fresh disk.

Example:

```toml
[machine_profile.defaults]
mode = "apply-existing"
target = "existing-nixos"

[machine_profile.safety]
default_destructive = false
requires_confirmation = true
requires_target_attestation = false
allowed_targets = ["existing-nixos"]
```

`existing-nixos` profiles may still mutate system services and packages, so they may require confirmation. They do not require physical target attestation by default because they are not disk-provisioning or raw-device delivery profiles.

## Safety rules

Machine profiles are policy-bearing artifacts. Validation is safety-first:

- secret entries are names only, never values
- secret names use uppercase environment-name syntax: `[A-Z0-9_]`
- the default target must appear in `allowed_targets`
- `physical-machine` defaults require target attestation
- destructive defaults must require confirmation
- unsupported schemas, modes, targets, and dependency kinds fail validation

## Forge boundary

Machine profiles are policy/default packages. Forge templates are materialization payloads.

A machine profile may depend on a forge template. A workstation artifact may reference a machine profile, a forge template, or both. Nex owns parsing, validation, safety gates, dependency semantics, and materialization behavior. Armory may index and distribute the source artifact without executing it.

Forge preflight must eventually evaluate requested operations against the resolved machine-profile policy before any build, provision, disk, or network mutation.

## Armory indexing contract

Armory should index machine profiles as source OCI artifacts with:

```json
{
  "kind": "machine-profile",
  "artifactType": "application/vnd.styrene.nex.machine-profile.v1+tar",
  "schema": "io.styrene.nex.machine-profile.v1",
  "sourcePath": "machine-profiles/<slug>",
  "requiredSecrets": [],
  "optionalSecrets": [],
  "dependencies": []
}
```

Armory may extract metadata, capabilities, required secret names, optional secret names, and dependency references. Armory must not execute, compose, resolve, or enforce machine-profile semantics beyond documented index extraction.

## Deferred beyond 0.18.0

The 0.18.0 boundary establishes terminology, schema, validation, and inspection. These are deferred unless separately specified:

- remote OCI install lifecycle
- lock files
- remote dependency resolution
- machine-profile composition/extends
- full materialization orchestration from machine profile alone
