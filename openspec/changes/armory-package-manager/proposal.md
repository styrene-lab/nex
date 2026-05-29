# Armory Package Manager Integration

## Intent

Make Nex the stable resolver and installer plane for public Armory package references while Omegon remains the runtime activation layer.

## Scope

Phase 1 implements read-only registry discovery:

- Configure Armory registries in Nex config.
- Fetch and parse Armory index JSON.
- Search Armory entries from `nex search`.
- Show metadata and dependencies with `nex info <kind>/<id>`.

Later phases will implement dependency locking, OCI fetch/verify, artifact validation, and Omegon activation locks.

## Success Criteria

- Operators can configure a registry URL.
- `nex search security` includes matching Armory packages when a registry is configured.
- `nex info profile/rust-shop` prints metadata, dependencies, install commands, and activation metadata.
- Existing Nix package search behavior remains available.
