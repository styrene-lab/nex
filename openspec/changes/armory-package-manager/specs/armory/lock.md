# Armory Locking — Delta Spec

## ADDED Requirements

### Requirement: Lock-only install

Nex shall resolve Armory package roots and write deterministic lockfiles before any OCI materialization is implemented.

#### Scenario: Install writes package lock
Given an Armory registry contains `profile/rust-shop` with required dependency `skill/rust`
When the operator runs `nex install profile/rust-shop`
Then Nex resolves both packages
And writes `packages.lock.json` containing exact refs, versions, registry names, OCI refs, digests, and dependency edges.

#### Scenario: Dry run does not write lock
Given an Armory registry contains `profile/rust-shop`
When the operator runs `nex install profile/rust-shop --dry-run`
Then Nex prints the resolved graph
And no lockfile is written.

### Requirement: Dependency graph safety

Nex shall reject invalid dependency graphs.

#### Scenario: Missing required dependency fails
Given `profile/rust-shop` depends on `skill/rust`
And the registry does not contain `skill/rust`
When the operator installs `profile/rust-shop`
Then Nex fails before writing a lockfile
And reports `skill/rust` as missing.

#### Scenario: Cyclic dependencies fail
Given `profile/a` depends on `profile/b`
And `profile/b` depends on `profile/a`
When the operator installs `profile/a`
Then Nex fails before writing a lockfile
And reports the cycle path.

### Requirement: Omegon provisional activation lock

Nex shall write a provisional activation lock for Omegon runtime roots.

#### Scenario: Profile install writes activation lock
Given `profile/rust-shop` resolves successfully
When the operator installs it
Then Nex writes `omegon-activation-lock.json`
And each package entry is marked `pending` until materialized by the store phase.
