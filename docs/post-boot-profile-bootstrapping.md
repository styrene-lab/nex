# Post-Boot Profile Persistence and Nex Bootstrapping

Status: immediate repair implemented; immutable provenance follow-up remains

Branch: `fix/post-boot-profile-bootstrapping`

Date: 2026-08-22

## Implementation Result

The immediate repair now:

- writes and reads one `profile/{source,resolved.toml,state.json}` contract;
- validates completeness, TOML, source/state consistency, and the snapshot digest
  before Polymerize begins interactive setup or disk mutation;
- rejects secret-bearing top-level profile data and unsupported legacy payloads;
- persists the exact validated payload under `~/nix-config/.nex/profile`;
- removes stale profile state when a Forge output directory is reused for a
  generic installer; and
- declares `github:styrene-lab/nex` as a generated flake input and installs its
  default package in the NixOS system closure.

The transitional state records the textual base-to-leaf chain but does not claim
that mutable references are immutable revisions. Resolving and recording commit
IDs and per-layer hashes remains the next provenance follow-up described below.

## Goal

A machine installed by `nex forge` and `nex polymerize` should retain enough
profile state to explain, reproduce, and intentionally update its configuration
after reboot. The installed system should also contain a working `nex` binary on
`PATH` without depending on the live installer, Cargo, or an imperative download.

## Evidence From This Machine

The current host demonstrates the gap:

- It is running NixOS 26.05 as host `nucleus`.
- `/run/current-system` exactly matches the result of building
  `/home/wilson/nix-config#nixosConfigurations.nucleus.config.system.build.toplevel`.
- `/home/wilson/nix-config/flake.nix` describes itself as installed by
  `nex polymerize`.
- The materialized files contain profile-derived choices such as COSMIC, Intel
  graphics, K3s settings, packages, and shell aliases.
- No original profile source, resolved profile snapshot, profile chain, revision,
  digest, or Nex version is retained in `/home/wilson/nix-config`.
- `nex` is not installed on `PATH`.
- `~/.config/nex/config.toml` points at the correct repository but contains the
  stale hostname `styrenehub`; the active host and flake use `nucleus`.
- `/etc/nixos` is empty because Polymerize correctly places the writable
  configuration in `/home/wilson/nix-config`.

The system is therefore Nex-generated, but it is not currently a durable,
profile-managed Nex system. Only flattened profile effects survived.

## Current Data Flow

### Forge

`src/ops/forge.rs` resolves a profile chain and retains two in-memory values:

- `ResolvedProfile.merged`: flattened TOML after merging the layers.
- `ResolvedProfile.chain`: textual layer references.

The chain is printed but not serialized. Forge writes:

```text
styrene/
  nex
  defaults/
  profile/
    machine-profile.pkl
    source
```

The content written as `machine-profile.pkl` is actually the merged TOML string,
not canonical Pkl. `source` contains only the original leaf reference. It does
not record resolved revisions, all layers, hashes, signatures, or the Nex
version used to resolve the profile. See `src/ops/forge.rs:544-561` and
`src/ops/forge.rs:629-640`.

### Polymerize

Polymerize reads profile content only from:

```text
profile/profile.toml
nex/profile.toml
```

It does not read the `profile/machine-profile.pkl` file produced by Forge. See
`src/ops/polymerize.rs:85-98`.

This filename and format mismatch means a current Forge bundle can preserve the
profile label shown to the operator while providing no profile content to the
materializer.

When profile content is available, `exec_write_config` parses it with:

```rust
profile_toml.and_then(|t| toml::from_str(t).ok())
```

An invalid profile is therefore silently treated as no profile. See
`src/ops/polymerize.rs:1290-1305`.

Polymerize flattens supported profile fields into `configuration.nix` and
`home.nix`. It does not pass the profile reference to `exec_write_config` and
does not copy either the merged content or provenance into the target config.
The installed Nex config records only `repo_path` and `hostname`; see
`src/ops/polymerize.rs:1820-1830`.

### Nex Binary

Forge bundles a target Nex binary for use by the live installer, but Polymerize
does not copy or declare it in the installed system. The generated flake includes
only `nixpkgs` and Home Manager; see `src/ops/polymerize.rs:1312-1347`.

The Nex repository already exposes a package as:

```nix
nex.packages.${system}.default
```

for all supported Linux and Darwin systems. Its wrapper includes Pkl on the
private runtime `PATH`; see `flake.nix:16-40`.

## Required Properties

The fix should provide these invariants:

1. Accepting a bundled profile requires valid profile content. Missing or invalid
   content must stop installation before destructive work begins.
2. The installed config retains the exact validated snapshot that generated it.
3. The installed config retains immutable provenance for every resolved layer.
4. Reapplying the recorded profile is deterministic and works offline.
5. Updating a profile is a separate, explicit networked operation.
6. Nex is part of the NixOS system closure and survives reboot.
7. Nex versions follow NixOS generations, updates, and rollbacks.
8. Profile-derived output has a clear ownership boundary so stale values can be
   removed during an update.

## Recommended Installed Layout

Keep profile state beside the declarative config so it can be versioned and
relocated with that config:

```text
~/nix-config/
  flake.nix
  configuration.nix
  hardware-configuration.nix
  home.nix
  nex-profile.nix
  nex-profile-home.nix
  .nex/
    profile/
      state.json
      resolved.toml
```

`resolved.toml` is the exact validated, secret-free merged snapshot used for
materialization. The generated `nex-profile.nix` and `nex-profile-home.nix` files
are owned by Nex and imported by the stable user configuration.

At minimum, `state.json` should contain:

```json
{
  "schema": 1,
  "requestedSource": "github:owner/profile@main",
  "resolvedLeaf": "github:owner/profile/<commit>",
  "profileId": "optional-profile-id",
  "profileVersion": "optional-profile-version",
  "layers": [
    {
      "requested": "github:owner/base@main",
      "resolved": "github:owner/base/<commit>",
      "sha256": "..."
    }
  ],
  "resolvedContentSha256": "...",
  "appliedAt": "...",
  "nexVersion": "...",
  "resolverSchema": "legacy-profile-v1"
}
```

The mutable requested source is needed to discover updates. The immutable
resolved source and content digest are needed to reproduce and audit the current
machine. Recording only `owner/repo` is not sufficient.

Do not put resolved secrets in either file.

## Recommended Nex Bootstrap

Add Nex as a flake input in the configuration generated by Polymerize and install
it as a NixOS system package:

```nix
inputs.nex = {
  url = "github:styrene-lab/nex";
  inputs.nixpkgs.follows = "nixpkgs";
};

outputs = { self, nixpkgs, home-manager, nex }:
{
  nixosConfigurations."nucleus" = nixpkgs.lib.nixosSystem {
    system = "x86_64-linux";
    modules = [
      {
        environment.systemPackages = [
          nex.packages."x86_64-linux".default
        ];
      }
    ];
  };
};
```

The generated template should substitute the detected system rather than
hard-code `x86_64-linux`.

This is preferable to installing Nex through Home Manager, `nix profile`, a
release download, or Cargo because it:

- makes `nex` available to both the target user and administrative shells;
- is available immediately after boot, independent of shell initialization;
- roots Nex and its Pkl runtime in the installed system closure;
- makes `nex update` update the Nex flake input with the rest of the system; and
- rolls the Nex version backward with a NixOS generation rollback.

`nex self-update` should not be used for this installation type because a
Nix-managed executable must be updated by changing the flake lock and rebuilding,
not by replacing a file in `/nix/store`.

## Suggested Implementation Order

### 1. Repair the Forge/Polymerize Contract

Define one explicit bundle contract instead of writing TOML under a Pkl filename.
A minimal transitional contract is:

```text
profile/
  source
  resolved.toml
  state.json
```

Forge should serialize the complete resolved chain and hashes. Polymerize should
read exactly those names and reject an incomplete profile payload.

### 2. Fail Closed Before Disk Mutation

Parse and validate the selected profile before partitioning or formatting the
target. Replace the `.ok()` parse suppression with contextual errors. If an
operator accepts a bundled profile, absent content must be an error rather than a
generic installation fallback.

### 3. Persist Snapshot and Provenance

Pass profile reference and resolution metadata through the Polymerize execution
path. Write `.nex/profile/state.json` and `.nex/profile/resolved.toml` beneath
the target `nix-config` before `nixos-install`, then set ownership with the rest
of that directory.

### 4. Bootstrap Nex Declaratively

Update the Polymerize flake template to include `github:styrene-lab/nex` and add
its default package to `environment.systemPackages`.

Pinning policy needs an explicit decision:

- Following the flake lock is the normal operational model and lets `nex update`
  update Nex.
- Forge may additionally pin the initial Nex input to the installer version or
  commit to ensure the first installed binary exactly matches the materializer.

The latter gives stronger reproducibility, while the former is the smallest
working change. In either case the generated `flake.lock` becomes the installed
Nex version lock.

### 5. Separate Reapply From Update

Future command behavior should distinguish:

- `nex profile reapply`: verify the stored digest, regenerate owned modules from
  `resolved.toml`, and rebuild without network access.
- `nex profile update`: resolve the requested source to new immutable revisions,
  show the source/content/effect diff, atomically replace state and generated
  modules, rebuild, and restore all files on failure.

Do not make every rebuild implicitly fetch a mutable profile branch.

### 6. Unify Profile Resolution

There are currently separate paths for Forge profile resolution, `profile apply`,
first-class machine profile parsing, and materialization payloads. A later cleanup
should establish one resolver that:

- evaluates canonical Pkl rather than parsing it as TOML;
- has an explicit compatibility path for legacy TOML;
- supports `extends` and `compose` consistently;
- rejects cycles and excessive depth;
- resolves remote layers to immutable revisions;
- validates profile policy and target constraints; and
- returns merged content together with complete provenance.

This unification is larger than the immediate post-boot repair and should not
block the minimal bundle-contract and Nex-bootstrap fixes.

## Test Plan

Add focused tests before broad resolver work:

1. Forge a profile bundle and assert that source, resolved content, and state are
   present under the documented names.
2. Load that bundle through Polymerize and assert that profile effects appear in
   generated Nix files.
3. Assert that missing accepted profile content fails before disk operations.
4. Assert that malformed profile content returns a contextual error instead of
   producing a generic system.
5. Assert that the installed config contains `.nex/profile/state.json` and
   `.nex/profile/resolved.toml` with matching hashes.
6. Assert that every base, composed, and leaf layer is represented in state.
7. Assert that generated `flake.nix` declares the Nex input and installs its
   default package in `environment.systemPackages`.
8. Evaluate or build the generated flake and confirm `nex` is present in the
   resulting system closure.
9. Assert that a failed profile update restores the old snapshot, state, generated
   modules, and flake lock.
10. Assert that an offline reapply does not contact the profile source.

The existing Polymerize bundle test covers defaults such as networking and SSH
keys, but it does not exercise the Forge profile filename/content handoff.

## Immediate Scope Recommendation

For the next implementation session, keep the first patch narrow:

1. Correct and test the bundle profile filename/format contract.
2. Make profile parsing fail closed.
3. Preserve the leaf source plus exact merged snapshot in `~/nix-config/.nex`.
4. Add Nex to the generated system flake and test that output.

Complete immutable per-layer provenance and unified Pkl resolution in a follow-up
once the post-boot path is no longer lossy.
