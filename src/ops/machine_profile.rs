use std::path::Path;

use anyhow::Result;
use serde::Serialize;

use crate::machine_profile::{MachineProfileDocument, MACHINE_PROFILE_FILE};

pub fn run_validate(path: &Path) -> Result<()> {
    MachineProfileDocument::from_path(path)?;
    eprintln!("{} is valid", display_input(path));
    Ok(())
}

pub fn run_inspect(path: &Path, json: bool) -> Result<()> {
    let document = MachineProfileDocument::from_path(path)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&MachineProfileInspect::from(&document))?);
        return Ok(());
    }

    let profile = &document.machine_profile;

    print_section("Machine Profile");
    print_kv("ID", &profile.id);
    print_kv("Slug", &profile.slug);
    print_kv("Name", &profile.name);
    print_kv("Version", &profile.version);
    print_kv("Schema", &profile.schema);
    print_kv("Min Nex", &profile.min_nex);
    if let Some(description) = &profile.description {
        print_kv("Description", description);
    }
    if let Some(license) = &profile.license {
        print_kv("License", license);
    }

    print_section("Defaults");
    print_kv("Mode", &profile.defaults.mode.to_string());
    print_kv("Target", &profile.defaults.target.to_string());

    print_section("Safety");
    print_kv(
        "Default destructive",
        &profile.safety.default_destructive.to_string(),
    );
    print_kv(
        "Requires confirmation",
        &profile.safety.requires_confirmation.to_string(),
    );
    print_kv(
        "Requires target attestation",
        &profile.safety.requires_target_attestation.to_string(),
    );
    let allowed_targets = profile
        .safety
        .allowed_targets
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    print_kv("Allowed targets", &allowed_targets);

    if let Some(secrets) = &profile.secrets {
        print_section("Secrets");
        print_kv("Required", &secrets.required.join(", "));
        print_kv("Optional", &secrets.optional.join(", "));
    }

    if !document.dependencies.is_empty() {
        print_section("Dependencies");
        for dependency in &document.dependencies {
            print_kv(
                &format!("{}:{}", dependency.kind, dependency.id),
                &format!("{} required={}", dependency.version, dependency.required),
            );
        }
    }

    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MachineProfileInspect {
    kind: &'static str,
    schema: String,
    id: String,
    slug: String,
    name: String,
    version: String,
    description: Option<String>,
    license: Option<String>,
    min_nex: String,
    defaults: MachineProfileDefaultsInspect,
    safety: MachineProfileSafetyInspect,
    secrets: MachineProfileSecretsInspect,
    dependencies: Vec<MachineProfileDependencyInspect>,
}

#[derive(Serialize)]
struct MachineProfileDefaultsInspect {
    mode: String,
    target: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MachineProfileSafetyInspect {
    default_destructive: bool,
    requires_confirmation: bool,
    requires_target_attestation: bool,
    allowed_targets: Vec<String>,
}

#[derive(Serialize)]
struct MachineProfileSecretsInspect {
    required: Vec<String>,
    optional: Vec<String>,
}

#[derive(Serialize)]
struct MachineProfileDependencyInspect {
    kind: String,
    id: String,
    version: String,
    required: bool,
}

impl From<&MachineProfileDocument> for MachineProfileInspect {
    fn from(document: &MachineProfileDocument) -> Self {
        let profile = &document.machine_profile;
        let secrets = profile.secrets.as_ref();
        Self {
            kind: "machine-profile",
            schema: profile.schema.clone(),
            id: profile.id.clone(),
            slug: profile.slug.clone(),
            name: profile.name.clone(),
            version: profile.version.clone(),
            description: profile.description.clone(),
            license: profile.license.clone(),
            min_nex: profile.min_nex.clone(),
            defaults: MachineProfileDefaultsInspect {
                mode: profile.defaults.mode.to_string(),
                target: profile.defaults.target.to_string(),
            },
            safety: MachineProfileSafetyInspect {
                default_destructive: profile.safety.default_destructive,
                requires_confirmation: profile.safety.requires_confirmation,
                requires_target_attestation: profile.safety.requires_target_attestation,
                allowed_targets: profile
                    .safety
                    .allowed_targets
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
            },
            secrets: MachineProfileSecretsInspect {
                required: secrets.map(|s| s.required.clone()).unwrap_or_default(),
                optional: secrets.map(|s| s.optional.clone()).unwrap_or_default(),
            },
            dependencies: document
                .dependencies
                .iter()
                .map(|dependency| MachineProfileDependencyInspect {
                    kind: dependency.kind.to_string(),
                    id: dependency.id.clone(),
                    version: dependency.version.clone(),
                    required: dependency.required,
                })
                .collect(),
        }
    }
}

fn print_section(label: &str) {
    println!("\n{label}");
}

fn print_kv(key: &str, value: &str) {
    println!("  {key}: {value}");
}

fn display_input(path: &Path) -> String {
    if path.is_dir() {
        path.join(MACHINE_PROFILE_FILE).display().to_string()
    } else {
        path.display().to_string()
    }
}
