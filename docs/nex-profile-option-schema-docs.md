+++
id = "nex-profile-option-schema-docs"
kind = "design_node"

[data]
title = "Generate profile option schema, docs, and search"
status = "exploring"
issue_type = "developer-experience"
priority = 2
parent = "nex-devenv-parallels"
dependencies = []
open_questions = [
  "Should option metadata be authored in Pkl, Rust, or generated from Rust types?",
  "How do profile fragment categories map to option paths?",
  "Can option docs also drive Flynt/UI forms and AI tool context?"
]
+++

## Overview

Devenv's option search/docs are a major discoverability advantage. Nex should define machine-profile options as structured metadata and generate docs, CLI search, validation hints, and UI/AI affordances from that metadata.

## Candidate commands

```text
nex profile options search gpu
nex profile options show hardware.gpu.amd
nex profile options list --category hardware
```

## Option metadata shape

```json
{
  "path": "hardware.gpu.amd.enable",
  "type": "bool",
  "default": false,
  "description": "Enable AMD GPU driver/profile support.",
  "category": "hardware",
  "safety": {
    "mutatesHardwareDrivers": true,
    "requiresConfirmation": true
  },
  "examples": ["hardware.gpu.amd.enable = true"]
}
```

## Uses

- CLI search/show
- generated docs
- profile validation errors with remediation
- interactive profile wizard
- Flynt forms
- MCP/AI tool context
- compatibility checks between fragments

## Decisions

- Proposed: start with Rust-authored metadata for existing profile fragment categories, then consider Pkl-native metadata when schemas stabilize.
- Proposed: every safety-sensitive option must declare safety metadata.
- Proposed: generated docs should link options to profile fragments and implementation modules.
