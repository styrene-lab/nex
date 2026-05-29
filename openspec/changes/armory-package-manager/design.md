# Design

## Phase 1 — registry discovery (implemented)

Add a small `armory` module that owns:

- registry configuration types;
- package reference parsing (`<kind>/<id>`);
- index fetch/parse;
- query and lookup helpers;
- display rendering for search/info.

Nex config accepts:

```toml
[[registries]]
name = "styrene-armory"
url = "https://armory.styrene.io/api/index.json"
trust = "signed"
```

Canonical Pkl config keeps the same field names. If no registries are configured, Nex uses a built-in default registry for discovery only.

## Phase 2 — lock-only graph resolution

Goal: make `nex install <kind>/<id>` deterministic without yet pulling OCI payloads.

### CLI surface

- `nex install profile/rust-shop --dry-run`
  - resolves the registry package graph;
  - prints planned root/dependencies;
  - does not write files.
- `nex install profile/rust-shop`
  - resolves required dependencies recursively;
  - skips optional dependencies with warnings;
  - writes a package lock containing exact package refs, versions, registry, OCI refs, digests, and dependency edges;
  - for Omegon-runtime roots, writes a provisional activation lock with package refs and unresolved local paths marked `pending`.
- `nex lock refresh`
  - re-resolves existing roots from the package lock;
  - updates versions/digests according to registry state.

### Files

Use a project/user-scoped Nex state directory, not the Nix config repo:

```text
~/.local/state/nex/packages.lock.json
~/.local/state/nex/omegon-activation-lock.json
```

Lock schema v1:

```json
{
  "schema": "io.styrene.nex.package-lock.v1",
  "registries": [{ "name": "styrene-armory", "url": "https://..." }],
  "roots": [{ "packageRef": "profile/rust-shop" }],
  "packages": [
    {
      "packageRef": "skill/rust",
      "version": "1.0.0",
      "registry": "styrene-armory",
      "ociRef": "oci://...",
      "digest": "sha256:...",
      "dependencies": []
    }
  ]
}
```

Activation lock v1 remains intentionally incomplete until Phase 3 materializes local paths:

```json
{
  "schema": "io.styrene.nex.omegon-activation-lock.v1",
  "root": { "kind": "profile", "id": "rust-shop", "version": "1.0.0" },
  "packages": [
    { "kind": "skill", "id": "rust", "version": "1.0.0", "status": "pending" }
  ]
}
```

### Resolver rules

- Required dependencies are recursively resolved.
- Optional dependencies are reported and omitted unless a later flag includes them.
- Missing required dependency is a hard error.
- Cycles are a hard error with a displayed cycle path.
- Duplicate refs collapse to one package; conflicting versions/digests are a hard error.
- Registry lookup is by exact `packageRef`.

### Non-goals for Phase 2

- No OCI fetch.
- No digest/signature verification beyond recording registry-provided fields.
- No artifact validator execution.
- No runtime activation.

## Phase 3 — OCI fetch, store, and validation

Goal: turn resolved locks into local installed packages.

### Store layout

Use content-addressed package directories:

```text
~/.local/share/nex/store/sha256-<digest>/
~/.local/share/nex/store/ref/<kind>/<id> -> ../../sha256-<digest>
```

If a package has no digest, use a registry/ref/version staging path but mark it untrusted unless the registry trust policy allows it.

### Fetch behavior

- Pull `ociRef` with a small OCI abstraction.
- Prefer `oras` if present for initial implementation; later replace with native OCI client if needed.
- Verify digest after fetch.
- Enforce registry trust policy:
  - `trust = "unsigned"` allows digest-only/unverified packages;
  - `trust = "signed"` requires a signature/attestation once signature metadata exists.

### Validation

Invoke existing validators by package kind:

| Kind | Validator |
|---|---|
| `machine-profile` | machine profile validation |
| `materialization-payload` | materialization validation |
| `forge-template` | forge request/template validation |

Omegon runtime packages (`skill`, `persona`, `tone`, `profile`, `agent`, `extension`, `workstation`) are schema-validated according to package metadata/manifests once extracted.

### Lock update

After fetch, package lock entries gain:

```json
{
  "path": "/Users/.../.local/share/nex/store/sha256-...",
  "verified": true,
  "installedAt": "..."
}
```

Activation lock entries gain concrete local paths.

## Phase 4 — Omegon activation handoff

Goal: allow Omegon to run without registry access.

### Activation lock ownership

Nex writes:

```text
~/.local/state/nex/omegon-activation-lock.json
```

Omegon reads this lock and does not fetch Armory at runtime when local paths exist.

### Runtime semantics

| Kind | Activation |
|---|---|
| `skill` | load skill guidance |
| `persona` | load persona directive/memory seeds |
| `tone` | load tone guidance |
| `profile` | set agent profile defaults |
| `agent` | instantiate configured agent |
| `extension` | register tools/surfaces, enabled per lock |
| `workstation` | composite activation plus Nex/Nix materialization handoff |

### Commands

- `nex lock refresh`
- `nex list --armory` or `nex armory list` (choose in implementation)
- `nex remove <kind>/<id>` updates roots and regenerates locks.

## Cross-phase constraints

- Do not collapse `profile` and `machine-profile`.
- Trust failures must fail closed.
- Locks are the source of truth after resolution.
- Network access belongs to Nex, not Omegon runtime.
- Each phase must ship with tests and remain useful independently.
