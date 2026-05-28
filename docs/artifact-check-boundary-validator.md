# Artifact check boundary validator

Nex owns semantic validation for Nex artifact directories. Armory owns catalog,
packaging, and distribution metadata. The boundary is the CLI:

```bash
nex artifact check <artifact-dir> --json
```

## Artifact directory shapes

```text
machine-profiles/<id>/
├── machine-profile.pkl
├── armory.toml       # optional to Nex
└── README.md         # optional to Nex

materialization-payloads/<id>/
├── payload.pkl
├── armory.toml       # optional to Nex
└── README.md         # optional to Nex
```

Nex keys off canonical source files, not `armory.toml`:

- `machine-profile.pkl` => `machine-profile`
- `payload.pkl` => `materialization-payload`

## Validation pipeline

1. Detect artifact kind by canonical entrypoint.
2. Evaluate Pkl to raw JSON.
3. Inspect raw JSON for forbidden cross-boundary fields before typed deserialization.
4. Run typed Nex semantic validation.
5. If `armory.toml` exists, validate only the boundary fields Nex understands.
6. Emit a JSON report and return non-zero when `ok` is false.

## JSON success contract

```json
{
  "ok": true,
  "path": "materialization-payloads/styrene.rpi4-kiosk-sd-image",
  "artifact_kind": "materialization-payload",
  "id": "styrene.rpi4-kiosk-sd-image",
  "schema": "io.styrene.nex.materialization-payload.v1",
  "version": null,
  "entrypoint": "payload.pkl",
  "evidence": {
    "tier": "evaluates",
    "result": "passed",
    "validated_with": "nex 0.21.4"
  },
  "diagnostics": []
}
```

## JSON failure contract

```json
{
  "ok": false,
  "path": "materialization-payloads/bad",
  "artifact_kind": "materialization-payload",
  "id": "bad",
  "schema": "io.styrene.nex.materialization-payload.v1",
  "version": null,
  "entrypoint": "payload.pkl",
  "evidence": {
    "tier": "evaluates",
    "result": "failed",
    "validated_with": "nex 0.21.4"
  },
  "diagnostics": [
    {
      "severity": "error",
      "code": "forbidden-boundary-field",
      "message": "materialization-payload artifacts must not declare machine-profile policy",
      "path": "machine_profile"
    }
  ]
}
```

## Evidence tiers

`evaluates` is the default evidence tier. Higher tiers are recognized separately
and fail with `unsupported-evidence-tier` until Nex implements validators for
those claims.
