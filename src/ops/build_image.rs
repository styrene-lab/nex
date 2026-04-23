use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};
use console::style;

use crate::exec;
use crate::ops::forge;

/// Run `nex build-image` — build an OCI container image from a profile.
pub fn run(profile_ref: &str, name: Option<&str>, tag: &str, dry_run: bool) -> Result<()> {
    println!();
    println!(
        "  {} — build container image",
        style("nex build-image").bold()
    );
    println!();

    // Resolve profile chain (same as forge)
    let resolved = if Path::new(profile_ref).join("profile.toml").exists() {
        // Local path
        let content = std::fs::read_to_string(Path::new(profile_ref).join("profile.toml"))?;
        forge::ResolvedProfile {
            merged: content,
            chain: vec![profile_ref.to_string()],
        }
    } else if Path::new(profile_ref).exists() && profile_ref.ends_with(".toml") {
        let content = std::fs::read_to_string(profile_ref)?;
        forge::ResolvedProfile {
            merged: content,
            chain: vec![profile_ref.to_string()],
        }
    } else {
        forge::resolve_profile_chain(profile_ref)?
    };

    let profile: toml::Value = toml::from_str(&resolved.merged).context("invalid profile.toml")?;

    // Derive image name from profile
    let image_name = name.unwrap_or_else(|| {
        profile
            .get("meta")
            .and_then(|m| m.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or("nex-image")
    });

    println!(
        "  {} profile: {} ({} layers)",
        style("✓").green().bold(),
        style(profile_ref).cyan(),
        resolved.chain.len()
    );
    println!(
        "  {} image: {}:{}",
        style("→").cyan(),
        style(image_name).bold(),
        tag
    );

    if dry_run {
        println!();
        println!("  Would build: {}:{}", image_name, tag);
        return Ok(());
    }

    // Collect packages — [container.packages] takes priority over [packages.nix]
    let packages: Vec<&str> = profile
        .get("container")
        .and_then(|c| c.get("packages"))
        .and_then(|n| n.as_array())
        .or_else(|| {
            profile
                .get("packages")
                .and_then(|p| p.get("nix"))
                .and_then(|n| n.as_array())
        })
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    // Collect env vars
    let env_vars: Vec<(String, String)> = profile
        .get("shell")
        .and_then(|s| s.get("env"))
        .and_then(|e| e.as_table())
        .map(|t| {
            t.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();

    // Container-specific settings
    let container = profile.get("container");
    let entrypoint = container
        .and_then(|c| c.get("entrypoint"))
        .and_then(|e| e.as_str())
        .unwrap_or("/bin/bash");
    let expose: Vec<u16> = container
        .and_then(|c| c.get("expose"))
        .and_then(|e| e.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_integer().map(|i| i as u16))
                .collect()
        })
        .unwrap_or_default();
    let user = container
        .and_then(|c| c.get("user"))
        .and_then(|u| u.as_str());
    let workdir = container
        .and_then(|c| c.get("workdir"))
        .and_then(|w| w.as_str())
        .unwrap_or("/workspace");
    let cmd: Vec<&str> = container
        .and_then(|c| c.get("cmd"))
        .and_then(|c| c.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    // Generate the nix expression
    let nix_expr = generate_image_nix(
        image_name, tag, &packages, &env_vars, entrypoint, &expose, user, workdir, &cmd,
    );

    // Write to temp file
    let tmp_dir = std::env::temp_dir().join("nex-build-image");
    std::fs::create_dir_all(&tmp_dir)?;
    let nix_file = tmp_dir.join("image.nix");
    std::fs::write(&nix_file, &nix_expr)?;

    println!();
    println!(
        "  {} building ({} packages)...",
        style(">>>").bold(),
        packages.len()
    );

    // Build with nix
    let nix = exec::find_nix();
    let output = Command::new(&nix)
        .args([
            "build",
            "--impure",
            "--no-link",
            "--print-out-paths",
            "-f",
            &nix_file.display().to_string(),
        ])
        .output()
        .context("nix build failed")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("required system") && stderr.contains("x86_64-linux") {
            bail!(
                "Cannot build Linux container images on macOS without a remote builder.\n\
                 Options:\n\
                 • Build on a Linux machine: nex build-image {profile_ref}\n\
                 • Set up a remote builder: https://wiki.nixos.org/wiki/Distributed_build\n\
                 • Use a Linux VM: nix build in a NixOS VM or container"
            );
        }
        bail!("image build failed:\n{stderr}");
    }

    let store_path = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if store_path.is_empty() {
        bail!("nix build produced no output");
    }

    // The output is a Docker tarball or OCI layout
    let output_file = format!("{image_name}-{tag}.tar.gz");

    // Copy the image tarball to the working directory
    std::fs::copy(&store_path, &output_file)?;
    println!();
    println!(
        "  {} {}",
        style("✓").green().bold(),
        style(&output_file).cyan()
    );
    println!();

    // Detect available runtime — prefer podman, fall back to docker
    let runtime = detect_container_runtime();

    let size_mb = std::fs::metadata(&output_file)
        .map(|m| m.len() / (1024 * 1024))
        .unwrap_or(0);
    println!(
        "  {} {} ({} MB)",
        style("✓").green().bold(),
        style(&output_file).cyan(),
        size_mb
    );
    println!();
    println!(
        "  Load:  {}",
        style(format!("{runtime} load -i {output_file}")).cyan()
    );
    println!(
        "  Run:   {}",
        style(format!("{runtime} run -it {image_name}:{tag}")).cyan()
    );

    println!();
    Ok(())
}

/// Detect the container runtime — prefer podman, fall back to docker.
fn detect_container_runtime() -> &'static str {
    if Command::new("podman")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return "podman";
    }
    if Command::new("docker")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return "docker";
    }
    "podman" // canonical default even if not installed
}

/// Generate a Nix expression that builds an OCI image using dockerTools.
fn generate_image_nix(
    name: &str,
    tag: &str,
    packages: &[&str],
    env_vars: &[(String, String)],
    entrypoint: &str,
    expose: &[u16],
    user: Option<&str>,
    workdir: &str,
    cmd: &[&str],
) -> String {
    let mut nix = String::new();

    // Containers are always Linux. Use the host arch if on Linux,
    // otherwise default to x86_64-linux (requires remote builder from macOS).
    let container_system = if cfg!(target_os = "linux") {
        crate::discover::detect_system()
    } else {
        "x86_64-linux"
    };
    nix.push_str("let\n");
    nix.push_str(&format!("  pkgs = import <nixpkgs> {{ system = \"{container_system}\"; config.allowUnfree = true; }};\n"));
    nix.push_str("in\n");
    nix.push_str("pkgs.dockerTools.buildLayeredImage {\n");
    nix.push_str(&format!("  name = \"{name}\";\n"));
    nix.push_str(&format!("  tag = \"{tag}\";\n"));
    nix.push_str("\n");

    // Contents — packages
    nix.push_str("  contents = with pkgs; [\n");
    // Always include basics for a usable container
    nix.push_str("    bashInteractive\n");
    nix.push_str("    coreutils\n");
    nix.push_str("    cacert\n");
    for pkg in packages {
        if *pkg != "bash" && *pkg != "coreutils" {
            nix.push_str(&format!("    {pkg}\n"));
        }
    }
    nix.push_str("  ];\n");
    nix.push_str("\n");

    // Config
    nix.push_str("  config = {\n");

    // Entrypoint
    nix.push_str(&format!("    Entrypoint = [ \"{entrypoint}\" ];\n"));

    // Cmd
    if !cmd.is_empty() {
        let cmd_str = cmd
            .iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(" ");
        nix.push_str(&format!("    Cmd = [ {cmd_str} ];\n"));
    }

    // Env
    let mut env_lines: Vec<String> = vec![
        format!("\"PATH=/bin:/usr/bin\""),
        format!("\"SSL_CERT_FILE=${{pkgs.cacert}}/etc/ssl/certs/ca-bundle.crt\""),
    ];
    for (key, val) in env_vars {
        env_lines.push(format!("\"{key}={val}\""));
    }
    nix.push_str("    Env = [\n");
    for env in &env_lines {
        nix.push_str(&format!("      {env}\n"));
    }
    nix.push_str("    ];\n");

    // ExposedPorts
    if !expose.is_empty() {
        nix.push_str("    ExposedPorts = {\n");
        for port in expose {
            nix.push_str(&format!("      \"{port}/tcp\" = {{}};\n"));
        }
        nix.push_str("    };\n");
    }

    // WorkingDir
    nix.push_str(&format!("    WorkingDir = \"{workdir}\";\n"));

    // User
    if let Some(u) = user {
        nix.push_str(&format!("    User = \"{u}\";\n"));
    }

    nix.push_str("  };\n");

    // Create working directory
    nix.push_str(&format!(
        "\n  extraCommands = ''\n    mkdir -p {workdir}\n  '';\n"
    ));

    nix.push_str("}\n");

    nix
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_image_nix_basic() {
        let nix = generate_image_nix(
            "test-image",
            "v1",
            &["git", "ripgrep"],
            &[("EDITOR".to_string(), "vim".to_string())],
            "/bin/bash",
            &[8080],
            Some("app"),
            "/workspace",
            &[],
        );
        assert!(nix.contains("name = \"test-image\""));
        assert!(nix.contains("tag = \"v1\""));
        assert!(nix.contains("git"));
        assert!(nix.contains("ripgrep"));
        assert!(nix.contains("EDITOR=vim"));
        assert!(nix.contains("8080/tcp"));
        assert!(nix.contains("User = \"app\""));
        assert!(nix.contains("buildLayeredImage"));
    }

    #[test]
    fn test_generate_image_nix_minimal() {
        let nix = generate_image_nix(
            "minimal",
            "latest",
            &[],
            &[],
            "/bin/bash",
            &[],
            None,
            "/",
            &[],
        );
        assert!(nix.contains("bashInteractive"));
        assert!(nix.contains("coreutils"));
        assert!(nix.contains("cacert"));
        assert!(!nix.contains("User ="));
        assert!(!nix.contains("ExposedPorts"));
    }

    #[test]
    fn test_generate_image_nix_with_cmd() {
        let nix = generate_image_nix(
            "server",
            "latest",
            &["nginx"],
            &[],
            "/bin/nginx",
            &[80, 443],
            None,
            "/var/www",
            &["-g", "daemon off;"],
        );
        assert!(nix.contains("Entrypoint = [ \"/bin/nginx\" ]"));
        assert!(nix.contains("Cmd = [ \"-g\" \"daemon off;\" ]"));
        assert!(nix.contains("80/tcp"));
        assert!(nix.contains("443/tcp"));
    }

    #[test]
    fn test_generate_image_no_duplicate_bash() {
        let nix = generate_image_nix(
            "test",
            "latest",
            &["bash", "coreutils", "git"],
            &[],
            "/bin/bash",
            &[],
            None,
            "/",
            &[],
        );
        // bash and coreutils should only appear once (from the base, not from packages)
        let bash_count = nix.matches("bashInteractive").count();
        assert_eq!(bash_count, 1);
    }
}
