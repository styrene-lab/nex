use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use console::style;

use crate::exec;
use crate::ops::forge;

/// Run `nex build-image` — build an OCI container image from a machine profile or package manifest.
pub fn run(source: &str, name: Option<&str>, tag: Option<&str>, dry_run: bool) -> Result<()> {
    println!();
    println!(
        "  {} — build container image",
        style("nex build-image").bold()
    );
    println!();

    let build_source = resolve_build_source(source)?;
    let profile: toml::Value =
        toml::from_str(&build_source.resolved.merged).context("invalid machine-profile.toml")?;

    let image_name = name
        .map(ToOwned::to_owned)
        .or_else(|| build_source.package.as_ref()?.image_name.clone())
        .or_else(|| {
            profile
                .get("meta")
                .and_then(|m| m.get("name"))
                .and_then(|n| n.as_str())
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| "nex-image".to_string());
    let image_tag = tag
        .map(ToOwned::to_owned)
        .or_else(|| build_source.package.as_ref()?.image_tag.clone())
        .unwrap_or_else(|| "latest".to_string());

    println!(
        "  {} profile: {} ({} layers)",
        style("✓").green().bold(),
        style(&build_source.profile_ref).cyan(),
        build_source.resolved.chain.len()
    );
    if let Some(package) = &build_source.package {
        println!(
            "  {} package: {}{}",
            style("✓").green().bold(),
            style(&package.package_name).cyan(),
            package
                .package_version
                .as_deref()
                .map(|version| format!(":{version}"))
                .unwrap_or_default()
        );
    }
    println!(
        "  {} image: {}:{}",
        style("→").cyan(),
        style(&image_name).bold(),
        image_tag
    );

    if dry_run {
        println!();
        println!("  Would build: {}:{}", image_name, image_tag);
        return Ok(());
    }

    let packages = collect_packages(&profile);
    let env_vars = collect_env_vars(&profile);
    let container = profile.get("container");
    let package = build_source.package.as_ref();

    let entrypoint = package
        .and_then(|p| p.entrypoint.clone())
        .or_else(|| {
            container
                .and_then(|c| c.get("entrypoint"))
                .and_then(|e| e.as_str())
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| "/bin/bash".to_string());
    let expose = package
        .and_then(|p| p.expose.clone())
        .or_else(|| {
            container
                .and_then(|c| c.get("expose"))
                .and_then(|e| e.as_array())
                .map(|arr| ports_from_array(arr))
        })
        .unwrap_or_default();
    let user = container
        .and_then(|c| c.get("user"))
        .and_then(|u| u.as_str())
        .map(ToOwned::to_owned);
    let workdir = container
        .and_then(|c| c.get("workdir"))
        .and_then(|w| w.as_str())
        .unwrap_or("/workspace")
        .to_string();
    let cmd = package
        .and_then(|p| p.cmd.clone())
        .or_else(|| {
            container
                .and_then(|c| c.get("cmd"))
                .and_then(|c| c.as_array())
                .map(|arr| strings_from_array(arr))
        })
        .unwrap_or_default();
    let labels = build_source
        .package
        .as_ref()
        .map(|p| p.labels(&build_source.profile_ref))
        .unwrap_or_default();

    let nix_expr = generate_image_nix(&ImageConfig {
        name: &image_name,
        tag: &image_tag,
        packages: &packages,
        env_vars: &env_vars,
        entrypoint: &entrypoint,
        expose: &expose,
        user: user.as_deref(),
        workdir: &workdir,
        cmd: &cmd,
        labels: &labels,
    });

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
                 • Build on a Linux machine: nex build-image {}\n\
                 • Set up a remote builder: https://wiki.nixos.org/wiki/Distributed_build\n\
                 • Use a Linux VM: nix build in a NixOS VM or container",
                build_source.requested
            );
        }
        bail!("image build failed:\n{stderr}");
    }

    let store_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if store_path.is_empty() {
        bail!("nix build produced no output");
    }

    let output_file = image_tarball_name(&image_name, &image_tag);
    std::fs::copy(&store_path, &output_file)?;
    println!();
    println!(
        "  {} {}",
        style("✓").green().bold(),
        style(&output_file).cyan()
    );
    println!();

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
        style(format!("{runtime} run -it {image_name}:{image_tag}")).cyan()
    );

    println!();
    Ok(())
}

struct BuildSource {
    requested: String,
    profile_ref: String,
    resolved: forge::ResolvedProfile,
    package: Option<PackageImage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PackageImage {
    package_name: String,
    package_version: Option<String>,
    package_source: Option<String>,
    image_name: Option<String>,
    image_tag: Option<String>,
    entrypoint: Option<String>,
    cmd: Option<Vec<String>>,
    expose: Option<Vec<u16>>,
    agent_role: Option<String>,
    agent_mode: Option<String>,
    agent_posture: Option<String>,
    agent_model: Option<String>,
}

impl PackageImage {
    fn labels(&self, profile_ref: &str) -> Vec<(String, String)> {
        let mut labels = vec![
            (
                "io.styrene.package.name".to_string(),
                self.package_name.clone(),
            ),
            (
                "io.styrene.nex.profile".to_string(),
                profile_ref.to_string(),
            ),
        ];
        if let Some(version) = &self.package_version {
            labels.push((
                "org.opencontainers.image.version".to_string(),
                version.clone(),
            ));
            labels.push(("io.styrene.package.version".to_string(), version.clone()));
        }
        if let Some(source) = &self.package_source {
            labels.push((
                "org.opencontainers.image.source".to_string(),
                source.clone(),
            ));
        }
        if let Some(role) = &self.agent_role {
            labels.push(("io.styrene.agent.role".to_string(), role.clone()));
        }
        if let Some(mode) = &self.agent_mode {
            labels.push(("io.styrene.agent.mode".to_string(), mode.clone()));
        }
        if let Some(posture) = &self.agent_posture {
            labels.push(("io.styrene.agent.posture".to_string(), posture.clone()));
        }
        if let Some(model) = &self.agent_model {
            labels.push(("io.styrene.agent.model".to_string(), model.clone()));
        }
        labels
    }
}

fn resolve_build_source(source: &str) -> Result<BuildSource> {
    if let Some(package_path) = package_manifest_path(source) {
        let manifest = std::fs::read_to_string(&package_path)
            .with_context(|| format!("reading {}", package_path.display()))?;
        let value: toml::Value = toml::from_str(&manifest).context("invalid package manifest")?;
        let package = parse_package_manifest(&value)?;
        let profile_ref = value
            .get("nex")
            .and_then(|n| n.get("profile"))
            .and_then(|p| p.as_str())
            .context("package manifest requires [nex].profile")?;
        let profile_ref = resolve_manifest_profile_ref(&package_path, profile_ref);

        return Ok(BuildSource {
            requested: source.to_string(),
            resolved: resolve_profile_input(&profile_ref)?,
            profile_ref,
            package: Some(package),
        });
    }

    Ok(BuildSource {
        requested: source.to_string(),
        profile_ref: source.to_string(),
        resolved: resolve_profile_input(source)?,
        package: None,
    })
}

fn resolve_profile_input(profile_ref: &str) -> Result<forge::ResolvedProfile> {
    let manifest_path = Path::new(profile_ref).join(crate::machine_profile::MACHINE_PROFILE_FILE);
    if manifest_path.exists() {
        let content = std::fs::read_to_string(&manifest_path)?;
        Ok(forge::ResolvedProfile {
            merged: content,
            chain: vec![profile_ref.to_string()],
        })
    } else if Path::new(profile_ref).exists() && profile_ref.ends_with(".toml") {
        let content = std::fs::read_to_string(profile_ref)?;
        Ok(forge::ResolvedProfile {
            merged: content,
            chain: vec![profile_ref.to_string()],
        })
    } else {
        forge::resolve_profile_chain(profile_ref)
    }
}

fn package_manifest_path(source: &str) -> Option<PathBuf> {
    let path = Path::new(source);
    if path.is_dir() {
        let package_path = path.join("styrene-package.toml");
        if package_path.exists() {
            return Some(package_path);
        }
    }
    if path.is_file()
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "styrene-package.toml" || name.ends_with(".package.toml"))
    {
        return Some(path.to_path_buf());
    }
    None
}

fn resolve_manifest_profile_ref(manifest_path: &Path, profile_ref: &str) -> String {
    let profile_path = Path::new(profile_ref);
    if profile_path.is_absolute() {
        return profile_ref.to_string();
    }
    if profile_ref.contains('/')
        && !profile_ref.starts_with('.')
        && !profile_ref.ends_with(".toml")
        && !profile_path.exists()
    {
        return profile_ref.to_string();
    }
    manifest_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(profile_path)
        .display()
        .to_string()
}

fn parse_package_manifest(value: &toml::Value) -> Result<PackageImage> {
    let package = value
        .get("package")
        .context("package manifest requires [package]")?;
    let package_name = package
        .get("name")
        .and_then(|name| name.as_str())
        .context("package manifest requires [package].name")?
        .to_string();

    let image = value.get("image");
    let agent = value.get("agent");

    Ok(PackageImage {
        package_name,
        package_version: package
            .get("version")
            .and_then(|version| version.as_str())
            .map(ToOwned::to_owned),
        package_source: package
            .get("source")
            .and_then(|source| source.as_str())
            .map(ToOwned::to_owned),
        image_name: image
            .and_then(|image| image.get("name"))
            .and_then(|name| name.as_str())
            .map(ToOwned::to_owned),
        image_tag: image
            .and_then(|image| image.get("tag"))
            .and_then(|tag| tag.as_str())
            .map(ToOwned::to_owned),
        entrypoint: image
            .and_then(|image| image.get("entrypoint"))
            .and_then(|entrypoint| entrypoint.as_str())
            .map(ToOwned::to_owned),
        cmd: image
            .and_then(|image| image.get("cmd"))
            .and_then(|cmd| cmd.as_array())
            .map(|arr| strings_from_array(arr)),
        expose: image
            .and_then(|image| image.get("ports"))
            .or_else(|| image.and_then(|image| image.get("expose")))
            .and_then(|ports| ports.as_array())
            .map(|arr| ports_from_array(arr)),
        agent_role: agent
            .and_then(|agent| agent.get("role"))
            .and_then(|role| role.as_str())
            .map(ToOwned::to_owned),
        agent_mode: agent
            .and_then(|agent| agent.get("mode"))
            .and_then(|mode| mode.as_str())
            .map(ToOwned::to_owned),
        agent_posture: agent
            .and_then(|agent| agent.get("posture"))
            .and_then(|posture| posture.as_str())
            .map(ToOwned::to_owned),
        agent_model: agent
            .and_then(|agent| agent.get("model"))
            .and_then(|model| model.as_str())
            .map(ToOwned::to_owned),
    })
}

fn collect_packages(profile: &toml::Value) -> Vec<String> {
    profile
        .get("container")
        .and_then(|c| c.get("packages"))
        .and_then(|n| n.as_array())
        .or_else(|| {
            profile
                .get("packages")
                .and_then(|p| p.get("nix"))
                .and_then(|n| n.as_array())
        })
        .map(|arr| strings_from_array(arr))
        .unwrap_or_default()
}

fn collect_env_vars(profile: &toml::Value) -> Vec<(String, String)> {
    profile
        .get("shell")
        .and_then(|s| s.get("env"))
        .and_then(|e| e.as_table())
        .map(|t| {
            t.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

fn strings_from_array(arr: &[toml::Value]) -> Vec<String> {
    arr.iter()
        .filter_map(|v| v.as_str().map(ToOwned::to_owned))
        .collect()
}

fn ports_from_array(arr: &[toml::Value]) -> Vec<u16> {
    arr.iter()
        .filter_map(|v| v.as_integer().and_then(|i| u16::try_from(i).ok()))
        .collect()
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
    "podman"
}

struct ImageConfig<'a> {
    name: &'a str,
    tag: &'a str,
    packages: &'a [String],
    env_vars: &'a [(String, String)],
    entrypoint: &'a str,
    expose: &'a [u16],
    user: Option<&'a str>,
    workdir: &'a str,
    cmd: &'a [String],
    labels: &'a [(String, String)],
}

/// Generate a Nix expression that builds an OCI image using dockerTools.
#[allow(clippy::too_many_arguments)]
fn generate_image_nix(cfg: &ImageConfig<'_>) -> String {
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
    nix.push_str(&format!("  name = {};\n", nix_string(cfg.name)));
    nix.push_str(&format!("  tag = {};\n", nix_string(cfg.tag)));
    nix.push('\n');

    nix.push_str("  contents = with pkgs; [\n");
    nix.push_str("    bashInteractive\n");
    nix.push_str("    coreutils\n");
    nix.push_str("    cacert\n");
    for pkg in cfg.packages {
        if pkg != "bash" && pkg != "coreutils" {
            nix.push_str(&format!("    {pkg}\n"));
        }
    }
    nix.push_str("  ];\n");
    nix.push('\n');

    nix.push_str("  config = {\n");
    nix.push_str(&format!(
        "    Entrypoint = [ {} ];\n",
        nix_string(cfg.entrypoint)
    ));

    if !cfg.cmd.is_empty() {
        let cmd_str = cfg
            .cmd
            .iter()
            .map(|c| nix_string(c))
            .collect::<Vec<_>>()
            .join(" ");
        nix.push_str(&format!("    Cmd = [ {cmd_str} ];\n"));
    }

    let mut env_lines = vec![
        nix_string("PATH=/bin:/usr/bin"),
        nix_string("SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt"),
    ];
    for (key, val) in cfg.env_vars {
        env_lines.push(nix_string(&format!("{key}={val}")));
    }
    nix.push_str("    Env = [\n");
    for env in &env_lines {
        nix.push_str(&format!("      {env}\n"));
    }
    nix.push_str("    ];\n");

    if !cfg.expose.is_empty() {
        nix.push_str("    ExposedPorts = {\n");
        for port in cfg.expose {
            nix.push_str(&format!(
                "      {} = {{}};\n",
                nix_string(&format!("{port}/tcp"))
            ));
        }
        nix.push_str("    };\n");
    }

    if !cfg.labels.is_empty() {
        nix.push_str("    Labels = {\n");
        for (key, value) in cfg.labels {
            nix.push_str(&format!(
                "      {} = {};\n",
                nix_string(key),
                nix_string(value)
            ));
        }
        nix.push_str("    };\n");
    }

    nix.push_str(&format!("    WorkingDir = {};\n", nix_string(cfg.workdir)));

    if let Some(u) = cfg.user {
        nix.push_str(&format!("    User = {};\n", nix_string(u)));
    }

    nix.push_str("  };\n");
    nix.push_str(&format!(
        "\n  extraCommands = ''\n    mkdir -p {}\n  '';\n",
        shell_single_quote(cfg.workdir)
    ));
    nix.push_str("}\n");

    nix
}

fn nix_string(value: &str) -> String {
    format!("{value:?}")
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn image_tarball_name(image_name: &str, image_tag: &str) -> String {
    let safe_name: String = image_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let safe_tag: String = image_tag
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("{safe_name}-{safe_tag}.tar.gz")
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_image_nix_basic() {
        let env = vec![("EDITOR".to_string(), "vim".to_string())];
        let packages = vec!["git".to_string(), "ripgrep".to_string()];
        let labels = vec![(
            "io.styrene.package.name".to_string(),
            "styrene.agent.test".to_string(),
        )];
        let nix = generate_image_nix(&ImageConfig {
            name: "test-image",
            tag: "v1",
            packages: &packages,
            env_vars: &env,
            entrypoint: "/bin/bash",
            expose: &[8080],
            user: Some("app"),
            workdir: "/workspace",
            cmd: &[],
            labels: &labels,
        });
        assert!(nix.contains("name = \"test-image\""));
        assert!(nix.contains("tag = \"v1\""));
        assert!(nix.contains("git"));
        assert!(nix.contains("ripgrep"));
        assert!(nix.contains("EDITOR=vim"));
        assert!(nix.contains("8080/tcp"));
        assert!(nix.contains("User = \"app\""));
        assert!(nix.contains("\"io.styrene.package.name\" = \"styrene.agent.test\""));
        assert!(nix.contains("buildLayeredImage"));
    }

    #[test]
    fn test_generate_image_nix_minimal() {
        let nix = generate_image_nix(&ImageConfig {
            name: "minimal",
            tag: "latest",
            packages: &[],
            env_vars: &[],
            entrypoint: "/bin/bash",
            expose: &[],
            user: None,
            workdir: "/",
            cmd: &[],
            labels: &[],
        });
        assert!(nix.contains("bashInteractive"));
        assert!(nix.contains("coreutils"));
        assert!(nix.contains("cacert"));
        assert!(!nix.contains("User ="));
        assert!(!nix.contains("ExposedPorts"));
    }

    #[test]
    fn test_generate_image_nix_with_cmd() {
        let cmd = vec!["-g".to_string(), "daemon off;".to_string()];
        let packages = vec!["nginx".to_string()];
        let nix = generate_image_nix(&ImageConfig {
            name: "server",
            tag: "latest",
            packages: &packages,
            env_vars: &[],
            entrypoint: "/bin/nginx",
            expose: &[80, 443],
            user: None,
            workdir: "/var/www",
            cmd: &cmd,
            labels: &[],
        });
        assert!(nix.contains("Entrypoint = [ \"/bin/nginx\" ]"));
        assert!(nix.contains("Cmd = [ \"-g\" \"daemon off;\" ]"));
        assert!(nix.contains("80/tcp"));
        assert!(nix.contains("443/tcp"));
    }

    #[test]
    fn test_generate_image_no_duplicate_bash() {
        let packages = vec![
            "bash".to_string(),
            "coreutils".to_string(),
            "git".to_string(),
        ];
        let nix = generate_image_nix(&ImageConfig {
            name: "test",
            tag: "latest",
            packages: &packages,
            env_vars: &[],
            entrypoint: "/bin/bash",
            expose: &[],
            user: None,
            workdir: "/",
            cmd: &[],
            labels: &[],
        });
        let bash_count = nix.matches("bashInteractive").count();
        assert_eq!(bash_count, 1);
    }

    #[test]
    fn test_image_tarball_name_sanitizes_registry_path() {
        assert_eq!(
            image_tarball_name("ghcr.io/styrene-lab/primary", "0.1.0"),
            "ghcr.io_styrene-lab_primary-0.1.0.tar.gz"
        );
    }

    #[test]
    fn test_parse_package_manifest() {
        let value: toml::Value = toml::from_str(
            r#"
[package]
name = "styrene.agent.primary"
version = "0.1.0"
source = "github:styrene-lab/packages/primary"

[nex]
profile = "./machine-profile.toml"

[image]
name = "ghcr.io/styrene-lab/primary"
tag = "0.1.0"
entrypoint = "/bin/omegon"
cmd = ["serve", "--control-plane", "0.0.0.0:7842"]
ports = [7842]

[agent]
role = "primary-driver"
mode = "daemon"
posture = "orchestrator"
model = "anthropic:claude-sonnet-4-6"
"#,
        )
        .unwrap();

        let package = parse_package_manifest(&value).unwrap();
        assert_eq!(package.package_name, "styrene.agent.primary");
        assert_eq!(
            package.image_name.as_deref(),
            Some("ghcr.io/styrene-lab/primary")
        );
        assert_eq!(package.image_tag.as_deref(), Some("0.1.0"));
        assert_eq!(package.entrypoint.as_deref(), Some("/bin/omegon"));
        assert_eq!(package.expose, Some(vec![7842]));
        assert_eq!(package.agent_role.as_deref(), Some("primary-driver"));
        assert_eq!(
            package.labels("./machine-profile.toml"),
            vec![
                (
                    "io.styrene.package.name".to_string(),
                    "styrene.agent.primary".to_string()
                ),
                (
                    "io.styrene.nex.profile".to_string(),
                    "./machine-profile.toml".to_string()
                ),
                (
                    "org.opencontainers.image.version".to_string(),
                    "0.1.0".to_string()
                ),
                (
                    "io.styrene.package.version".to_string(),
                    "0.1.0".to_string()
                ),
                (
                    "org.opencontainers.image.source".to_string(),
                    "github:styrene-lab/packages/primary".to_string()
                ),
                (
                    "io.styrene.agent.role".to_string(),
                    "primary-driver".to_string()
                ),
                ("io.styrene.agent.mode".to_string(), "daemon".to_string()),
                (
                    "io.styrene.agent.posture".to_string(),
                    "orchestrator".to_string()
                ),
                (
                    "io.styrene.agent.model".to_string(),
                    "anthropic:claude-sonnet-4-6".to_string()
                ),
            ]
        );
    }
}
