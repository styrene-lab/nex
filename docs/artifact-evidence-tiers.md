# Artifact evidence tiers

Nex owns artifact evidence semantics for Armory-distributed Nex artifacts.
Armory may display evidence claims, but Nex defines what a tier means and which
checks can produce it.

## Tiers

| Tier | Meaning | Initial support |
|---|---|---|
| `evaluates` | Pkl evaluates, artifact boundary inspection passes, and typed Nex semantic validation passes. | Supported by `nex artifact check`. |
| `materializes` | Nex can scaffold or produce a materialization workspace/plan for the requested target. | Recognized, unsupported. |
| `builds-image` | Deterministic Nix build completes and produces the expected artifact/output link. | Recognized, unsupported. |
| `boots-emulated` | Built artifact boots in an emulator smoke test. | Recognized, unsupported. |
| `boots-hardware` | Built artifact boots on declared hardware with an attested report. | Recognized, unsupported. |
| `operational` | Post-boot service/health checks pass. | Recognized, unsupported. |

## CLI

```bash
nex artifact check ./path --evidence evaluates --json
```

`evaluates` is the default evidence tier.

Higher tiers currently return a stable unsupported diagnostic instead of
silently passing or inventing validation semantics.

## JSON evidence record

```json
{
  "tier": "evaluates",
  "result": "passed",
  "validated_with": "nex 0.21.0"
}
```
