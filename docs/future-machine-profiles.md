# Future machine profiles

This is a parking lot for physical hardware targets that should eventually be
covered by Styrene-flavored Nix machine profiles and materialization payloads.

## Goal

Build toward a consistent Styrene-flavored Nix layer that can be applied across
physical hardware with:

- explicit machine-profile policy and safety posture;
- reusable profile fragments;
- deterministic materialization payloads;
- local validation before install, disk write, or airgap handoff;
- hardware-specific attestation where destructive operations are possible.

## Apple Intel MacBook Pro T2, circa 2020

### Intent

Install Linux/NixOS bare metal rather than continuing with macOS/nix-darwin.
This is old enough hardware that full Linux replacement is acceptable if the T2
path is reliable.

### Candidate machine profile

```text
io.styrene.nex.machine-profile.apple.intel-macbookpro-t2-2020
```

Suggested slug:

```text
apple-intel-macbookpro-t2-2020
```

### Safety posture

This should be a high-risk physical-machine profile:

- `default_destructive = true`
- `requires_confirmation = true`
- `requires_target_attestation = true`
- `allowed_targets = ["physical-machine", "vm"]`

Do not treat this as a generic x86 laptop. The T2 chip controls enough of the
boot/input/audio/wireless path that hardware-specific support is mandatory.

### Known Linux path

Research found an active T2 Linux path:

- <https://t2linux.org>
- <https://github.com/t2linux/nixos-t2-iso>

T2 Linux docs list 2020 MacBook Pro models as supported T2 machines. The NixOS
specific project provides installers for T2 Macs.

### Required preinstall constraints

Startup Security Utility must allow non-Apple boot:

- Secure Boot: No Security
- Allow Boot Media: allow external/removable boot

A T2-aware kernel/module stack is required for core hardware:

- keyboard
- trackpad
- touch bar
- audio
- fan
- Wi-Fi

Wi-Fi/Bluetooth firmware handling must be verified before wiping macOS or any
recovery path.

### Open questions

- Exact model identifier: e.g. `MacBookPro16,2`, `MacBookPro16,3`, or
  `MacBookPro16,4`.
- Intel-only graphics or AMD dGPU.
- Exact T2 NixOS flake module output/import path.
- Wi-Fi/Bluetooth firmware extraction path without relying on a preserved macOS
  install.
- Suspend/resume reliability.
- Audio/touchbar acceptability.
- Whether initial install should target external USB/NVMe before internal disk.

### Candidate materialization direction

```pkl
flake_inputs {
  t2_linux = "github:t2linux/nixos-t2-iso"
  nixos_hardware = "github:NixOS/nixos-hardware"
}

nixos_module {
  extra_config = List(
    """
    nixpkgs.hostPlatform = "x86_64-linux";

    # Exact T2 module import needs upstream verification.
    # imports = [ inputs.t2_linux.nixosModules.<module> ];

    boot.loader.systemd-boot.enable = true;
    boot.loader.efi.canTouchEfiVariables = true;

    networking.networkmanager.enable = true;
    hardware.graphics.enable = true;

    services.xserver.enable = true;
    services.displayManager.gdm.enable = true;
    services.desktopManager.gnome.enable = true;

    services.power-profiles-daemon.enable = true;
    """
  )
}
```

Before physical install, require deterministic validation:

```text
nex forge check-materialization \
  --source apple-intel-macbookpro-t2.pkl \
  --hostname <host> \
  --target toplevel
```

For image workflows:

```text
nex forge check-materialization \
  --source apple-intel-macbookpro-t2.pkl \
  --hostname <host> \
  --target sd-image
```
