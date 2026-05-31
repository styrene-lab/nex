+++
id = "nex-secretspec-integration"
kind = "design_node"

[data]
title = "Adopt SecretSpec-style secrets contracts for Nex profiles"
status = "exploring"
issue_type = "architecture"
priority = 2
parent = "nex-devenv-parallels"
dependencies = []
open_questions = [
  "Should Nex call the upstream `secretspec` CLI/SDK directly or define a compatible Pkl-native schema first?",
  "Should secret values ever enter Nex profile evaluation, or only runtime process/apply phases?",
  "Which providers must Nex support initially: keyring, env, dotenv, 1Password, OpenBao/Vault?",
  "How should secrets be passed into generated NixOS/nix-darwin modules without leaking into the Nix store?"
]
+++

## Overview

Devenv integrates SecretSpec, which separates secret declaration from secret provisioning. Nex can use the same pattern for machine profiles: profiles declare which secrets they need, while each machine/operator/CI environment decides where those secrets come from.

## SecretSpec research summary

SecretSpec splits secrets into three concerns:

- **WHAT**: which secrets are needed (`DATABASE_URL`, `API_KEY`, `WIREGUARD_PRIVATE_KEY`)
- **HOW**: requirements, defaults, validation, generation, optional/required
- **WHERE**: provider (`keyring`, `dotenv`, `env`, `1password`, `lastpass`, `openbao`, etc.)

Devenv's best-practice guidance is important:

- prefer runtime loading via `secretspec run -- command`
- avoid dumping secrets into the whole shell environment
- expose secrets only to processes that need them
- keep secrets out of Nix evaluation whenever possible

SecretSpec also supports declarative generation:

```toml
[profiles.default]
DB_PASSWORD = { description = "Database password", type = "password", generate = true }
API_TOKEN = { description = "Internal token", type = "hex", generate = { bytes = 32 } }
SESSION_KEY = { description = "Session signing key", type = "base64", generate = { bytes = 64 } }
```

Generation is idempotent: generate if missing, never overwrite.

## Nex use cases

Nex profiles need secrets for:

- SSH authorized key material or deploy keys
- WireGuard private keys / preshared keys
- binary cache tokens
- GitHub/GitLab/Forge tokens
- SOPS/age identities if a profile chooses encrypted repo material
- service credentials for self-hosted services
- Wi-Fi credentials for install media / target systems
- enrollment tokens for mesh/VPN/MDM-like flows
- local-only generated passwords for databases/services

## Design principle

Profiles should declare secret contracts, not secret values.

Bad:

```pkl
services.foo.password = "super-secret"
```

Good:

```pkl
secrets.required = List("FOO_PASSWORD")
services.foo.passwordSecret = "FOO_PASSWORD"
```

Provider binding should be local/operator-specific:

```text
nex secrets check --profile gaming --provider keyring
nex secrets run --profile gaming -- command
nex profile apply gaming --secrets-provider keyring
```

## Candidate Nex surfaces

```text
nex secrets check <profile>
nex secrets list <profile>
nex secrets generate <profile>
nex secrets run <profile> -- <command>
nex profile test <profile> --secrets-provider keyring
nex profile apply <profile> --secrets-provider keyring
```

For machine install flows:

```text
nex forge plan --request request.pkl --secrets-profile default
nex forge build-materialization payload.pkl --secrets-profile default
```

## Candidate profile schema

```pkl
secrets {
  profile = "default"

  required = new Mapping {
    ["CACHE_TOKEN"] = new SecretRequirement {
      description = "Binary cache upload token"
      required = true
      providerHint = "keyring"
    }
    ["WG_PRIVATE_KEY"] = new SecretRequirement {
      description = "WireGuard private key"
      required = true
      type = "command"
      generateCommand = "wg genkey"
    }
  }
}
```

Alternative: consume upstream `secretspec.toml` directly and only reference secret names from Pkl profile artifacts.

## Nix-store leakage constraint

This is the hardest part. Nex must avoid putting secret values into derivations, generated Nix files, build logs, or `/nix/store`.

Rules:

- Secret contracts may be evaluated in Pkl/Nix.
- Secret values must not be evaluated by Nix unless the value is explicitly public.
- Generated NixOS/nix-darwin modules should reference runtime files/environment mechanisms, not inline values.
- For services, prefer runtime secret files with strict permissions or systemd credential mechanisms where available.
- For commands, prefer `secretspec run -- <command>`-style execution.

## Relationship to existing Nex identity/secrets tools

Nex already has Styrene identity and Git credential surfaces. SecretSpec-style contracts should compose with, not replace, those:

- Styrene identity can authenticate/sign profile artifacts.
- SecretSpec-like providers supply runtime secret values.
- Nex validates that required secrets exist before applying a profile.

## Decisions

- Proposed: adopt the SecretSpec separation (`what/how/where`) as a Nex design principle.
- Proposed: first implementation should validate secret contracts and report missing secrets; do not inject secrets into materialization yet.
- Proposed: runtime secret loading should be provider-driven and least-privilege; avoid global shell export by default.
- Proposed: local-only generated secrets are high value for onboarding and should be supported after read/check flows.
