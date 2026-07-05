# Nex provider-aware package installation plan

## Status

Design plan for correcting the current macOS/nix-darwin/Homebrew bias in imperative package operations while preserving the intended model where Homebrew can exist as a convenience/necessity layer on top of Determinate Nix, including on NixOS machines.

## Problem

`nex install` currently conflates three concerns:

1. package source resolution (`nixpkgs`, Homebrew formula, Homebrew cask),
2. provider availability (`brew` happens to be on `PATH`), and
3. declaration target selection (which Nix module/list should be edited).

The clearest code smell is `Config::homebrew_file: PathBuf` in `src/config.rs`. Its comment says the Homebrew file is absent on Linux, but the field is unconditional and on Linux is populated with either `nix/modules/nixos/packages.nix` or `configuration.nix`. Callers in package operations then treat that path as a Homebrew module.

Affected operations include at least:

- `src/ops/install.rs`
- `src/ops/remove.rs`
- `src/ops/list.rs`
- `src/ops/profile.rs`
- `src/ops/adopt.rs`
- `src/ops/migrate.rs`
- `src/ops/doctor.rs`

The resulting UX failure is not that Homebrew is present on NixOS. Homebrew may legitimately be layered on Determinate Nix. The failure is that Nex does not know whether Homebrew is an enabled provider for the current profile, and it does not have provider-specific write targets.

## Design principles

1. **Detection is evidence, not authority.** Finding `brew` on `PATH` should not automatically make Homebrew part of `nex install` auto-resolution or a valid mutation target.
2. **Provider enablement is profile/config policy.** Nex should resolve through providers enabled for the active Nex profile/repo, not every tool available on the host.
3. **Declaration targets are provider-specific.** A Nix package target and a Homebrew module target are different destinations. They must not share one ambiguously named `PathBuf`.
4. **Platform supplies defaults, not the whole truth.** macOS commonly enables Nix + Homebrew formulae + casks. NixOS defaults to Nix, but may enable Homebrew formulae explicitly. Casks require a real provider story before they are enabled on Linux.
5. **Short-term safety before schema migration.** First prevent wrong edits and wrong suggestions; then refactor the config model.

## Target behavior matrix

| Host/profile | `nex install foo` | `--nix` | `--brew` | `--cask` |
|---|---|---|---|---|
| macOS + nix-darwin + Homebrew target | Nix + Brew Formula + Cask resolution | Nix target | Brew formula target | Brew cask target |
| NixOS + Determinate Nix only | Nix only | Nix target | error: Homebrew provider not enabled | error: cask provider not enabled |
| NixOS + Determinate Nix + Homebrew formula target | Nix + Brew Formula resolution, policy-ranked | Nix target | Brew formula target | error unless cask target exists |
| Linux + Linuxbrew intentionally enabled | Nix + Brew Formula resolution if configured | Nix target | Brew formula target | disabled by default |

## Provider policy model

Introduce a small policy layer before larger config schema work.

Suggested initial shape:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageProvider {
    Nixpkgs,
    HomebrewFormula,
    HomebrewCask,
}

pub struct ProviderPolicy {
    pub auto_sources: Vec<Source>,
    pub nix_targets: Vec<PathBuf>,
    pub homebrew_formula_target: Option<PathBuf>,
    pub homebrew_cask_target: Option<PathBuf>,
}
```

The first implementation can expose this through helper functions instead of a public config schema migration:

```rust
impl Config {
    pub fn homebrew_formula_target(&self) -> Option<&Path> { ... }
    pub fn homebrew_cask_target(&self) -> Option<&Path> { ... }
    pub fn auto_install_sources(&self) -> Vec<Source> { ... }
}
```

Initial semantics:

- Darwin: Homebrew target is valid if the Darwin Homebrew module path exists or is part of the scaffolded repo shape.
- Linux: Homebrew target is `None` until explicit provider configuration exists.
- Later Linux Homebrew support adds explicit config, not ambient `brew` detection.

## Phase 1 — safety patch

Goal: prevent invalid Linux/NixOS Homebrew behavior without blocking future Homebrew-on-NixOS support.

Scope:

- `src/config.rs`
- `src/ops/install.rs`
- `src/ops/remove.rs`
- `src/ops/list.rs`
- integration tests

Tasks:

1. Add provider target helpers to `Config`:
   - `homebrew_formula_target()`
   - `homebrew_cask_target()`
   - `has_homebrew_provider()` or equivalent
2. On Linux, return `None` for Homebrew targets unless explicit provider configuration exists.
3. In `install`:
   - reject `--brew` if formula target is absent;
   - reject `--cask` if cask target is absent;
   - do not inspect Homebrew lists in duplicate detection if Homebrew target is absent;
   - do not emit `brew not available` warning unless Homebrew is enabled/expected;
   - do not suggest `--cask` for Nix-not-found unless cask provider is enabled.
4. In `remove`:
   - reject explicit `--brew`/`--cask` when corresponding provider target is absent;
   - in auto mode, only search Homebrew lists when targets exist.
5. In `list`:
   - only render Homebrew sections when a Homebrew provider target exists;
   - avoid treating Linux NixOS package files as Homebrew modules.
6. Update integration tests:
   - Linux/default auto install is Nix-only and emits no Brew warning;
   - Linux/default `--brew` and `--cask` fail without edits;
   - Darwin/default behavior keeps current resolution and warning tests.

Acceptance criteria:

- No Linux code path writes Homebrew lists into `configuration.nix` or `nix/modules/nixos/packages.nix` by accident.
- `cargo check` passes.
- Relevant integration tests pass.

## Phase 2 — resolver source filtering

Goal: make package resolution consume provider policy directly.

Scope:

- `src/resolve.rs`
- `src/ops/install.rs`
- tests for resolver behavior

Tasks:

1. Add `resolve_with_sources(pkg, sources)`.
2. Keep `resolve(pkg)` as a compatibility wrapper, or replace call sites in one pass.
3. Only run `brew_*` probes if the corresponding source is allowed.
4. Return whether an allowed provider was skipped because its executable was unavailable, separately from whether Brew exists globally.
5. Make auto mode call resolver with `config.auto_install_sources()`.

Acceptance criteria:

- NixOS auto mode does not call `brew` unless Homebrew is enabled.
- macOS auto mode retains Nix/Brew conflict handling.
- Existing cask redirect logic remains intact for Darwin/Homebrew-enabled profiles.

## Phase 3 — explicit provider configuration

Goal: support intended NixOS + Determinate Nix + Homebrew layering.

Scope:

- `src/config.rs`
- docs
- tests

Suggested config shape:

```pkl
providers {
  homebrew {
    enable = true
    formulaeTarget = "nix/modules/homebrew/packages.nix"
    casks = false
  }
}
```

Compatibility TOML shape if needed:

```toml
[providers.homebrew]
enable = true
formulae_target = "nix/modules/homebrew/packages.nix"
casks = false
```

Tasks:

1. Parse provider configuration in local config.
2. Resolve provider target paths relative to repo unless absolute.
3. Validate target existence or scaffold it through an explicit command.
4. Permit Linux `--brew` when formula target is configured.
5. Keep Linux casks disabled unless `casks = true` and a cask target exists.

Acceptance criteria:

- A NixOS machine can opt into Homebrew formula management without macOS scaffolding.
- Ambient `brew` alone is not enough to mutate config.

## Phase 4 — config schema cleanup

Goal: remove the misleading `homebrew_file` overload.

Scope:

- `src/config.rs`
- all Homebrew callers
- all package operation tests

Tasks:

1. Replace unconditional `homebrew_file: PathBuf` with explicit provider targets:

```rust
pub struct Config {
    pub nix_packages_file: PathBuf,
    pub module_files: Vec<(String, PathBuf)>,
    pub nixos_system_packages_file: Option<PathBuf>,
    pub homebrew_formula_file: Option<PathBuf>,
    pub homebrew_cask_file: Option<PathBuf>,
    ...
}
```

or use a nested provider target struct.

2. Remove all direct uses of the old field.
3. Update docs and CLI help language from macOS-only assumptions to provider-aware language.

Acceptance criteria:

- There is no field named `homebrew_file` that can point to a non-Homebrew file.
- Package operations are provider-target driven.

## Phase 5 — UX/reporting polish

Goal: make provider behavior explainable.

Tasks:

1. Add provider status output to `nex doctor`:
   - Nix provider target(s)
   - Homebrew binary detection
   - Homebrew provider enabled/disabled
   - configured Homebrew target path(s)
2. Add dry-run output that names provider and target:

```text
would add wget to nixpkgs target nix/modules/home/base.nix
would add jq to Homebrew formula target nix/modules/homebrew/packages.nix
```

3. Add a future command or config helper for enabling Homebrew provider explicitly.

## Subagent work split

Useful bounded splits:

1. **Install/remove/list safety patch**
   - Scope: `src/config.rs`, `src/ops/install.rs`, `src/ops/remove.rs`, `src/ops/list.rs`
   - Output: implementation plus focused tests.
2. **Resolver filtering patch**
   - Scope: `src/resolve.rs`, resolver tests, install call sites.
   - Output: `resolve_with_sources` and source-filtered Brew probing.
3. **Integration test/harness platform support**
   - Scope: `tests/integration/**`, possibly `src/discover.rs` only if a test override is needed.
   - Output: deterministic Darwin/Linux provider-policy tests.

Run Phase 1 before Phase 2 if conflicts arise. Phase 3+ should wait until Phase 1/2 are merged and observed on a NixOS host such as `nucleus`.
