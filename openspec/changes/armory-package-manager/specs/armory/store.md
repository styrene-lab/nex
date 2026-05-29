# Armory Store — Delta Spec

## ADDED Requirements

### Requirement: OCI-backed package materialization

Nex shall fetch OCI-backed Armory packages into a local content-addressed store.

#### Scenario: Package payload fetched and verified
Given an Armory package has `ociRef` and `digest`
When Nex materializes the package
Then the OCI payload is fetched
And the fetched digest matches the registry digest
And the package lock records the local store path.

#### Scenario: Digest mismatch fails closed
Given an Armory package declares digest `sha256:expected`
When the fetched payload digest is `sha256:actual`
Then Nex fails the install
And does not update the lock as verified.

### Requirement: Artifact validation dispatch

Nex shall run existing validators for package kinds it already understands.

#### Scenario: Machine profile package validates
Given a materialized `machine-profile` package
When the package is installed
Then Nex invokes machine-profile validation before marking it installed.

#### Scenario: Materialization payload package validates
Given a materialized `materialization-payload` package
When the package is installed
Then Nex invokes materialization validation before marking it installed.
