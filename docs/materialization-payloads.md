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
