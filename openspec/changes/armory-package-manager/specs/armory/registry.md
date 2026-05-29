# Armory Registry — Delta Spec

## ADDED Requirements

### Requirement: Registry-backed discovery

Nex shall discover public Armory packages from configured registry index URLs.

#### Scenario: Search returns Armory packages
Given a Nex config with an Armory registry URL
When the operator runs `nex search security`
Then Nex queries the registry index
And matching package refs are shown in the results.

### Requirement: Package info rendering

Nex shall render metadata for an Armory package reference.

#### Scenario: Show package info
Given the Armory index contains `profile/rust-shop`
When the operator runs `nex info profile/rust-shop`
Then Nex prints the package name, version, description, dependencies, install command, and activation metadata.
