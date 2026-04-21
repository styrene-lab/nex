use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use console::style;

use crate::output;

/// Run `nex init` — bootstrap nix, homebrew, and a nix-darwin config on a fresh Mac.
pub fn run(from: Option<String>, dry_run: bool) -> Result<()> {
    println!();
    println!("  {} — first-time setup", style("nex init").bold());
    println!();

    // 0. Check if a nix-darwin config already exists
    if let Ok(existing) = crate::discover::find_repo() {
        eprintln!(
            "  {} found existing nix-darwin config at {}",
            style("!").yellow().bold(),
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
                std::fs::write(config_dir.join("config.toml"), &config_content)?;
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

    // 2. Check / install Homebrew
    let has_brew = check_cmd("brew");
    if has_brew {
        ok("homebrew", &capture_version("brew", &["--version"]));
    } else if dry_run {
        output::dry_run("would install Homebrew");
    } else {
        install_homebrew()?;
    }

    // 3. Detect hostname
    let hostname = crate::discover::hostname()?;
    ok("hostname", &hostname);

    // 4. Set up the nix-darwin config repo
    let repo_path = match from {
        Some(url) => clone_repo(&url, dry_run)?,
        None => scaffold_repo(&hostname, dry_run)?,
    };

    ok("config repo", &repo_path.display().to_string());

    // 5. Write nex config so future commands find the repo
    let config_dir = crate::config::config_dir()?;

    if !dry_run {
        std::fs::create_dir_all(&config_dir)?;
        let config_content = format!(
            "repo_path = \"{}\"\nhostname = \"{}\"\n",
            repo_path.display(),
            hostname
        );
        std::fs::write(config_dir.join("config.toml"), config_content)?;
    }
    ok(
        "config",
        &config_dir.join("config.toml").display().to_string(),
    );

    // 6. First build + switch
    if dry_run {
        output::dry_run("would run darwin-rebuild switch");
        println!();
        return Ok(());
    }

    // Ensure git tree is clean so nix doesn't refuse to build
    let _ = Command::new("git")
        .args(["add", "-A"])
        .current_dir(&repo_path)
        .output();
    // Set fallback git identity if not configured (fresh systems)
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
    let commit_status = Command::new("git")
        .args(["commit", "-m", "nex init"])
        .current_dir(&repo_path)
        .output();
    if let Err(e) = commit_status {
        output::error(&format!(
            "git commit failed: {e} — nix build may warn about dirty tree"
        ));
    }

    println!();
    output::status("building (this takes a few minutes on first run)...");

    // First build to verify it works
    let build_status = Command::new("nix")
        .args([
            "build",
            &format!(".#darwinConfigurations.{hostname}.system"),
            "--show-trace",
        ])
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

    output::status("activating (sudo required)...");

    // nix-darwin refuses to overwrite files in /etc on first run.
    // Move them out of the way so activation can proceed.
    let etc_files = ["/etc/shells", "/etc/nix/nix.conf"];
    for path in &etc_files {
        let p = Path::new(path);
        let backup = format!("{path}.before-nix-darwin");
        if p.exists() && !Path::new(&backup).exists() {
            info("backing up", &format!("{path} → {backup}"));
            let _ = Command::new("sudo").args(["mv", path, &backup]).status();
        }
    }

    // Use the darwin-rebuild from the build result, via sudo
    let result_path = repo_path.join("result/sw/bin/darwin-rebuild");
    let switch_ok = if result_path.exists() {
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
        // Fallback: maybe darwin-rebuild is already in PATH
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
    };

    if !switch_ok {
        // Restore /etc files that were moved
        for path in &etc_files {
            let backup = format!("{path}.before-nix-darwin");
            if Path::new(&backup).exists() {
                let _ = Command::new("sudo").args(["mv", &backup, path]).status();
                info("restored", path);
            }
        }
        println!();
        output::error("automatic activation failed — run manually:");
        println!(
            "  cd {} && sudo ./result/sw/bin/darwin-rebuild switch --flake .#{}",
            repo_path.display(),
            hostname
        );
        println!();
        println!("  After that, open a new terminal and nex is ready.");
        return Ok(());
    }

    // Check if brew has existing packages that need adopting
    let has_brew_packages = crate::exec::brew_available()
        && (!crate::exec::brew_leaves().unwrap_or_default().is_empty()
            || !crate::exec::brew_list_casks()
                .unwrap_or_default()
                .is_empty());

    if has_brew_packages {
        println!();
        eprintln!(
            "  {} existing brew packages detected",
            style("!").yellow().bold()
        );
        eprintln!(
            "  Run {} to add them to the nex config before switching.",
            style("nex adopt").bold()
        );
        eprintln!(
            "  This prevents {} from removing your installed packages.",
            style("cleanup = \"zap\"").dim()
        );
    }

    println!();
    println!("  {} Setup complete.", style("✓").green().bold());
    println!();
    println!("  Next steps:");
    if has_brew_packages {
        println!(
            "  {}  Capture existing brew packages",
            style("  nex adopt").cyan()
        );
    }
    println!(
        "  {}  Install a package",
        style("  nex install htop").cyan()
    );
    println!("  {}  Show all packages", style("  nex list").cyan());
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
    // Source nix in current process env
    if let Ok(profile) =
        std::fs::read_to_string("/nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh")
    {
        // Can't source bash in Rust, but we can update PATH
        for line in profile.lines() {
            if line.starts_with("export PATH=") || line.contains("PATH=") {
                if let Some(path_val) = line.split('=').nth(1) {
                    let cleaned = path_val
                        .trim_matches('"')
                        .replace("$PATH", &std::env::var("PATH").unwrap_or_default());
                    std::env::set_var("PATH", cleaned);
                }
            }
        }
    }
    // Simpler fallback: just add the known nix paths
    let current_path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var(
        "PATH",
        format!("/nix/var/nix/profiles/default/bin:{current_path}"),
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
    let repo_path = home.join("macos-nix");

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
    let home = dirs::home_dir().context("no home directory")?;
    let repo_path = home.join("macos-nix");

    if repo_path.exists() {
        return Ok(repo_path);
    }

    if dry_run {
        output::dry_run(&format!(
            "would scaffold nix-darwin config at {}",
            repo_path.display()
        ));
        return Ok(repo_path);
    }

    output::status("scaffolding nix-darwin config...");

    // Create directory structure
    let host_dir = repo_path.join(format!("nix/hosts/{hostname}"));
    let darwin_dir = repo_path.join("nix/modules/darwin");
    let home_dir = repo_path.join("nix/modules/home");
    let lib_dir = repo_path.join("nix/lib");

    std::fs::create_dir_all(&host_dir)?;
    std::fs::create_dir_all(&darwin_dir)?;
    std::fs::create_dir_all(&home_dir)?;
    std::fs::create_dir_all(&lib_dir)?;

    let user = std::env::var("USER").unwrap_or_else(|_| "user".to_string());

    // flake.nix
    std::fs::write(
        repo_path.join("flake.nix"),
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
  }};

  outputs = {{ self, nixpkgs, nix-darwin, home-manager, mac-app-util }}:
    let
      mkHost = import ./nix/lib/mkHost.nix {{ inherit nixpkgs nix-darwin home-manager mac-app-util; }};
    in
    {{
      darwinConfigurations."{hostname}" = mkHost {{
        hostname = "{hostname}";
        system = "aarch64-darwin";
        username = "{user}";
        hostModule = ./nix/hosts/{hostname};
      }};
    }};
}}
"#
        ),
    )?;

    // mkHost.nix
    std::fs::write(
        lib_dir.join("mkHost.nix"),
        r#"{ nixpkgs, nix-darwin, home-manager, mac-app-util }:

{ hostname, system, username, hostModule }:

nix-darwin.lib.darwinSystem {
  inherit system;
  specialArgs = { inherit hostname username; };
  modules = [
    hostModule
    mac-app-util.darwinModules.default
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
"#,
    )?;

    // Host default.nix
    std::fs::write(
        host_dir.join("default.nix"),
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
"#,
    )?;

    // darwin/base.nix
    // Detect Determinate Nix — if present, disable nix-darwin's nix management
    let has_determinate =
        check_cmd("determinate-nixd") || Path::new("/nix/var/determinate").exists();

    let nix_block = if has_determinate {
        "  # Determinate Nix manages the daemon — disable nix-darwin's nix management\n  \
         nix.enable = false;\n"
    } else {
        "  nix.settings.experimental-features = [ \"nix-command\" \"flakes\" ];\n  \
         nix.package = pkgs.nix;\n"
    };

    std::fs::write(
        darwin_dir.join("base.nix"),
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
        ),
    )?;

    // darwin/homebrew.nix
    std::fs::write(
        darwin_dir.join("homebrew.nix"),
        r#"{ ... }:

{
  homebrew = {
    enable = true;
    onActivation = {
      autoUpdate = true;
      upgrade = true;
      cleanup = "zap";
    };
    brews = [
    ];
    casks = [
    ];
  };
}
"#,
    )?;

    // home/base.nix
    std::fs::write(
        home_dir.join("base.nix"),
        "{ pkgs, username, ... }:\n\
         \n\
         {\n\
         \x20 home = {\n\
         \x20   username = username;\n\
         \x20   homeDirectory = \"/Users/${username}\";\n\
         \x20   stateVersion = \"25.05\";\n\
         \x20 };\n\
         \n\
         \x20 home.sessionPath = [ \"$HOME/.local/bin\" ];\n\
         \n\
         \x20 home.packages = with pkgs; [\n\
         \x20   git\n\
         \x20   vim\n\
         \x20 ];\n\
         \n\
         \x20 programs.home-manager.enable = true;\n\
         }\n",
    )?;

    // Init git repo
    let _ = Command::new("git")
        .args(["init"])
        .current_dir(&repo_path)
        .output();
    let _ = Command::new("git")
        .args(["branch", "-m", "main"])
        .current_dir(&repo_path)
        .output();
    let _ = Command::new("git")
        .args(["add", "-A"])
        .current_dir(&repo_path)
        .output();
    let _ = Command::new("git")
        .args(["commit", "-m", "init: nex scaffold"])
        .current_dir(&repo_path)
        .output();

    Ok(repo_path)
}
