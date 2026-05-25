# Nex materialization payloads

Nex materialization payloads are the Pkl-first contract for generating and
checking NixOS materialization workspaces. They sit below machine profiles and
profile fragments:

- `machine-profile.pkl` owns policy, defaults, safety, and dependency intent.
- profile fragments define reusable catalog objects.
- materialization payloads own generated Nix module/flake inputs and local
  evaluation semantics.

Canonical authoring format is Pkl. TOML is compatibility/interchange only.

## Canonical Pkl shape

```pkl
flake_inputs {
  dns_dhcp = "github:styrene-lab/dhcp-dns-work"
  nixos_hardware = "github:NixOS/nixos-hardware"
}
```

The evaluated model is equivalent to:

```json
{
  "flake_inputs": {
    "dns_dhcp": "github:styrene-lab/dhcp-dns-work",
    "nixos_hardware": "github:NixOS/nixos-hardware"
  }
}
```

## Flake input contract

Each `flake_inputs` entry is emitted into generated `flake.nix`:

```nix
inputs = {
  nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  home-manager = {
    url = "github:nix-community/home-manager";
    inputs.nixpkgs.follows = "nixpkgs";
  };
  dns_dhcp.url = "github:styrene-lab/dhcp-dns-work";
};
```

Generated NixOS configurations receive all inputs through `specialArgs`:

```nix
specialArgs = { inherit inputs; username = "..."; hostname = "..."; };
```

This lets generated modules and explicit extra config reference third-party
flakes without `builtins.getFlake`, inline `fetchGit`, or target-time impurity.

## Validation

`nex forge check-materialization --source <payload.pkl> --hostname <host>` must:

1. evaluate the Pkl payload through the shared Nex Pkl evaluator;
2. validate flake input names and references;
3. scaffold a temporary flake workspace;
4. render extra flake inputs into `flake.nix`;
5. evaluate:

```text
.#nixosConfigurations.<host>.config.system.build.toplevel
```

This check is the local pre-validation primitive for issue #5. It should fail
before target install, disk write, or airgap handoff.

## Compatibility TOML

Compatibility TOML uses:

```toml
[flake_inputs]
dns_dhcp = "github:styrene-lab/dhcp-dns-work"
nixos_hardware = "github:NixOS/nixos-hardware"
```

Humans should prefer Pkl. TOML should be treated as generated or legacy
interchange.

## nixosModule export

The 0.19.0 module-export surface is:

```text
nex forge build-module <payload.pkl> --name <name> --output <dir>
```

The command validates the canonical Pkl materialization payload and writes a
small flake exposing:

```nix
nixosModules.<name> = import ./module.nix;
```

This establishes the composable output boundary for issue #5. Later slices can
fill `module.nix` with generated fragment/materialization content.

## Deterministic validation targets

Materialization validation supports explicit targets:

```text
nex forge check-materialization --source payload.pkl --hostname host --target toplevel
nex forge check-materialization --source payload.pkl --hostname host --target sd-image
```

The checker uses deterministic evaluation flags:

```text
nix eval --no-update-lock-file --no-write-lock-file --offline <attr>
```

This intentionally prevents validation from mutating lock files or fetching new
inputs. The flake lock must already contain everything needed for the selected
target. That makes validation predictable enough to gate disk writes, target
installs, and airgap handoff.

## Module payload content

A materialization payload may include NixOS module fragments:

```pkl
nixos_module {
  extra_config = List(
    "services.openssh.enable = true;"
  )
}
```

`extra_config` is rendered into generated `module.nix` and into the temporary
workspace used by `check-materialization`. Impure fetch escape hatches such as
`builtins.getFlake` and `builtins.fetchGit` are rejected; use `flake_inputs`
instead so validation can remain offline and deterministic.

## Deterministic build output

Hermetic materialization builds use the same scaffold and validation path before
building:

```text
nex forge build-materialization payload.pkl --hostname host --target sd-image --output ./dist/host-sd-image
```

The build first runs deterministic validation for the selected target, then runs:

```text
nix build --no-update-lock-file --no-write-lock-file --offline --out-link <output> <attr>
```

For `--target sd-image`, the built attr is:

```text
.#nixosConfigurations.<host>.config.system.build.sdImage
```

The command writes a Nix out-link at `--output`. The flake lock and store must
already contain everything needed; the build will not fetch or rewrite lock
state.
