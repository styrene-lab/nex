## 1. Registry read/search/info
<!-- specs: armory/registry -->

- [x] Add registry config shape to local Nex config.
- [x] Add Armory index fetch/parse support.
- [x] Extend `nex search` to include Armory registry entries.
- [x] Add `nex info <kind>/<id>` for Armory metadata and dependencies.
- [x] Validate with unit tests and existing command checks.

## 2. Lock-only graph resolution
<!-- specs: armory/lock -->

- [x] Add package lock structs and JSON serialization.
- [x] Add state path helpers for `~/.local/state/nex/packages.lock.json` and activation lock.
- [x] Implement dependency graph resolver over fetched registry indexes.
- [x] Detect missing required dependencies.
- [x] Detect dependency cycles and report the cycle path.
- [x] Collapse duplicate refs and reject conflicting version/digest records.
- [x] Route `nex install <kind>/<id>` to Armory lock-only install while preserving existing Nix/Homebrew install behavior for bare package names.
- [x] Implement `--dry-run` output for Armory installs.
- [x] Write package lock for non-dry-run installs.
- [x] Write provisional Omegon activation lock for Omegon-runtime roots.
- [x] Add tests for graph resolution, cycle detection, lock serialization, and install routing.

## 3. OCI fetch, store, and validation
<!-- specs: armory/store -->

- [ ] Add content-addressed Nex package store layout.
- [ ] Add OCI fetch abstraction, initially shelling out to `oras` when available.
- [ ] Verify fetched payload digest against registry metadata.
- [ ] Enforce registry trust policy, failing closed for signed registries without verifiable metadata.
- [ ] Extract payloads into the Nex store.
- [ ] Invoke existing validators for `machine-profile`, `materialization-payload`, and `forge-template` packages.
- [ ] Update package lock entries with installed paths and verification status.
- [ ] Add tests for digest mismatch, missing `oras`, validator dispatch, and store path calculation.

## 4. Omegon activation handoff
<!-- specs: armory/activation -->

- [ ] Finalize activation lock schema.
- [ ] Populate activation lock entries with local package paths after store materialization.
- [ ] Encode runtime defaults for `profile`, `agent`, `extension`, and `workstation` roots.
- [ ] Add `nex lock refresh` to re-resolve roots and update locks.
- [ ] Add remove/list UX for installed Armory package roots.
- [ ] Add tests proving Omegon-runtime activation locks do not require registry access.
