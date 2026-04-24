# The Roast

Adversarial assessment of nex, performed 2026-04-24.

---

## 1. Your "Parser" Is a Regex With a Dream

The entire edit engine (`edit.rs`) is a `starts_with` + indentation heuristic pretending to understand Nix syntax. The magic `+2` tolerance on line 26 means any stray `];` within 2 spaces of your open bracket ends the list. You control the scaffold, sure -- until someone hand-edits their config, adds a nested list, or uses `let ... in` inside the block. One misplaced semicolon and `edit::insert` cheerfully injects a package name into the middle of an unrelated expression.

You acknowledge "line-based editing, not a nix parser" in CLAUDE.md like that's a design decision and not a liability. It's both.

## 2. "Atomic" Writes That Aren't

`atomic_write` (edit.rs:126) does `write_all` then `persist` (rename) -- but never calls `flush()` or `sync_all()`. The data can sit in a kernel buffer when `persist` renames the file. Power loss = you renamed a partially-written temp file over the user's config. Congratulations, their nix-darwin repo is now a truncated file that `darwin-rebuild` will refuse to parse, and your backup is already gone because `commit_all` deleted it.

Every `std::fs::write` in `doctor.rs`, `profile.rs`, `polymerize.rs`, and `init.rs` has the same problem -- no temp+rename, no fsync. Direct overwrites of nix config files. The one module that bothered with atomicity (`edit.rs`) didn't finish the job.

## 3. Self-Update Is a Supply Chain Attack Waiting to Happen

`self_update.rs`:
- Fetches the latest release tag from GitHub API over `curl -fsSL` -- no signature verification, no checksum validation.
- Downloads a tarball from a URL constructed by string interpolation.
- Extracts it with `tar -xzf` into a tmpdir -- **no path traversal protection**. A malicious tarball with `../../usr/local/bin/nex` entries escapes the tmpdir.
- Replaces the running binary with `cp -f` or `rename`.

You're one compromised GitHub token away from every `nex self-update` installing malware. There's no GPG signature, no cosign, no SHA256 digest check, no SLSA provenance -- nothing. The install script at `site/public/install.sh` probably has the same problem (curl-pipe-sh pattern).

## 4. Silent Error Swallowing Everywhere

The `let _ =` pattern is pandemic:

- `adopt.rs:143-150` -- git add + commit silently ignored. User thinks packages were captured. They weren't committed.
- `doctor.rs:29-36` -- same thing. Doctor "fixes" configs but doesn't persist them to git.
- `profile.rs:701-708` -- profile applied, git commit silently fails.
- `forge.rs:446` -- nix copy failure ignored; ISO boots into a broken state.
- `polymerize.rs:375` -- `read_dir` errors `.flatten()`'d away.

You've got a CLI whose entire value proposition is "safe, atomic, reverts on failure" -- and half the operations silently eat errors on the persistence layer. The nix file edit succeeds but the git commit fails and nobody knows until they `git status` three weeks later.

## 5. WiFi Password Written to World-Readable `/tmp`

`polymerize.rs:343`: the WiFi SSID and password are format-string interpolated (no escaping -- SSID injection into wpa_supplicant config is possible) into a file at `/tmp/wpa_supplicant.conf`. Default umask. World-readable. If the function bails early, the cleanup at line 417 never runs. The password sits on disk until reboot.

## 6. Version Comparison Is String Equality

`resolve.rs:132`: `n.version == b.version`. That's it. `"1.0.0"` != `"1.0"`. `"2.1.0-rc1"` != `"2.1.0"`. You're making install-source recommendations to users based on whether two version strings are byte-identical. No semver parsing, no normalization. The "recommend nix for equal versions" feature only works when upstream packagers happen to use the exact same string representation across two different ecosystems.

## 7. Config File Writes Are Not Atomic Either

`config.rs:152-154`: `set_preference` does `read_to_string` then `std::fs::write`. If the process is killed between read and write, you get a zero-byte config file. The `join("\n")` on line 143 also silently normalizes any CRLF line endings (not that TOML on macOS should have them, but still). And there's no file locking -- two concurrent `nex install` runs can race on the config file.

## 8. `is_already_declared` Reads Every File Multiple Times

`install.rs:90-122`: For each package, you call `edit::contains` -- which does `read_to_string` + `lines().collect()` + linear scan -- once per alias, per file, per list type. For a package with 3 aliases across 2 nix files + 1 homebrew file checking 2 list types, that's potentially **12 full file reads and parses** just to check if one package is already installed. And you do this for every package in the install list. Read the files once.

## 9. No Input Validation at the CLI Boundary

- `--disk` flag in `forge` accepts any path. `/dev/disk0` (your boot drive) passes validation.
- `--hostname` accepts arbitrary strings including spaces, slashes, and shell metacharacters.
- `--from` (repo URL) has no format validation.
- `--nix`, `--cask`, and `--brew` flags aren't marked mutually exclusive in clap -- you can pass all three.
- Package names are never validated for characters that could break the nix expression when inserted.

## 10. The Backup System Has a Silent Data Loss Bug

`EditSession::backup` (edit.rs:177) skips if a backup already exists for that path. But it never verifies the backup file is still on disk. If the backup was deleted between creation and `revert_all`, the `restore` function (line 149-153) checks `backup_path.exists()`, finds it missing, and returns `Ok(())`. Your "atomic revert" silently does nothing. The user's config changes are now permanent and they'll never know the revert failed.

## 11. Module Discovery Is Hardcoded to One File

`config.rs:84-88`: Module discovery is literally "check if `kubernetes.nix` exists." That's it. One hardcoded path. Your CLAUDE.md says the architecture supports multiple module files, but the discovery is a single `if` statement. Every other module file is invisible to duplicate detection and listing.

## 12. The `forge` and `polymerize` Commands Are 4,000 Lines of YOLO

These two files are nearly 4,000 lines combined and they're building bootable ISOs and running NixOS installers. They shell out to `dd`, `mkfs`, `mount`, `nixos-install`, and `nix-env` with minimal error handling and no integration tests. The `forge` command has a code path that writes a placeholder shell script as the `nex` binary on the ISO -- meaning users who boot from this ISO get a fake binary that just prints "not available" and exits.

---

## The Good

Credit where it's due: `unsafe_code = "forbid"` is set. The core install/remove/revert loop in `install.rs` is actually well-structured -- backup before edit, revert on switch failure, commit on success. The alias system is clever. The integration test suite with mock binaries is genuinely thoughtful. The `anyhow` usage with `.context()` is consistent in the modules where it exists.

The bones are good. The problem is that the safety guarantees the tool promises (atomic edits, safe onboarding, revert on failure) have gaps in every layer: the file writes aren't durable, the git commits are fire-and-forget, the backup verification is missing, and the self-update is unsigned. The tool is one bad day away from eating someone's config.
