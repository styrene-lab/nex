+++
id = "bundle-pkl-evaluator-for-nex-releases"
kind = "design_node"

[data]
title = "Bundle a private Pkl evaluator with Nex release artifacts"
status = "decided"
issue_type = "runtime-dependency"
priority = 1
parent = "nixos-curl-install-pkl-runtime"
dependencies = ["devenv-surface-awareness-catalog"]
open_questions = []
+++

## Overview

Nex uses Pkl as its canonical definition language. Release tarballs should carry a Nex-private Pkl evaluator so users do not need to install or align a global `pkl` binary.

This is not a global Pkl install. The bundled evaluator is scoped to Nex's release artifact and is only used by Nex.

## Decision

Bundle Pkl under:

```text
libexec/nex/pkl
```

Evaluator discovery order:

1. `NEX_PKL`
2. bundled `libexec/nex/pkl`
3. ambient `pkl`
4. `nix shell nixpkgs#pkl -c pkl`

Rationale:

- `NEX_PKL` remains an explicit operator override.
- Bundled Pkl avoids ambient version drift and missing runtime dependencies.
- Ambient `pkl` remains useful for dev builds.
- Nix fallback preserves previous behavior for source/dev installs.

## License posture

Pkl is Apache-2.0. Release artifacts must include upstream license and third-party notices.

Required artifact layout:

```text
bin/nex
libexec/nex/pkl
share/doc/nex/third-party/pkl/LICENSE.txt
share/doc/nex/third-party/pkl/THIRD-PARTY-NOTICES.txt
```

## Release implementation

Release workflow downloads the platform Pkl binary matching each Nex target:

```text
aarch64-apple-darwin      -> pkl-macos-aarch64
x86_64-apple-darwin       -> pkl-macos-amd64
x86_64-unknown-linux-gnu  -> pkl-linux-amd64
aarch64-unknown-linux-gnu -> pkl-linux-aarch64
```

Pkl version is pinned in the workflow.

## Non-goals

- Do not install Pkl globally.
- Do not overwrite user `pkl`.
- Do not silently download Pkl at runtime.
- Do not make bundled Pkl unoverrideable.
