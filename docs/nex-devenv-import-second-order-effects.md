+++
id = "nex-devenv-import-second-order-effects"
kind = "design_node"

[data]
title = "Second-order effects of devenv compatibility"
status = "exploring"
issue_type = "risk-analysis"
priority = 2
parent = "nex-devenv-import-migration"
dependencies = []
open_questions = [
  "How much migration review state must persist in generated artifacts?",
  "Should Nex refuse to apply migrated profiles with unresolved `requiresReview` items?",
  "Can UI flows make project-scoped vs machine-scoped distinctions obvious enough?"
]
+++

# Second-order effects of devenv compatibility

## Overview

Consuming devenv projects creates value, but also creates product and safety risks. This node captures the second-order effects so implementation does not accidentally turn Nex into an unsafe devenv clone or a misleading migration wizard.

## Risk: scope confusion

Users may assume Nex can replace devenv for day-to-day project shell workflows.

Actual desired posture:

- Nex can import and understand devenv project environments.
- Nex can containerize/project-scope some devenv content.
- Nex remains machine/profile lifecycle infrastructure.

Mitigation:

- Use buckets: `portable`, `projectScoped`, `machineScopedCandidate`, `requiresReview`, `unsupported`.
- UI copy must say "project-scoped" when an item is not a machine profile feature.
- `nex devenv migrate` should not imply automatic machine promotion.

## Risk: service lifecycle escalation

A devenv service is usually local/dev and ephemeral. A Nex machine service may be persistent, exposed on boot, and security-sensitive.

Examples:

- Postgres in devenv may be local-only dev state; as a NixOS service it needs persistence, backups, auth, firewall posture.
- Redis in devenv may be unauthenticated localhost only; as a system service it may become remotely reachable if misconfigured.
- Mailpit/httpbin/wiremock are dev test services, not production services.

Mitigation:

- Default services to project/container scoped.
- Promotion to machine service requires explicit target and explanation.
- `profile explain` must show service lifecycle and exposure.

## Risk: arbitrary command laundering

Tasks/processes/enterShell can contain arbitrary commands. Migration could accidentally launder them into trusted Nex task graphs.

Mitigation:

- Preserve arbitrary commands only as `requiresReview` unless classified by known-safe patterns.
- Do not run tasks during inspect/migrate-plan.
- Generated profile tests should not execute imported arbitrary tasks by default.

## Risk: secret value leakage

Devenv projects may use `.env`, dotenv integration, or SecretSpec. Migration must not copy secret values into generated Pkl/Nix/profile artifacts.

Mitigation:

- Import SecretSpec contracts only.
- Treat `.env` as provider hint/local value source, not artifact content.
- Redact any accidentally discovered values in reports.
- Use `nex secrets check/run` for runtime resolution.

## Risk: Nix evaluation trust boundary

A devenv project is arbitrary Nix. Evaluating it can require network, local files, flake inputs, overlays, and potentially impure assumptions.

Mitigation:

- Static inspect by default.
- Evaluated metadata is opt-in until a safe stable devenv metadata interface is proven.
- Preserve `devenv.lock` provenance and warn when lock cannot be honored.

## Risk: generated artifacts rot after migration

If a project keeps evolving its devenv files after migration, Nex artifacts may drift.

Mitigation:

- Store source file hashes in migration report.
- Add `nex devenv compare <path> --profile <generated>` later.
- Warn when source lock/hash has changed.

## Risk: UI overconfidence

A UI can make migration appear safer than it is by hiding review buckets behind green checkmarks.

Mitigation:

- UI must show unresolved `requiresReview` count prominently.
- Applying generated profiles with unresolved review items should be blocked or require explicit override.
- Safety taxonomy should drive confirmation language.

## Risk: dependency on devenv CLI stability

If Nex relies on `devenv info` output and that output changes, importer breaks.

Mitigation:

- Version imported report schema separately.
- Keep static mode functional without devenv CLI.
- Treat evaluated mode as enrichment.

## Decisions

- Proposed: generated migrated profiles retain migration report and source hashes.
- Proposed: profile apply refuses unresolved `requiresReview` items by default.
- Proposed: project-scoped items are never applied as machine state unless explicitly promoted.
