use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use console::style;

use crate::discover::{self, Platform};
use crate::output;

/// Run `nex init` — bootstrap nix (+ homebrew on macOS) and a system config.
pub fn run(from: Option<String>, dry_run: bool) -> Result<()> {
    tracing::info!(from = ?from, dry_run, "init");
    let platform = discover::detect_platform();

    println!();
    println!("  {} — first-time setup", style("nex init").bold());
    println!();

    let config_label = match platform {
        Platform::Darwin => "nix-darwin",
        Platform::Linux => "NixOS",
    };

    // 0. Check if a nix config already exists
    if let Ok(existing) = crate::discover::find_repo() {
        eprintln!(
            "  {} found existing {} config at {}",
            style("!").yellow().bold(),
            config_label,
            style(existing.display()).cyan()
        );
        eprintln!();

        let adopt = dialoguer::Confirm::new()
            .with_prompt(format!(
                "  Use {} instead of creating a new config?",
                existing.display()
            ))
            .default(true)
            .interact()?;

        if adopt {
            let hostname = crate::discover::hostname()?;
            let config_dir = crate::config::config_dir()?;

            if !dry_run {
                std::fs::create_dir_all(&config_dir)?;
                let config_content = format!(
                    "repo_path = \"{}\"\nhostname = \"{}\"\n",
                    existing.display(),
                    hostname
                );
                crate::edit::atomic_write_bytes(
                    &config_dir.join("config.toml"),
                    config_content.as_bytes(),
                )?;
            }

            ok("config repo", &existing.display().to_string());
            ok(
                "config",
                &config_dir.join("config.toml").display().to_string(),
            );
            eprintln!();
            eprintln!(
                "  nex is now using {}. Run {} to activate.",
                style(existing.display()).cyan(),
                style("nex switch").bold()
            );
            eprintln!();
            return Ok(());
        }
        eprintln!();
    }

    // 1. Check / install Nix
    let has_nix = check_cmd("nix");
    if has_nix {
        ok("nix", &capture_version("nix", &["--version"]));
    } else if dry_run {
        output::dry_run("would install Determinate Nix");
    } else {
        install_nix()?;
    }

    // 2. Check / install Homebrew (macOS only)
    //
    // Scaffolded configs declare nix-homebrew, which installs and pins brew
    // during the first `darwin-rebuild switch` — no imperative install needed.
    // For --from clones we don't know what's in the user's flake, so fall back
    // to the shell installer if brew is missing.
    let _has_brew = if platform == Platform::Darwin {
        let has_brew = check_cmd("brew");
        if has_brew {
            ok("homebrew", &capture_version("brew", &["--version"]));
        } else if dry_run {
            if from.is_some() {
                output::dry_run("would install Homebrew (clone path)");
            } else {
                output::dry_run("Homebrew will be installed by nix-homebrew on first switch");
            }
        } else if from.is_some() {
            install_homebrew()?;
        } else {
            info(
                "homebrew",
                "deferred — nix-homebrew will install on first switch",
            );
        }
        has_brew || !dry_run
    } else {
        false
    };

    // 3. Verify git is available (required for nix flakes)
    //    On a fresh Mac, the Homebrew installer installs Xcode Command Line Tools
    //    which provides git. If something went wrong, catch it here before we try
    //    to scaffold a git repo.
    if !dry_run {
        let has_git = Command::new("git")
            .args(["--version"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if has_git {
            ok("git", &capture_version("git", &["--version"]));
        } else {
            eprintln!();
            eprintln!(
                "  {} git is not available — nix flakes require a git repository",
                style("!").red().bold(),
            );
            eprintln!();
            if platform == Platform::Darwin {
                eprintln!("  Install Xcode Command Line Tools, then re-run nex init:");
                eprintln!("    {}", style("xcode-select --install").cyan());
            } else {
                eprintln!("  Install git, then re-run nex init:");
                eprintln!(
                    "    {}",
                    style("sudo apt install git  # or your distro's equivalent").cyan()
                );
            }
            eprintln!();
            bail!("git is required but not found");
        }
    }

    // 4. Detect hostname
    let hostname = crate::discover::hostname()?;
    ok("hostname", &hostname);

    // 5. Set up the nix-darwin config repo
    let repo_path = match from {
        Some(url) => clone_repo(&url, dry_run)?,
        None => scaffold_repo(&hostname, dry_run)?,
    };

    ok("config repo", &repo_path.display().to_string());

    // 6. Write nex config so future commands find the repo
    let config_dir = crate::config::config_dir()?;

    if !dry_run {
        std::fs::create_dir_all(&config_dir)?;
        let config_content = format!(
            "repo_path = \"{}\"\nhostname = \"{}\"\n",
            repo_path.display(),
            hostname
        );
        crate::edit::atomic_write_bytes(
            &config_dir.join("config.toml"),
            config_content.as_bytes(),
        )?;
    }
    ok(
        "config",
        &config_dir.join("config.toml").display().to_string(),
    );

    // 7. First build + switch
    if dry_run {
        let rebuild_cmd = match platform {
            Platform::Darwin => "darwin-rebuild switch",
            Platform::Linux => "nixos-rebuild switch",
        };
        output::dry_run(&format!("would run {rebuild_cmd}"));
        println!();
        return Ok(());
    }

    // Ensure git tree is clean so nix doesn't refuse to build.
    // scaffold_repo already does git init + add + commit + identity setup,
    // but clone_repo or an adopted repo may have dirty state.
    crate::exec::git_commit(&repo_path, "nex init");

    println!();
    output::status("building (this takes a few minutes on first run)...");

    // First build to verify it works
    let nix = crate::exec::find_nix();
    let build_attr = match platform {
        Platform::Darwin => format!(".#darwinConfigurations.{hostname}.system"),
        Platform::Linux => format!(".#nixosConfigurations.{hostname}.config.system.build.toplevel"),
    };
    let build_status = Command::new(&nix)
        .args(["build", &build_attr, "--show-trace"])
        .current_dir(&repo_path)
        .status()
        .context("failed to run nix build")?;

    if !build_status.success() {
        bail!(
            "nix build failed — check the config at {}\n\
             You can fix issues and re-run: nex init",
            repo_path.display()
        );
    }

    // Check for existing brew packages BEFORE activating, because
    // homebrew.onActivation.cleanup = "zap" will remove anything not in the
    // nix-managed brew lists. Run nex adopt to capture them first.
    if platform == Platform::Darwin {
        let has_brew_packages = crate::exec::brew_available()
            && (!crate::exec::brew_leaves().unwrap_or_default().is_empty()
                || !crate::exec::brew_list_casks()
                    .unwrap_or_default()
                    .is_empty());

        if has_brew_packages {
            println!();
            eprintln!(
                "  {} existing brew packages detected — adopting before activation",
                style("!").yellow().bold()
            );
            eprintln!(
                "  This prevents {} from removing your installed packages.",
                style("cleanup = \"zap\"").dim()
            );
            println!();
            // Run nex adopt to capture existing packages into the nix config
            let adopt_status =
                Command::new(std::env::current_exe().unwrap_or_else(|_| "nex".into()))
                    .args(["adopt"])
                    .current_dir(&repo_path)
                    .status();
            if let Ok(status) = adopt_status {
                if status.success() {
                    // Re-stage and commit the adopted packages
                    crate::exec::git_commit(
                        &repo_path,
                        "nex adopt: capture existing brew packages",
                    );
                    // Rebuild with the adopted packages
                    output::status("rebuilding with adopted packages...");
                    let _ = Command::new(&nix)
                        .args(["build", &build_attr, "--show-trace"])
                        .current_dir(&repo_path)
                        .status();
                }
            }
        }
    }

    output::status("activating (sudo required)...");

    // nix-darwin refuses to overwrite files in /etc on first run.
    // Move them out of the way so activation can proceed. (macOS only)
    let etc_files = ["/etc/shells", "/etc/nix/nix.conf"];
    if platform == Platform::Darwin {
        for path in &etc_files {
            let p = Path::new(path);
            let backup = format!("{path}.before-nix-darwin");
            if p.exists() && !Path::new(&backup).exists() {
                info("backing up", &format!("{path} → {backup}"));
                let _ = Command::new("sudo").args(["mv", path, &backup]).status();
            }
        }
    }

    // Ensure home-manager profile dirs exist before first switch
    crate::exec::ensure_profile_dirs();

    // Use the system rebuild from the build result, via sudo
    let switch_ok = match platform {
        Platform::Darwin => {
            let result_path = repo_path.join("result/sw/bin/darwin-rebuild");
            if result_path.exists() {
                Command::new("sudo")
                    .args([
                        result_path.to_string_lossy().as_ref(),
                        "switch",
                        "--flake",
                        &format!(".#{hostname}"),
                    ])
                    .current_dir(&repo_path)
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false)
            } else {
                Command::new("sudo")
                    .args([
                        "darwin-rebuild",
                        "switch",
                        "--flake",
                        &format!(".#{hostname}"),
                    ])
                    .current_dir(&repo_path)
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false)
            }
        }
        Platform::Linux => Command::new("sudo")
            .args([
                "nixos-rebuild",
                "switch",
                "--flake",
                &format!(".#{hostname}"),
            ])
            .current_dir(&repo_path)
            .status()
            .map(|s| s.success())
            .unwrap_or(false),
    };

    if !switch_ok {
        // Restore /etc files that were moved (macOS only)
        if platform == Platform::Darwin {
            for path in &etc_files {
                let backup = format!("{path}.before-nix-darwin");
                if Path::new(&backup).exists() {
                    let _ = Command::new("sudo").args(["mv", &backup, path]).status();
                    info("restored", path);
                }
            }
        }
        println!();
        output::error("automatic activation failed — run manually:");
        match platform {
            Platform::Darwin => println!(
                "  cd {} && sudo ./result/sw/bin/darwin-rebuild switch --flake .#{}",
                repo_path.display(),
                hostname
            ),
            Platform::Linux => println!(
                "  cd {} && sudo nixos-rebuild switch --flake .#{}",
                repo_path.display(),
                hostname
            ),
        }
        println!();
        println!("  After that, open a new terminal and nex is ready.");
        return Ok(());
    }

    println!();
    println!("  {} System setup complete.", style("✓").green().bold());
    println!();

    // ── Identity setup (optional, interactive) ─────────────────────
    let identity_path = styrene_identity::file_signer::FileSigner::default_path();
    if !identity_path.exists() {
        let setup_identity = dialoguer::Confirm::new()
            .with_prompt("  Create a Styrene identity? (SSH keys, git signing, mesh)")
            .default(true)
            .interact()
            .unwrap_or(false);

        if setup_identity {
            if let Err(e) = crate::ops::identity::run_init(None) {
                eprintln!(
                    "  {} identity creation failed: {e}",
                    style("!").yellow().bold()
                );
                eprintln!(
                    "  Run {} later to set up your identity.",
                    style("nex identity init").bold()
                );
            } else {
                // Offer git signing
                let setup_git = dialoguer::Confirm::new()
                    .with_prompt("  Configure git commit signing?")
                    .default(true)
                    .interact()
                    .unwrap_or(false);

                if setup_git {
                    if let Err(e) = crate::ops::identity::run_git(false) {
                        eprintln!(
                            "  {} git signing setup failed: {e}",
                            style("!").yellow().bold()
                        );
                    }
                }

                // Offer SSH key registration
                let setup_ssh = dialoguer::Confirm::new()
                    .with_prompt("  Register an SSH key? (e.g. for GitHub)")
                    .default(true)
                    .interact()
                    .unwrap_or(false);

                if setup_ssh {
                    let label: String = dialoguer::Input::new()
                        .with_prompt("  SSH key label")
                        .default("github".to_string())
                        .interact_text()
                        .unwrap_or_else(|_| "github".to_string());

                    if let Err(e) = crate::ops::identity::run_ssh(None, false, Some(label)) {
                        eprintln!("  {} SSH key setup failed: {e}", style("!").yellow().bold());
                    }
                }
            }
        }
    } else {
        eprintln!(
            "  {} identity already exists at {}",
            style("✓").green().bold(),
            style(identity_path.display()).dim()
        );
    }

    println!();
    println!("  {} All done.", style("✓").green().bold());
    println!();
    println!("  Next steps:");
    println!(
        "  {}  Install a package",
        style("  nex install htop").cyan()
    );
    println!("  {}  Show all packages", style("  nex list").cyan());
    if identity_path.exists() {
        println!(
            "  {}  Show your identity",
            style("  nex identity show").cyan()
        );
    }
    println!();

    Ok(())
}

fn check_cmd(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn capture_version(cmd: &str, args: &[&str]) -> String {
    Command::new(cmd)
        .args(args)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().lines().next().unwrap_or("").to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn ok(label: &str, detail: &str) {
    eprintln!(
        "  {} {}: {}",
        style("✓").green().bold(),
        label,
        style(detail).dim()
    );
}

fn info(label: &str, detail: &str) {
    eprintln!("  {} {}: {}", style("→").cyan(), label, style(detail).dim());
}

fn install_nix() -> Result<()> {
    output::status("installing Determinate Nix...");
    let status = Command::new("sh")
        .args([
            "-c",
            "curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install",
        ])
        .status()
        .context("failed to run nix installer")?;

    if !status.success() {
        output::error("shell installer failed — trying macOS .pkg installer...");
        install_nix_pkg()?;
    }

    source_nix_env();
    Ok(())
}

fn install_nix_pkg() -> Result<()> {
    let tmp_dir = std::env::temp_dir();
    let pkg_path = tmp_dir.join("determinate-nix.pkg");

    output::status("downloading Determinate Nix .pkg...");
    let dl = Command::new("curl")
        .args([
            "-fsSL",
            "https://install.determinate.systems/determinate-pkg/stable/Universal",
            "-o",
            &pkg_path.display().to_string(),
        ])
        .status()
        .context("failed to download .pkg installer")?;

    if !dl.success() {
        bail!(
            "failed to download Determinate Nix .pkg\n\
             Install Nix manually: https://determinate.systems/nix-installer\n\
             Then re-run: nex init"
        );
    }

    output::status("installing .pkg (sudo required)...");
    let install = Command::new("sudo")
        .args([
            "installer",
            "-pkg",
            &pkg_path.display().to_string(),
            "-target",
            "/",
        ])
        .status()
        .context("failed to run .pkg installer")?;

    // Clean up
    let _ = std::fs::remove_file(&pkg_path);

    if !install.success() {
        bail!(
            "Determinate Nix .pkg installation failed\n\
             Install Nix manually: https://determinate.systems/nix-installer\n\
             Then re-run: nex init"
        );
    }

    Ok(())
}

fn source_nix_env() {
    // Add well-known nix paths so subsequent commands can find the nix binary.
    // We can't source nix-daemon.sh from Rust, but the known paths are stable.
    let current_path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var(
        "PATH",
        format!("/nix/var/nix/profiles/default/bin:/run/current-system/sw/bin:{current_path}"),
    );
}

fn install_homebrew() -> Result<()> {
    output::status("installing Homebrew...");
    let status = Command::new("sh")
        .args([
            "-c",
            "/bin/bash -c \"$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)\"",
        ])
        .status()
        .context("failed to run Homebrew installer")?;

    if !status.success() {
        bail!("Homebrew installation failed");
    }

    // Add homebrew to PATH for this process
    let current_path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", format!("/opt/homebrew/bin:{current_path}"));

    Ok(())
}

fn clone_repo(url: &str, dry_run: bool) -> Result<PathBuf> {
    let home = dirs::home_dir().context("no home directory")?;
    let repo_path = home.join(discover::default_repo_name());

    if repo_path.exists() {
        return Ok(repo_path);
    }

    if dry_run {
        output::dry_run(&format!("would clone {url} to {}", repo_path.display()));
        return Ok(repo_path);
    }

    output::status(&format!("cloning {url}..."));
    let status = Command::new("git")
        .args(["clone", url, &repo_path.display().to_string()])
        .status()
        .context("failed to run git clone")?;

    if !status.success() {
        bail!("git clone failed");
    }

    Ok(repo_path)
}

fn scaffold_repo(hostname: &str, dry_run: bool) -> Result<PathBuf> {
    let platform = discover::detect_platform();
    let home = dirs::home_dir().context("no home directory")?;
    let repo_path = home.join(discover::default_repo_name());

    if repo_path.exists() {
        return Ok(repo_path);
    }

    if dry_run {
        output::dry_run(&format!(
            "would scaffold nix config at {}",
            repo_path.display()
        ));
        return Ok(repo_path);
    }

    let config_label = match platform {
        Platform::Darwin => "nix-darwin",
        Platform::Linux => "NixOS",
    };
    output::status(&format!("scaffolding {config_label} config..."));

    // Create directory structure
    let host_dir = repo_path.join(format!("nix/hosts/{hostname}"));
    let home_dir = repo_path.join("nix/modules/home");
    let lib_dir = repo_path.join("nix/lib");

    std::fs::create_dir_all(&host_dir)?;
    std::fs::create_dir_all(&home_dir)?;
    std::fs::create_dir_all(&lib_dir)?;

    let user = std::env::var("USER").unwrap_or_else(|_| "user".to_string());
    let system = discover::detect_system();

    match platform {
        Platform::Darwin => {
            let darwin_dir = repo_path.join("nix/modules/darwin");
            std::fs::create_dir_all(&darwin_dir)?;
            scaffold_darwin(
                &repo_path,
                &host_dir,
                &darwin_dir,
                &lib_dir,
                hostname,
                system,
                &user,
            )?;
        }
        Platform::Linux => {
            let nixos_dir = repo_path.join("nix/modules/nixos");
            std::fs::create_dir_all(&nixos_dir)?;
            scaffold_nixos(
                &repo_path, &host_dir, &nixos_dir, &lib_dir, hostname, system, &user,
            )?;
        }
    }

    // home/base.nix — shared between platforms
    let home_directory = match platform {
        Platform::Darwin => "/Users/${username}",
        Platform::Linux => "/home/${username}",
    };
    crate::edit::atomic_write_bytes(
        &home_dir.join("base.nix"),
        format!(
            "{{ pkgs, username, ... }}:\n\
             \n\
             {{\n\
             \x20 home = {{\n\
             \x20   username = username;\n\
             \x20   homeDirectory = \"{home_directory}\";\n\
             \x20   stateVersion = \"25.05\";\n\
             \x20 }};\n\
             \n\
             \x20 home.sessionPath = [\n\
             \x20   \"$HOME/.local/bin\"\n\
             \x20 ];\n\
             \n\
             \x20 home.packages = with pkgs; [\n\
             \x20   git\n\
             \x20   vim\n\
             \x20 ];\n\
             \n\
             \x20 # Enable bash so home-manager generates .bashrc and .bash_profile.\n\
             \x20 # Without this, the login shell works but has no managed config.\n\
             \x20 programs.bash.enable = true;\n\
             \x20 programs.home-manager.enable = true;\n\
             }}\n"
        )
        .as_bytes(),
    )?;

    // Init git repo — nix flakes require files to be tracked by git
    let git_init = Command::new("git")
        .args(["init"])
        .current_dir(&repo_path)
        .output()
        .context("failed to run git init")?;

    if !git_init.status.success() {
        bail!(
            "git init failed in {} — nix flakes require a git repository.\n\
             Check that git is installed: git --version",
            repo_path.display()
        );
    }

    let _ = Command::new("git")
        .args(["branch", "-m", "main"])
        .current_dir(&repo_path)
        .output();

    let git_add = Command::new("git")
        .args(["add", "-A"])
        .current_dir(&repo_path)
        .output()
        .context("failed to run git add")?;

    if !git_add.status.success() {
        bail!(
            "git add failed in {} — nix flakes require files to be tracked.\n\
             Run manually: cd {} && git add -A && git commit -m 'init'",
            repo_path.display(),
            repo_path.display()
        );
    }

    // Set fallback git identity if not configured (fresh systems with no .gitconfig)
    let has_name = Command::new("git")
        .args(["config", "user.name"])
        .current_dir(&repo_path)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !has_name {
        let user = std::env::var("USER").unwrap_or_else(|_| "nex".to_string());
        let _ = Command::new("git")
            .args(["config", "user.name", &user])
            .current_dir(&repo_path)
            .output();
        let _ = Command::new("git")
            .args(["config", "user.email", &format!("{user}@localhost")])
            .current_dir(&repo_path)
            .output();
    }

    let commit_out = Command::new("git")
        .args(["commit", "-m", "init: nex scaffold"])
        .current_dir(&repo_path)
        .output();
    match commit_out {
        Ok(o) if !o.status.success() => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            if !stderr.contains("nothing to commit") {
                output::warn(&format!(
                    "git commit failed — please commit manually: {}",
                    stderr.trim()
                ));
            }
        }
        Err(e) => output::warn(&format!("could not run git commit: {e}")),
        _ => {}
    }

    Ok(repo_path)
}

// ── Darwin (macOS) scaffolding ───────────────────────────────────────────

fn scaffold_darwin(
    repo_path: &Path,
    host_dir: &Path,
    darwin_dir: &Path,
    lib_dir: &Path,
    hostname: &str,
    system: &str,
    user: &str,
) -> Result<()> {
    let enable_rosetta = if system == "aarch64-darwin" {
        "true"
    } else {
        "false"
    };

    // flake.nix
    crate::edit::atomic_write_bytes(
        &repo_path.join("flake.nix"),
        format!(
            r#"{{
  description = "macOS workstation management — nix-darwin + home-manager";

  inputs = {{
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    nix-darwin = {{
      url = "github:LnL7/nix-darwin";
      inputs.nixpkgs.follows = "nixpkgs";
    }};
    home-manager = {{
      url = "github:nix-community/home-manager";
      inputs.nixpkgs.follows = "nixpkgs";
    }};
    mac-app-util.url = "github:hraban/mac-app-util";

    # Declarative Homebrew installation (the brew binary itself + taps).
    # Package lists live in nix-darwin's homebrew.brews/casks options.
    nix-homebrew.url = "github:zhaofengli/nix-homebrew";
    homebrew-core = {{
      url = "github:homebrew/homebrew-core";
      flake = false;
    }};
    homebrew-cask = {{
      url = "github:homebrew/homebrew-cask";
      flake = false;
    }};
  }};

  outputs = {{ self, nixpkgs, nix-darwin, home-manager, mac-app-util,
              nix-homebrew, homebrew-core, homebrew-cask }}:
    let
      mkHost = import ./nix/lib/mkHost.nix {{
        inherit nixpkgs nix-darwin home-manager mac-app-util
                nix-homebrew homebrew-core homebrew-cask;
      }};
    in
    {{
      darwinConfigurations."{hostname}" = mkHost {{
        hostname = "{hostname}";
        system = "{system}";
        username = "{user}";
        hostModule = ./nix/hosts/{hostname};
      }};
    }};
}}
"#
        )
        .as_bytes(),
    )?;

    // mkHost.nix
    crate::edit::atomic_write_bytes(
        &lib_dir.join("mkHost.nix"),
        r#"{ nixpkgs, nix-darwin, home-manager, mac-app-util,
    nix-homebrew, homebrew-core, homebrew-cask }:

{ hostname, system, username, hostModule }:

nix-darwin.lib.darwinSystem {
  inherit system;
  specialArgs = {
    inherit hostname username;
    inherit homebrew-core homebrew-cask;
  };
  modules = [
    hostModule
    mac-app-util.darwinModules.default
    nix-homebrew.darwinModules.nix-homebrew
    home-manager.darwinModules.home-manager
    {
      home-manager = {
        useGlobalPkgs = true;
        useUserPackages = true;
        backupFileExtension = "backup";
        extraSpecialArgs = { inherit hostname username; };
        sharedModules = [
          mac-app-util.homeManagerModules.default
        ];
      };
    }
  ];
}
"#
        .as_bytes(),
    )?;

    // Host default.nix
    crate::edit::atomic_write_bytes(
        &host_dir.join("default.nix"),
        r#"{ pkgs, hostname, username, ... }:

{
  imports = [
    ../../modules/darwin/base.nix
    ../../modules/darwin/homebrew.nix
  ];

  networking.hostName = hostname;
  networking.localHostName = hostname;

  home-manager.users.${username} = import ../../modules/home/base.nix;

  system.stateVersion = 6;
}
"#
        .as_bytes(),
    )?;

    // darwin/base.nix
    let has_determinate =
        check_cmd("determinate-nixd") || Path::new("/nix/var/determinate").exists();

    let nix_block = if has_determinate {
        "  # Determinate Nix manages the daemon — disable nix-darwin's nix management\n  \
         nix.enable = false;\n"
    } else {
        "  nix.settings.experimental-features = [ \"nix-command\" \"flakes\" ];\n  \
         nix.package = pkgs.nix;\n"
    };

    crate::edit::atomic_write_bytes(
        &darwin_dir.join("base.nix"),
        format!(
            r#"{{ pkgs, username, ... }}:

{{
{nix_block}
  nixpkgs.config.allowUnfree = true;

  system.primaryUser = username;

  environment.shells = [ pkgs.bash ];
  users.users.${{username}} = {{
    shell = pkgs.bash;
    home = "/Users/${{username}}";
  }};

  security.pam.services.sudo_local.touchIdAuth = true;
}}
"#
        )
        .as_bytes(),
    )?;

    // darwin/homebrew.nix
    crate::edit::atomic_write_bytes(
        &darwin_dir.join("homebrew.nix"),
        format!(
            r#"{{ config, username, homebrew-core, homebrew-cask, ... }}:

{{
  # nix-homebrew installs and pins Homebrew itself + the core/cask taps.
  # It does NOT install packages — that's still done by the homebrew.* options below.
  nix-homebrew = {{
    enable = true;
    enableRosetta = {enable_rosetta};
    user = username;
    taps = {{
      "homebrew/homebrew-core" = homebrew-core;
      "homebrew/homebrew-cask" = homebrew-cask;
    }};
    mutableTaps = false;
  }};

  homebrew = {{
    enable = true;
    # Keep nix-darwin's tap list aligned with nix-homebrew's pinned taps.
    taps = builtins.attrNames config.nix-homebrew.taps;
    onActivation = {{
      # Taps are pinned via flake inputs and live in the read-only /nix/store,
      # so `brew update` (which does git fetch+reset inside the tap dir) fails.
      # Run `nex update` to bump the pinned tap content instead.
      autoUpdate = false;
      upgrade = true;
      cleanup = "zap";
    }};
    brews = [
    ];
    casks = [
    ];
  }};
}}
"#
        )
        .as_bytes(),
    )?;

    Ok(())
}

// ── NixOS (Linux) scaffolding ────────────────────────────────────────────

fn scaffold_nixos(
    repo_path: &Path,
    host_dir: &Path,
    nixos_dir: &Path,
    lib_dir: &Path,
    hostname: &str,
    system: &str,
    user: &str,
) -> Result<()> {
    // flake.nix
    crate::edit::atomic_write_bytes(
        &repo_path.join("flake.nix"),
        format!(
            r#"{{
  description = "NixOS workstation management — NixOS + home-manager";

  inputs = {{
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    home-manager = {{
      url = "github:nix-community/home-manager";
      inputs.nixpkgs.follows = "nixpkgs";
    }};
  }};

  outputs = {{ self, nixpkgs, home-manager }}:
    let
      mkHost = import ./nix/lib/mkHost.nix {{ inherit nixpkgs home-manager; }};
    in
    {{
      nixosConfigurations."{hostname}" = mkHost {{
        hostname = "{hostname}";
        system = "{system}";
        username = "{user}";
        hostModule = ./nix/hosts/{hostname};
      }};
    }};
}}
"#
        )
        .as_bytes(),
    )?;

    // mkHost.nix
    crate::edit::atomic_write_bytes(
        &lib_dir.join("mkHost.nix"),
        r#"{ nixpkgs, home-manager }:

{ hostname, system, username, hostModule }:

nixpkgs.lib.nixosSystem {
  inherit system;
  specialArgs = { inherit hostname username; };
  modules = [
    hostModule
    home-manager.nixosModules.home-manager
    {
      home-manager = {
        useGlobalPkgs = true;
        useUserPackages = true;
        backupFileExtension = "backup";
        extraSpecialArgs = { inherit hostname username; };
      };
    }
  ];
}
"#
        .as_bytes(),
    )?;

    // Host default.nix
    crate::edit::atomic_write_bytes(
        &host_dir.join("default.nix"),
        r#"{ pkgs, hostname, username, ... }:

{
  imports = [
    ../../modules/nixos/base.nix
    ./hardware-configuration.nix
  ];

  networking.hostName = hostname;

  home-manager.users.${username} = import ../../modules/home/base.nix;

  system.stateVersion = "25.05";
}
"#
        .as_bytes(),
    )?;

    // Generate hardware-configuration.nix if nixos-generate-config is available
    if check_cmd("nixos-generate-config") {
        let _ = Command::new("nixos-generate-config")
            .args(["--show-hardware-config"])
            .output()
            .map(|output| {
                if output.status.success() {
                    let _ = crate::edit::atomic_write_bytes(
                        &host_dir.join("hardware-configuration.nix"),
                        &output.stdout,
                    );
                }
            });
    }
    // If hardware-configuration.nix doesn't exist, create a placeholder
    if !host_dir.join("hardware-configuration.nix").exists() {
        crate::edit::atomic_write_bytes(
            &host_dir.join("hardware-configuration.nix"),
            r#"# Auto-generated hardware configuration.
# Replace with output of: nixos-generate-config --show-hardware-config
{ config, lib, pkgs, modulesPath, ... }:

{
  imports = [
    (modulesPath + "/installer/scan/not-detected.nix")
  ];

  boot.loader.systemd-boot.enable = true;
  boot.loader.efi.canTouchEfiVariables = true;
}
"#
            .as_bytes(),
        )?;
    }

    // nixos/base.nix
    let has_determinate =
        check_cmd("determinate-nixd") || Path::new("/nix/var/determinate").exists();

    let nix_block = if has_determinate {
        "  # Determinate Nix manages the daemon\n  \
         nix.enable = false;\n"
    } else {
        "  nix.settings.experimental-features = [ \"nix-command\" \"flakes\" ];\n"
    };

    crate::edit::atomic_write_bytes(
        &nixos_dir.join("base.nix"),
        format!(
            r#"{{ pkgs, username, ... }}:

{{
{nix_block}
  nixpkgs.config.allowUnfree = true;

  users.users.${{username}} = {{
    isNormalUser = true;
    extraGroups = [ "wheel" "networkmanager" "video" "audio" ];
    shell = pkgs.bash;
  }};

  environment.shells = [ pkgs.bash ];

  # Networking
  networking.networkmanager.enable = true;

  # Sound
  services.pipewire = {{
    enable = true;
    alsa.enable = true;
    pulse.enable = true;
  }};

  # Timezone — override in host config if needed
  time.timeZone = "America/New_York";

  # Locale
  i18n.defaultLocale = "en_US.UTF-8";
}}
"#
        )
        .as_bytes(),
    )?;

    Ok(())
}
