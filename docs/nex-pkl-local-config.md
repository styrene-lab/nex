---
id: nex-pkl-local-config
title: "Pkl-first local Nex configuration with safe TOML deprecation"
status: exploring
tags: [nex, pkl, config, migration, deprecation]
open_questions:
  - "Should Nex keep writing generated config.toml forever as a compatibility artifact, or stop by default after a deprecation window while keeping `nex config export --format toml`?"
  - "What release window should be guaranteed for TOML read compatibility: one minor release, two minor releases, or until 1.0?"
  - "Should generated config.toml include a warning header, and do any consumers fail on TOML comments?"
dependencies: []
related: []
---

# Pkl-first local Nex configuration with safe TOML deprecation

## Overview

Migrate Nex local user configuration from TOML-first ~/.config/nex/config.toml to canonical Pkl ~/.config/nex/config.pkl without breaking installed users. Existing TOML must remain readable during deprecation. Humans should not be required to hand-edit generated compatibility TOML; Nex should generate TOML from evaluated Pkl when needed for old tooling or rollback.

## Research

### Current TOML-first surface

Current config state in src/config.rs: load_file_config reads ~/.config/nex/config.toml first, then platform legacy dirs::config_dir()/nex/config.toml. set_preference, set_nested_preference, and append_to_list all write config.toml. Config::resolve error text tells users to create config.toml. Callers include init, relocate, install, identity, profile, doctor, polymerize, RBAC, e2e tests, and shell integration tests.

### Acceptance criteria

Acceptance criteria:

1. Existing TOML installs keep working:
   - Given only ~/.config/nex/config.toml exists, when any Nex command resolves config, then it reads the TOML config successfully.
   - Given only platform legacy nex/config.toml exists, when config is resolved, then it remains readable as legacy fallback.

2. Pkl is canonical for new installs:
   - Given nex init creates a local config, then ~/.config/nex/config.pkl is written.
   - Generated config.pkl evaluates through the shared Pkl evaluator into the same normalized FileConfig model used by Config::resolve.

3. Pkl wins when both exist:
   - Given config.pkl and config.toml both exist with different repo_path values, when config is resolved, then config.pkl is used.
   - Nex surfaces a warning/status that config.toml is compatibility output if appropriate.

4. Native TOML export from Pkl:
   - Given config.pkl exists, when Nex runs `nex config export --format toml` or equivalent internal export, then it writes/prints TOML generated from evaluated Pkl.
   - The exported TOML round-trips through the existing TOML FileConfig parser.
   - Export must not parse or transform Pkl source text directly; it must evaluate Pkl to the normalized model, then serialize TOML.

5. Humans do not maintain dual config:
   - Given config.pkl exists, when Nex updates preferences, identity git settings, or SSH labels, then config.pkl remains source of truth and config.toml is regenerated or left absent according to compatibility policy.
   - Nex never asks the user to manually copy keys between config.pkl and config.toml.

6. Non-destructive migration:
   - Given only config.toml exists, when `nex config migrate` runs, then config.pkl is created from the parsed TOML model and the original TOML is preserved as backup or generated compatibility output.
   - Migration is idempotent.
   - Failed migration leaves the original TOML untouched.

7. Atomic writes:
   - All writes to config.pkl and generated config.toml use existing atomic write machinery.

8. Deprecation messaging:
   - CLI/help/error text names config.pkl as canonical and config.toml as compatibility.
   - During the migration window, commands must not fail merely because only config.toml exists.

### Deprecation path

Deprecation path:

Phase 0 — current compatibility baseline:
- config.toml remains readable and writable.
- Add config.pkl read support behind Pkl-first precedence.

Phase 1 — Pkl-first new writes:
- New installs write config.pkl.
- If compatibility output is enabled, Nex also writes generated config.toml from evaluated config.pkl.
- Existing config.toml-only installs continue to work without migration.

Phase 2 — safe migration tooling:
- Add `nex config migrate` to create config.pkl from existing config.toml.
- Preserve original TOML as config.toml.bak or regenerate config.toml with a generated-file header.
- Add `nex config export --format toml` so TOML can always be produced from canonical Pkl.

Phase 3 — warning window:
- If only config.toml exists, commands continue but emit a migration hint in non-scripted contexts.
- No command fails only because the user has TOML.

Phase 4 — TOML as explicit compatibility:
- Stop writing config.toml by default if compatibility output is disabled, but keep read support and explicit TOML export.
- Do not remove TOML read support before a declared major boundary or pre-1.0 policy decision.

## Decisions

### Canonical local config is Pkl; TOML is generated compatibility only

**Status:** proposed

**Rationale:** Nex must be Pkl-first universally, but installed users may already have config.toml. Treating TOML as generated compatibility avoids manual TOML edits and preserves rollback/interoperability.

### Read precedence protects installed TOML users

**Status:** proposed

**Rationale:** Use config.pkl when present; otherwise read config.toml. If both exist, Pkl wins and TOML is considered a generated/exported compatibility artifact.

### Deprecation is phased and non-destructive

**Status:** proposed

**Rationale:** No release should strand users with only TOML installed. Migration should copy/convert first, preserve backups, and only remove automatic TOML writes after multiple releases with explicit messaging.

### Nex generates compatibility TOML from evaluated Pkl

**Status:** proposed

**Rationale:** The lowest-human-error path is to make Pkl the edited source and have Nex export config.toml from the evaluated normalized model. Humans should not hand-maintain dual config files.

## Open Questions

- Should Nex keep writing generated config.toml forever as a compatibility artifact, or stop by default after a deprecation window while keeping `nex config export --format toml`?
- What release window should be guaranteed for TOML read compatibility: one minor release, two minor releases, or until 1.0?
- Should generated config.toml include a warning header, and do any consumers fail on TOML comments?

## Implementation Notes

### Constraints

- Never make an existing config.toml unreadable during the migration window.
- Do not require users to manually edit or reconcile both Pkl and TOML files.
- If config.pkl exists, it is the source of truth; config.toml is generated compatibility output.
- If only config.toml exists, Nex must keep working and should offer/carry out safe conversion to config.pkl.
- Generated TOML must come from the evaluated Pkl normalized model, not from string rewriting Pkl source.
- Writes must be atomic and preserve a rollback path.
