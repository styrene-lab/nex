# Artifact relationship validation

Nex validates semantic compatibility between local Armory-distributed artifact
pairs. Armory may publish catalog relationship links, but Nex owns the semantic
checks once artifacts are fetched or available locally.

## CLI

```bash
nex artifact check-relationship \
  --profile ./machine-profiles/styrene.rpi4-kiosk \
  --payload ./materialization-payloads/styrene.rpi4-kiosk-sd-image \
  --json
```

## Semantics

Relationship validation composes standalone artifact validation:

1. `--profile` must be a valid `machine-profile` artifact directory.
2. `--payload` must be a valid `materialization-payload` artifact directory.
3. Pair-level checks only run after both artifacts pass standalone validation.

Deep target compatibility is intentionally conservative until target/build/delivery
vocabulary is explicit in both schemas.

## JSON contract

```json
{
  "ok": true,
  "profile": {
    "id": "styrene.rpi4-kiosk",
    "schema": "io.styrene.nex.machine-profile.v1",
    "artifact_kind": "machine-profile",
    "ok": true
  },
  "payload": {
    "id": "styrene.rpi4-kiosk-sd-image",
    "schema": "io.styrene.nex.materialization-payload.v1",
    "artifact_kind": "materialization-payload",
    "ok": true
  },
  "compatibility": {
    "systems": [],
    "targets": [],
    "build_targets": []
  },
  "diagnostics": []
}
```

Diagnostics use the same shape as `nex artifact check`:

```json
{
  "severity": "error",
  "code": "relationship-payload-invalid",
  "message": "payload artifact failed standalone validation",
  "path": "payload"
}
```
