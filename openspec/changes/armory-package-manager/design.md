# Design

## Phase 1

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

## Non-goals for Phase 1

- No OCI pulls.
- No signature verification.
- No package lock writes.
- No Omegon activation lock writes.
- No dependency graph install mutation.

Those are intentionally deferred so the registry contract can stabilize first.
