# RFC 0001 — Universal Keybinds

- **Status:** Draft
- **Author:** Chris Wilson
- **Date:** 2026-04-29
- **Touches:** `nex` core (profile schema + writers), `nex-personal`, `nex-gamingpc`

## Goal

One `[[keybinds]]` table in a profile produces identical muscle memory on every workstation, regardless of OS. The user thinks in **semantic actions** ("take a region screenshot to clipboard"), not in OS-flavored shortcuts ("Cmd+Shift+4 vs Super+Shift+S"). nex compiles that table to whatever each platform actually uses (skhd on macOS, COSMIC RON on NixOS, etc.).

## Non-goals

- Respecting platform-default shortcuts. Convention loses to consistency on purpose.
- Per-app keybindings (Slack, browser, IDE). Out of scope; those tools own their own config.
- Replacing per-app remappers (Karabiner complex modifications). nex defers to them when present.

## Why this is hard

Two problems get conflated; we separate them:

1. **Naming.** `Cmd` (mac) and `Super` (linux) are physically the same key on most desks (left-of-spacebar diamond), but every config format spells it differently.
2. **Implementation.** macOS has no first-class global-hotkey config; users daemonize `skhd` or `Karabiner`. COSMIC writes RON at `~/.config/cosmic/com.system76.CosmicSettings.Shortcuts/v1/custom`. GNOME uses `gsettings`. KDE uses `kwriteconfig5`. Each writer is bespoke.

The author writes once; nex translates.

## Design

### 1. Canonical key vocabulary

nex defines a single key/modifier namespace. Authors use only this:

| Modifier | macOS maps to | Linux maps to |
|---|---|---|
| `Mod`   | `cmd`     | `super` |
| `Alt`   | `option`  | `alt`   |
| `Shift` | `shift`   | `shift` |
| `Ctrl`  | `control` | `ctrl`  |
| `Hyper` | `cmd+ctrl+option+shift` | `super+ctrl+alt+shift` |

Keys: `A-Z`, `0-9`, `F1-F24`, `Space`, `Tab`, `Enter`, `Escape`, `Backspace`, `Delete`, `Left`, `Right`, `Up`, `Down`, `Home`, `End`, `PageUp`, `PageDown`, `Print`, `Comma`, `Period`, `Slash`, `Semicolon`, `Quote`, `Backtick`, `Minus`, `Equal`, `LeftBracket`, `RightBracket`, `Backslash`.

Combos use `+`: `Mod+Shift+S`, `Hyper+T`, `Ctrl+Alt+Comma`. Case-insensitive on input, normalized to PascalCase on store.

### 2. Action registry

nex ships a registry of well-known actions with built-in implementations per platform. Authors reference an action by dotted name; nex emits the right command.

Initial registry:

| Action | macOS command | Linux/COSMIC command |
|---|---|---|
| `screenshot.region.clipboard` | `screencapture -i -c` | `grim -g "$(slurp)" - \| wl-copy` |
| `screenshot.region.file` | `screencapture -i ~/Desktop/Screenshots/$(date +%F-%H%M%S).png` | `grim -g "$(slurp)" ~/Pictures/Screenshots/$(date +%F-%H%M%S).png` |
| `screenshot.full.clipboard` | `screencapture -c` | `grim - \| wl-copy` |
| `terminal.open` | `open -a kitty` | `kitty &` |
| `browser.open` | `open -a Safari` | `xdg-open https://` |
| `clipboard.history` | (Raycast/Maccy if installed) | `cliphist list \| wofi --show dmenu \| cliphist decode \| wl-copy` |
| `window.close` | (delegated; see §5) | (delegated) |
| `workspace.next` / `workspace.prev` | (mac doesn't really have these) | COSMIC native |

Registry lives in `nex/src/keybinds/registry.rs`. Each entry: `id`, `mac_command: Option<&str>`, `linux_command: Option<&str>`, `notes`. Missing platform → emit a warning during apply, skip on that platform.

### 3. Profile schema

```toml
# Use a built-in action — nex picks the command per platform.
[[keybinds]]
keys = "Mod+Shift+S"
action = "screenshot.region.clipboard"

# Custom command — overrides registry, or for actions that aren't registered.
[[keybinds]]
keys = "Hyper+T"
command = "kitty"          # used on both platforms

# Per-platform command overrides.
[[keybinds]]
keys = "Mod+Comma"
action = "settings.open"
mac.command = "open -a 'System Settings'"
linux.command = "cosmic-settings"

# Skip on a platform.
[[keybinds]]
keys = "Mod+Space"
action = "spotlight"
linux.skip = true   # use rofi/wofi via a different binding instead
```

Resolution order per (platform, keys) pair:
1. `<platform>.command` if present.
2. Top-level `command` if present.
3. Registry lookup by `action`.
4. If none resolve → error at `nex profile apply` time, do not write a partial config.

### 4. Inheritance & override

`[[keybinds]]` is **keyed by `keys`**, not array-appended. A child profile redefining `Mod+Shift+S` replaces the parent entry entirely. To remove a parent binding without replacing it: `{ keys = "Mod+Shift+S", remove = true }`.

This matters because `nex-gamingpc` extending `nex-personal` should be able to *override* a personal binding for one machine without the parent's version also firing.

### 5. Per-platform writers

#### macOS — `skhd`

- nex declares `skhd` as a brew dep when any `[[keybinds]]` table exists.
- nex writes `~/.config/skhd/skhdrc` from the resolved binding set.
- nex installs a launchd plist (`brew services start skhd` equivalent) idempotently.
- Window-management actions (`window.close`, `workspace.next`) are explicitly **out of scope for v1** on macOS — these need yabai/AeroSpace. v1 supports launchers and shell commands only.

#### Linux — COSMIC

- Writer target: `~/.config/cosmic/com.system76.CosmicSettings.Shortcuts/v1/custom`.
- Format: RON. Each entry: `(modifiers: [Super, Shift], key: Some("S"), description: Some("..."), action: Spawn("grim -g $(slurp) - | wl-copy"))`.
- nex never edits this file by hand; it owns the file (with a `# managed by nex` header in a sibling `.nex-managed` marker, since RON has no comment-and-survive convention everywhere). On apply, write atomically: temp file + rename.
- Native COSMIC actions (workspace switching, window close) emit `action: System(...)` instead of `Spawn(...)` — registry entry knows which.

#### Linux — GNOME / KDE (future)

Out of scope for v1. Schema is forward-compatible: add `[linux.gnome]` / `[linux.kde]` blocks per binding when needed.

### 6. Doctor checks

`nex doctor` adds:

- macOS: is `skhd` running? Does Accessibility permission cover it?
- Linux/COSMIC: does the target dir exist? Is the user in the right session (Wayland)?
- Both: any `action` references missing from the registry?
- Both: any conflicting `keys` across the merged profile chain?

### 7. CLI surface

- `nex keybinds list` — print the resolved table for the current platform, with command for each.
- `nex keybinds doctor` — alias for the relevant `nex doctor` subset.
- `nex keybinds apply` — re-emit just the keybind config without rerunning a full `profile apply`.

## Migration plan

1. **`nex` core** — add `Keybind`, `KeybindAction`, registry module, two writers (`skhd`, `cosmic-ron`). Wire into `apply_macos` and `apply_linux`.
2. **`styrene-lab/nex-profiles`** — leave alone; this is the public base and shouldn't carry personal bindings.
3. **`cwilson613/nex-personal`** — add the universal `[[keybinds]]` table. Move screenshot, terminal, browser bindings here.
4. **`cwilson613/nex-gamingpc`** — change `extends` from `styrene-lab/nex-profiles` → `cwilson613/nex-personal`. Add only gaming-PC-specific overrides (e.g. `Mod+Shift+G` to launch Steam).

## Open questions

- **Should `Hyper` be opt-in?** It requires an OS-level remap (Caps Lock → Hyper) on both platforms. Probably gate behind `[keybinds.hyper] enable = true` so authors who don't have it remapped don't get silently broken bindings.
- **Conflict detection across non-nex configs.** If the user has hand-edited their COSMIC shortcuts file, do we warn, merge, or overwrite? v1 proposal: overwrite with backup to `.cosmic-shortcuts.bak.<timestamp>`, warn if backup differs from prior nex output.
- **Per-app bindings.** Tempting to add `app = "kitty"` filter. Defer to v2 — most per-app needs are better served by the app's own config (which nex already templates for kitty).
- **Karabiner interop.** If a user has Karabiner installed with complex modifications, our skhd bindings may collide. v1: document that nex assumes skhd-only on macOS. v2: detect Karabiner and emit complex-mods JSON instead.

## Rollout

- Land core + skhd + cosmic-ron writers behind `nex keybinds` opt-in subcommand first; do not run on `profile apply` until validated on Chris's two machines.
- After two weeks of use without surprises, fold into `profile apply`.
