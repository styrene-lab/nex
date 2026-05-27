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

## First-pass semantics

Relationship validation composes standalone artifact validation:

1. `--profile` must be a valid `machine-profile` artifact directory.
2. `--payload` must be a valid `materialization-payload` artifact directory.
3. Pair-level checks only run after both artifacts pass standalone validation.

Deep target compatibility is intentionally deferred until target/build/delivery
vocabulary is stable enough to check without inventing semantics.

## JSON shape

```json
{
  "ok": true,
  "profile": { "ok": true },
  "payload": { "ok": true },
  "relationship": "machine-profile/materialization-payload",
  "diagnostics": []
}
```
