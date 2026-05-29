# Armory Activation — Delta Spec

## ADDED Requirements

### Requirement: Offline Omegon activation lock

Nex shall write an Omegon activation lock containing enough local package information for Omegon to activate without registry access.

#### Scenario: Activation lock contains local paths
Given `profile/rust-shop` has been materialized into the Nex package store
When Nex writes the Omegon activation lock
Then each package entry includes kind, id, version, digest, local path, and activation settings.

#### Scenario: Omegon runtime does not fetch registry
Given a valid activation lock exists
When Omegon starts with that lock
Then Omegon loads package content from local paths
And does not need to query the Armory registry.

### Requirement: Runtime kind separation

Nex shall preserve Armory package kind semantics in activation metadata.

#### Scenario: Profile and machine-profile remain distinct
Given the lock contains `profile/rust-shop` and `machine-profile/styrene.rpi4-kiosk`
When Nex writes activation/runtime metadata
Then `profile` is treated as an Omegon agent profile
And `machine-profile` is treated as Nex machine policy.
