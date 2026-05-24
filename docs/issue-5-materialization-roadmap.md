# Issue 5 Design: Materialization Modules and Hermetic NixOS Builds

Issue: <https://github.com/styrene-lab/nex/issues/5>

## Status

Planned for **0.19.0**. Do not expand 0.18.0 beyond issue #1, #3, and #4 closure.

## Terminology translation

Issue #5 was written using the pre-0.18 overloaded word "profile". After the 0.18.0 machine-profile and profile-fragment boundaries, translate the requested capabilities as follows:

| Issue language | 0.18+ Nex term | Owner |
|---|---|---|
| profile flake inputs | materialization payload/module flake inputs | materialization module generator |
| profile export as `nixosModule` | materialization module output | `build-module` / forge output |
| hardware image build | forge output format | forge planner/executor |
| `extra_config` validation | materialization/forge preflight evaluation | forge check / local Nix eval |

A `machine-profile.toml` remains policy/default/safety metadata. It should not become an arbitrary Nix payload container.

A `profile-fragment` remains a reusable partial configuration object inside a catalog. It may contribute payload sections, but it is not materialized standalone.

A **materialization module/payload** is the missing concept for issue #5: the generated NixOS/Home Manager module contract that owns flake inputs, extra config, and composable module export.

## Goals

- Allow materialization payloads to declare third-party flake inputs.
- Export generated configuration as composable `nixosModules`.
- Validate generated NixOS configurations locally before target install or disk mutation.
- Build hermetic hardware images such as Raspberry Pi SD images.
- Preserve the machine-profile safety boundary from issue #3.

## Non-goals for 0.19.0

- Making machine profiles arbitrary Nix modules.
- Installing profile fragments as standalone machines.
- Replacing forge templates with machine profiles.
- Supporting every NixOS image target on day one.

## Milestones

### 0.18.0 — Boundary release

Scope:

- Issue #1 flat-layout import corruption fix.
- Issue #3 machine-profile schema/docs/CLI.
- Issue #4 profile-fragment schema/docs/CLI.

Exit criteria:

- `cargo check` passes.
- `cargo test` passes.
- 0.18.0 changelog and version bump committed.
- Issue #1 closed with evidence.
- Issues #3 and #4 have implementation evidence comments; close if maintainers accept schema/contract-only completion.

### 0.19.0 — Materialization modules and hermetic builds

Scope:

1. Local NixOS evaluation / forge check.
2. Materialization flake inputs.
3. `nixosModules` export.
4. Hermetic `sd-image` forge output.

Exit criteria:

- Generated NixOS config can be evaluated locally before install.
- Materialization payloads can declare flake inputs and use them in generated modules.
- Nex can export a composable NixOS module.
- Nex can build at least one hermetic hardware image target, initially Raspberry Pi 4 / `aarch64` if feasible.

## Design nodes

### 1. Forge materialization check

Add a local validation path that evaluates generated NixOS configuration before disk/image/install actions.

Candidate command surfaces:

```text
nex forge check <request-or-template>
nex forge run --check-only --request <request>
```

Current CLI already has `nex forge check <path>` for forge templates. Extend the capability carefully rather than overloading behavior invisibly.

Acceptance:

- Invalid `extra_config` syntax fails locally.
- Invalid NixOS module options fail locally.
- Diagnostics identify the generated file/module where possible.
- No disk/network mutation occurs during check.

Implementation notes:

- Generate a temporary flake workspace.
- Run `nix eval .#nixosConfigurations.<host>.config.system.build.toplevel` or equivalent.
- Use argument-array process spawning, not shell interpolation.
- Keep tests mockable; full Nix eval can be integration-gated.

### 2. Materialization flake inputs

Add a payload-level `[flake_inputs]` section.

Example:

```toml
[flake_inputs]
dns-dhcp = "github:styrene-lab/dhcp-dns-work"
nixos-hardware = "github:NixOS/nixos-hardware"
```

Acceptance:

- Generated `flake.nix` includes declared inputs.
- Input names are validated as Nix identifiers.
- Input refs reject shell/path injection patterns.
- Inputs are available to generated modules via `specialArgs` or documented namespace.
- Tests assert generated flake content.

Boundary:

- This belongs to the materialization payload/module layer, not `machine-profile.toml`.

### 3. NixOS module export

Add a composable module export path.

Candidate command:

```text
nex build-module <source> --name <module-name> --output <dir>
```

Candidate output:

```nix
{
  outputs = { self, nixpkgs, ... }@inputs: {
    nixosModules.<name> = import ./module.nix { inherit inputs; };
  };
}
```

Acceptance:

- Existing flake can import the generated module.
- Generated module does not assume ownership of full host unless explicitly requested.
- Module export includes package/service/desktop settings derived from the payload.
- Tests validate generated file shape and import paths.

### 4. Hermetic hardware image output

Add forge output for prebuilt disk images.

Candidate CLI/request shape:

```text
nex forge --format sd-image --arch aarch64
```

or request-model equivalent:

```json
{
  "operation": "image",
  "format": "sd-image",
  "arch": "aarch64"
}
```

Acceptance:

- Builds complete image from `nixosSystem.config.system.build.sdImage` or target-specific equivalent.
- No target-side `polymerize` required.
- Output is airgap-safe after build.
- Machine-profile policy can block/allow image target by mode/target/safety.

Initial target:

- Raspberry Pi 4 / `aarch64`, if NixOS module support is available through `nixos-hardware` or NixOS image modules.

## Dependency order

Recommended order:

```text
forge-materialization-check
  -> materialization-flake-inputs
      -> nixos-module-export
          -> hermetic-sd-image-output
```

Rationale:

- Evaluation first prevents slow target/install failures.
- Flake inputs are prerequisite for third-party packages/modules.
- Module export exercises composability before hardware image generation.
- SD image output should only land after generated configs are evaluable and dependencies are explicit.

## Open questions

- What is the canonical name for the materialization payload file after 0.18.0?
- Does `extra_config` remain raw TOML string fields, or move into generated module snippets?
- Should `build-module` accept fragment catalogs directly, or only resolved materialization payloads?
- How much Nix evaluation should run in unit tests vs integration tests?
- What is the minimum supported hardware image target for first release?
