use anyhow::Result;
use serde::Serialize;

const CAPABILITIES_SCHEMA: &str = "io.styrene.nex.capabilities.v1";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CapabilitiesReport {
    schema: &'static str,
    version: &'static str,
    commands: Vec<Capability>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Capability {
    id: &'static str,
    argv: Vec<&'static str>,
    read_only: bool,
    mutates_host: bool,
    output_schema: Option<&'static str>,
    stability: &'static str,
}

pub fn run(json: bool) -> Result<()> {
    let report = CapabilitiesReport {
        schema: CAPABILITIES_SCHEMA,
        version: env!("CARGO_PKG_VERSION"),
        commands: vec![
            cap(
                "capabilities",
                &["capabilities", "--json"],
                true,
                false,
                Some(CAPABILITIES_SCHEMA),
                "stable",
            ),
            cap(
                "doctor.readiness",
                &["doctor", "--json"],
                true,
                false,
                Some("io.styrene.nex.host-readiness.v1"),
                "stable",
            ),
            cap(
                "devenv.inspect",
                &["devenv", "inspect", "<path>", "--json"],
                true,
                false,
                Some("io.styrene.nex.devenv-import-report.v1"),
                "stable",
            ),
            cap(
                "devenv.explain",
                &["devenv", "explain", "<path>", "--json"],
                true,
                false,
                Some("io.styrene.nex.devenv-import-report.v1"),
                "stable",
            ),
            cap(
                "devenv.plan",
                &["devenv", "plan", "<path>", "--json"],
                true,
                false,
                Some("io.styrene.nex.devenv-migration-plan.v1"),
                "stable",
            ),
            cap(
                "devenv.catalog.list",
                &["devenv", "catalog", "list", "--json"],
                true,
                false,
                Some("io.styrene.nex.devenv-surface-catalog-report.v1"),
                "stable",
            ),
            cap(
                "hardware.scan",
                &["hardware", "scan", "--json"],
                true,
                false,
                Some("io.styrene.nex.hardware-inventory.v1"),
                "stable",
            ),
            cap(
                "hardware.attest",
                &["hardware", "attest", "--disk", "<disk>", "--json"],
                true,
                false,
                Some("io.styrene.nex.disk-attestation.v1"),
                "stable",
            ),
            cap(
                "hardware.match",
                &["hardware", "match", "--json"],
                true,
                false,
                Some("io.styrene.nex.hardware-profile-match.v1"),
                "stable",
            ),
            cap(
                "machine-profile.inspect",
                &["machine-profile", "inspect", "<path>", "--json"],
                true,
                false,
                None,
                "provisional",
            ),
            cap(
                "profile-fragment.inspect",
                &["profile-fragment", "inspect", "<path>", "--json"],
                true,
                false,
                None,
                "provisional",
            ),
        ],
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("Nex capabilities ({})", report.version);
        for command in report.commands {
            println!("  {} -> {}", command.id, command.argv.join(" "));
        }
    }
    Ok(())
}

fn cap(
    id: &'static str,
    argv: &[&'static str],
    read_only: bool,
    mutates_host: bool,
    output_schema: Option<&'static str>,
    stability: &'static str,
) -> Capability {
    Capability {
        id,
        argv: argv.to_vec(),
        read_only,
        mutates_host,
        output_schema,
        stability,
    }
}
