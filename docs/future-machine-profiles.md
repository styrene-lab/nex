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

## Known physical hardware inventory to model

These are hardware targets already mentioned for eventual Styrene-flavored Nix
coverage. They should become explicit machine-profile/materialization targets as
we formalize physical hardware support.

### `gamingpc`

Custom AMD desktop machine.

Known from repository references:

- existing test/example hostname references use `gamingpc`;
- should probably become the first desktop-class physical-machine profile because
  it is known local hardware and likely easiest to validate.

Initial profile direction:

- `x86_64-linux`
- AMD GPU fragment
- gaming/Steam hardware fragment if applicable
- desktop fragment selected by actual use
- physical-machine attestation required before destructive install

Open questions:

- exact CPU/GPU/motherboard
- disk layout expectations
- Wi-Fi/Bluetooth requirements
- target desktop/session

### Asus Q502A

Older Asus laptop target.

Initial profile direction:

- `x86_64-linux`
- generic laptop power/input profile
- Intel graphics unless hardware inspection says otherwise
- likely lower-risk than T2 MacBook, but still physical-machine destructive ops
  require confirmation and attestation

Open questions:

- exact CPU/GPU/Wi-Fi chipset
- UEFI/BIOS boot quirks
- touchpad/input quirks
- suspend/resume behavior

### Asus T100TA

Older Asus Transformer Book / Bay Trail-class tablet-laptop target.

Initial profile direction:

- likely low-power Intel/Bay Trail hardware
- may need 32-bit UEFI handling despite 64-bit CPU class
- tablet/input/orientation/audio quirks likely matter
- should be treated as a special hardware profile rather than a generic laptop

Open questions:

- exact boot architecture constraints
- kernel/firmware requirements
- internal storage type and installer target
- touchscreen/rotation/audio support

### Raspberry Pi 4B

ARM single-board computer target.

Initial profile direction:

- `aarch64-linux`
- hermetic `sd-image` materialization target
- profile should exercise the 0.19.0 deterministic `sd-image` validation/build
  path
- likely the best first non-x86 hardware image target

Open questions:

- RAM variants in use
- boot from SD vs USB/NVMe
- headless vs desktop
- network provisioning defaults
- whether to use upstream NixOS Raspberry Pi modules, `nixos-hardware`, or a
  Styrene-specific board fragment

### Apple Intel MacBook Pro T2, circa 2020

Tracked above as a high-risk T2 Linux physical-machine target.

## Suggested implementation order

1. Raspberry Pi 4B — best fit for deterministic `sd-image` output and airgap
   artifact testing.
2. `gamingpc` — best x86_64 desktop validation target and likely good for AMD GPU
   / gaming fragments.
3. Asus Q502A — generic-ish older laptop profile after desktop path is stable.
4. Asus T100TA — likely special boot/input quirks; handle after the simpler x86
   laptop path.
5. Intel MacBook Pro T2 — high-risk but well-scoped via T2 Linux; do after the
   physical-machine safety and hardware attestation flow is mature.
