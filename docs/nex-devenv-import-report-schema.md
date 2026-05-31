+++
id = "nex-devenv-import-report-schema"
kind = "design_node"

[data]
title = "Define devenv import report schema"
status = "exploring"
issue_type = "schema"
priority = 2
parent = "nex-devenv-import-migration"
dependencies = []
open_questions = [
  "Should report item `source` point to line numbers when static parser can discover them?",
  "Should shell command bodies be stored verbatim, hashed, or summarized by default?",
  "How should UI acknowledge unresolved review items in the report schema?"
]
+++

# Define devenv import report schema

## Overview

`io.styrene.nex.devenv-import-report.v1` is the stable report consumed by CLI, UI, migration generation, and profile explain/test flows.

The report must support partial/static imports and richer/evaluated imports without changing the top-level model.

## Top-level schema

```json
{
  "schema": "io.styrene.nex.devenv-import-report.v1",
  "root": "/workspace/app",
  "mode": "static",
  "devenvVersion": null,
  "detected": {
    "devenvNix": true,
    "devenvYaml": true,
    "devenvLock": true,
    "devenvLocalNix": false,
    "devenvLocalYaml": false,
    "envrc": true,
    "secretspecToml": true
  },
  "sourceHashes": {},
  "items": [],
  "summary": {
    "portable": 3,
    "projectScoped": 2,
    "machineScopedCandidate": 1,
    "requiresReview": 2,
    "unsupported": 0
  },
  "warnings": []
}
```

## Item schema

```json
{
  "id": "packages.git",
  "kind": "package",
  "bucket": "portable",
  "safety": ["build"],
  "source": {
    "file": "devenv.nix",
    "path": "packages[0]",
    "line": null
  },
  "devenv": {
    "option": "packages",
    "valueSummary": "pkgs.git"
  },
  "nexCandidate": {
    "target": "profile.packages",
    "valueSummary": "git"
  },
  "review": {
    "required": false,
    "reason": null,
    "resolved": false
  },
  "messages": []
}
```

## Buckets

```text
portable
projectScoped
machineScopedCandidate
requiresReview
unsupported
```

## Item kinds

```text
package
language
service
process
task
shell-hook
test
output
container
secret-contract
dotenv-provider
git-hook
overlay
import
binary-cache
unknown
```

## Safety tags

Reuse command-surface taxonomy:

```text
read-only
local-file-write
network-read
network-write
build
user-config-mutation
system-config-mutation
privileged-mutation
hardware-driver-mutation
destructive-disk-operation
secret-contract
secret-value-runtime
identity-signing
arbitrary-command
```

## Review state

Review fields allow migration UI to persist acknowledgement:

```json
{
  "required": true,
  "reason": "enterShell contains arbitrary shell code",
  "resolved": false,
  "resolution": null
}
```

Possible resolutions:

```text
keep-project-scoped
promote-to-machine
preserve-as-task
drop
unsupported-acknowledged
```

## Privacy defaults

- Secret values must never appear.
- Shell command bodies may be sensitive; initial implementation should store summaries and hashes, not full bodies, unless `--include-source-snippets` is requested.
- Source hashes help detect drift without storing all content.

## Decisions

- Proposed: use a single `items` array with bucket/kind/safety rather than separate top-level arrays, while summary counts provide quick UI grouping.
- Proposed: static import can emit lower-fidelity items; evaluated import can enrich same item model.
- Proposed: report schema should be saved alongside generated migration artifacts and used by `profile explain`.
