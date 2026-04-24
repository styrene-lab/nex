# The Roast, Round 2

Second adversarial assessment of nex (v0.13.1), performed 2026-04-24. Round 1 fixed fsync, backup verification, self-update checksums, git commit surfacing, semver, and a dozen other things. Here's what's still broken.

---

## 1. parse_item() Eats All Your Quotes

`nixfile.rs:33`: `trimmed.trim_start_matches('"')` removes ALL leading quote characters, not just the first one. For the line `"""pkg"""`, it strips to `pkg"""`, then `find('"')` returns index 0, yielding `Some("")` -- an empty string. Meanwhile `validate_pkg_name` allows periods but `parse_item` rejects them (line 45: `word.contains('.')`), so you can `insert("python3.11")` but `parse_item` won't recognize it on the next call. Duplicate detection is broken for any package with a dot. Insert it twice, get two entries, corrupt your config.

## 2. Checksum Verification Is Optional (Downgrade Attack)

`self_update.rs:63-68`: If the `checksums.sha256` file download fails -- network error, firewall, or an attacker selectively blocking it -- the code prints a warning and proceeds without verification. An attacker performing a MITM can serve a backdoored tarball and simply block the checksum URL. The user sees "warning: checksum file not available" and installs the compromised binary anyway. Checksum verification that's optional is checksum theater.

## 3. shasum Doesn't Exist on Half of Linux

`self_update.rs:216`: `Command::new("shasum")`. The `shasum` binary is a Perl script that ships with macOS and some Linux distros. Alpine, NixOS minimal, most Docker images, and musl-based systems have `sha256sum` (GNU coreutils) instead. On those systems, `self-update` fails entirely -- not with a graceful skip, but a hard error from `.context("failed to run shasum")?`. You added checksum verification that breaks the update flow on the very Linux systems nex targets.

## 4. install.sh Has Zero Integrity Verification

`site/public/install.sh:91`: Downloads the binary with `curl -fsSL`, extracts it, moves it into PATH. No checksum. No signature. Nothing. The release workflow now generates `checksums.sha256` and attaches it to GitHub releases, but the install script never fetches or checks it. You hardened `self-update` but left the front door wide open for first-time installs. Every `curl | sh` invocation is a trust-me-bro.

## 5. The Release Workflow Uses Floating Action Tags

`.github/workflows/release.yml`: Every action is pinned to a major version tag -- `actions/checkout@v4`, `actions/upload-artifact@v4`, `dtolnay/rust-toolchain@stable`. These resolve to whatever the latest minor/patch is at build time. A compromised `v4` tag (or a breaking change in `dtolnay/rust-toolchain`) could inject malicious code into your release pipeline. The standard practice for release workflows is to pin to full commit SHAs.

## 6. validate_pkg_name Is Only Called on Insert, Not Remove

`edit.rs:79` calls `validate_pkg_name()`. `edit.rs:105` (remove) does not. The validation is asymmetric. This isn't exploitable today because `parse_item` does its own filtering, but it's a latent gap -- if someone manually adds a sketchy entry to the nix file and later a code path tries to remove by name with unsanitized input, there's no defense. Consistency matters in security boundaries.

## 7. profile.rs Is a Silent No-Op Factory

The profile apply code has at least 5 patterns where a string replacement can fail to match and the code writes the file unchanged with no error:

- Line ~911: `content.replace("    brews = [", ...)` -- if indentation differs, silent no-op
- Line ~1118: `content.replace("imports = [", ...)` -- if no imports list, silent no-op
- Line ~1120: `content.replace("{\n", ...)` -- if file uses `{\r\n` or `{ #comment`, silent no-op
- Line ~2048: `content.replace("../../modules/nixos/base.nix", ...)` -- if path differs, silent no-op
- Line ~2062: `if let Some(brace_pos) = content.find('{')` -- if no brace, entire block skipped silently

Each one writes the original content back to disk via `atomic_write_bytes`, reports success, and moves on. The user runs `nex profile apply`, sees green checkmarks, and their shell config / desktop module / homebrew taps were never actually wired up. The only way they find out is when things don't work.

## 8. set_preference Is Still a Hand-Rolled TOML Mutator

`config.rs:155-173`: The code splits lines on `=`, checks if the key matches, and does string replacement. This breaks on:

- TOML tables: `[section]\nhostname = "old"` -- matches `hostname` regardless of section
- Quoted values with `=`: `packages = ["foo=bar"]` -- splits on the wrong `=`
- Multi-line values -- silently corrupts them
- Inline comments -- silently strips them

The `toml` crate is already a dependency. Parse, modify the struct, serialize back.

## 9. init.rs Scaffolds 11 Files with std::fs::write

`init.rs` lines 584-1028: Every scaffolded nix file -- `flake.nix`, `base.nix`, `homebrew.nix`, `mkHost.nix`, host configs -- is written with bare `std::fs::write`. These are in the user's config repo. If `nex init` is interrupted (Ctrl-C, power loss, disk full) mid-scaffold, you get a partially written `flake.nix` that nix can't parse. The atomic write utility exists 200 lines away in `edit.rs`. It's not used here.

The defense is "these are new files being created, not existing files being overwritten" -- which is mostly true for scaffold, but `nex init` also detects and adopts existing repos, where it CAN overwrite.

## 10. doctor.rs Patches Nix Files with Brittle Multi-Line String Replacement

`doctor.rs:64-96`: The mac-app-util patching does a `.replace()` on a multi-line string that includes exact whitespace, exact newline positions, and exact content ordering. If the user reformatted their `flake.nix` (ran `nixfmt`, changed indentation, added a comment between inputs), none of the patterns match and the patch silently fails. The validation at lines 99-109 checks for keyword presence but not structural correctness -- a half-applied patch that contains the right keywords but in the wrong places will pass validation and produce a broken flake.

## 11. find_nix() Trusts Whatever "nix" Is on PATH

`exec.rs:10-12`: `Command::new("nix").arg("--version")` runs whatever binary named `nix` is first on PATH. In CI, containers, or compromised environments, this could be a malicious binary. The code does fall back to hardcoded paths (`/nix/var/nix/profiles/default/bin/nix`), but only when the PATH lookup fails -- if an attacker's `nix` binary succeeds the version check, it's used for all subsequent operations including `nix eval`, `nix build`, and `nix shell`.

## 12. polymerize Partition Sequence Has No Readiness Checks

`polymerize.rs:787-830`: After `parted` repartitions, there's a 1-second `sleep` and then `mkfs.ext4` runs on the new partition. No verification that the partition device node actually exists. If the kernel hasn't finished updating device nodes (common on slower hardware or VMs), `mkfs` writes to a stale or nonexistent device. The `umount -R /mnt` before mounting is still `let _ =` -- if it fails, stale mounts interfere with the fresh mount sequence. You're formatting and installing an OS to disk with sleep-and-pray synchronization.

---

## The Improvement

The v0.13.1 fixes were real. The fsync-before-rename closes the power-loss window. The semver comparison actually works. The git commit helper surfaces failures that were invisible before. The backup verification catches a genuine silent-data-loss path. The mutual exclusivity on CLI flags is correct.

But the pattern is: the core install/remove/revert loop is solid, and everything around it is held together with string matching and optimism. The profile system, the doctor, the init scaffold, the polymerize installer -- they all use `.replace()` on exact strings and silently succeed when nothing matched. That's the systemic issue: not missing checks, but a codebase that treats "I tried and nothing happened" as success.
