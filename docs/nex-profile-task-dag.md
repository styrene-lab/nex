+++
id = "nex-profile-task-dag"
kind = "design_node"

[data]
title = "Introduce a profile lifecycle task DAG"
status = "exploring"
issue_type = "architecture"
priority = 2
parent = "nex-devenv-parallels"
dependencies = []
open_questions = [
  "Should the task DAG be an internal library only or an operator-visible `nex tasks` surface?",
  "Do profile tasks need readiness states like devenv processes, or only success/failure?",
  "How should long-running build/process tasks stream progress into Forge/Nex event logs?"
]
+++

## Overview

Devenv tasks form a DAG with dependency states. Nex can use the same concept to avoid one-off orchestration across profile, hardware, secrets, forge, and materialization commands.

## Candidate internal DAG

```text
hardware:scan
profile:evaluate
profile:validate
profile:resolve-imports
profile:explain
secrets:check
forge:plan
forge:preflight
materialization:check
materialization:build
apply:confirm
apply:execute
```

Edges:

```text
profile:evaluate -> profile:validate -> profile:resolve-imports
hardware:scan -> hardware:match
profile:resolve-imports -> hardware:match
profile:resolve-imports -> secrets:check
hardware:match -> forge:plan
secrets:check -> forge:plan
forge:plan -> forge:preflight -> materialization:build -> apply:confirm -> apply:execute
```

## Operator-facing possibilities

```text
nex tasks list <profile>
nex tasks graph <profile>
nex tasks run <profile> profile:test
```

But first implementation can keep the DAG internal and expose only:

```text
nex profile test
nex profile apply
```

## Why this matters

Without a DAG, each command will reinvent:

- prerequisite ordering
- partial failure reporting
- event/log streaming
- skipped/degraded checks
- expensive step gating
- dry-run behavior

## Decisions

- Proposed: start with an internal `CheckGraph`/`TaskGraph` report structure, not a public `nex tasks` command.
- Proposed: model task status as `pending`, `running`, `passed`, `warning`, `blocked`, `skipped`.
- Proposed: reuse Forge event/report diagnostics where possible.
