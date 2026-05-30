+++
id = "linux-hardware-collector"
kind = "design_node"

[data]
title = "Implement Linux hardware collector"
status = "exploring"
issue_type = "collector"
priority = 2
parent = "nex-hardware-inventory-scan"
dependencies = ["hardware-inventory-schema-v1"]
open_questions = [
  "Which target Linux environments lack `lsblk --json`?",
  "Do we accept an optional `udev` dependency, or keep v1 command/sysfs-only?",
  "Should DMI reads use `/sys/class/dmi/id` directly before adding the `dmidecode` crate?"
]
+++

## Overview

Collect Linux host evidence for the hardware inventory scanner.

## Evidence sources

Primary:

```text
lsblk --json --bytes --output NAME,KNAME,PATH,TYPE,SIZE,MODEL,SERIAL,VENDOR,TRAN,ROTA,RM,HOTPLUG,MOUNTPOINTS,FSTYPE,PKNAME
/sys/class/dmi/id/*
/sys/class/block/*
/sys/block/*/queue/rotational
```

Optional later sources:

- `dmidecode` crate for richer SMBIOS/DMI if `/sys/class/dmi/id` is insufficient.
- `udev` crate for richer device properties if `lsblk`/sysfs fields are insufficient.
- `/sys/bus/pci/devices` plus `pci-ids` for GPU/NIC naming.

## Crate pressure

- `blockdev` may reduce lsblk JSON parsing boilerplate, but should be evaluated against Nex's exact field needs before adding it.
- `udev` brings native libudev expectations; keep it optional or deferred unless it materially improves classification.
- `dmidecode` is useful but may not be needed for v1 if `/sys/class/dmi/id` provides vendor/product/chassis.

## Decisions

- Proposed: v1 Linux collector starts with `lsblk --json` and sysfs only.
- Proposed: missing `lsblk` yields a degraded inventory with warnings rather than a hard failure unless disk classification was explicitly requested.
- Proposed: safety classifier must emit `unknown` when transport/removable/rotational evidence is incomplete.
