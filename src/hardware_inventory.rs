use std::process::Command;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

pub const HARDWARE_INVENTORY_SCHEMA_V1: &str = "io.styrene.nex.hardware-inventory.v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum HardwarePlatform {
    Darwin,
    Linux,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HardwareInventory {
    pub schema: String,
    pub platform: HardwarePlatform,
    pub arch: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vendor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu: Option<HardwareCpuSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<HardwareMemorySummary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disks: Vec<HardwareDisk>,
    pub evidence: HardwareEvidence,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<HardwareWarning>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HardwareCpuSummary {
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HardwareMemorySummary {
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiskAttestationReport {
    pub schema: String,
    pub disk: HardwareDisk,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HardwareDisk {
    pub id: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub whole_disk: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub internal: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub removable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ejectable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub solid_state: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bus: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vendor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_name: Option<String>,
    pub classification: DiskClassification,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_sources: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiskClassification {
    pub target_attestation: TargetAttestationCandidate,
    pub destructive_default: DestructiveDefault,
    pub confidence: ClassificationConfidence,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TargetAttestationCandidate {
    ExternalUsbSsd,
    ExternalThunderboltSsd,
    InternalAppleStorage,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DestructiveDefault {
    AllowedWithAttestation,
    Forbidden,
    RequiresOperatorAttestation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ClassificationConfidence {
    Strong,
    Weak,
    Unknown,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct HardwareEvidence {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HardwareWarning {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiskEvidence {
    pub id: String,
    pub path: String,
    pub whole_disk: Option<bool>,
    pub size_bytes: Option<u64>,
    pub internal: Option<bool>,
    pub removable: Option<bool>,
    pub ejectable: Option<bool>,
    pub solid_state: Option<bool>,
    pub bus: Option<String>,
    pub media_name: Option<String>,
    pub io_registry_entry_name: Option<String>,
    pub device_tree_path: Option<String>,
    pub evidence_sources: Vec<String>,
}

pub fn scan_host() -> Result<HardwareInventory> {
    if cfg!(target_os = "macos") {
        return collect_darwin_inventory();
    }
    Ok(degraded_inventory(
        HardwarePlatform::Unknown,
        "UNSUPPORTED_PLATFORM",
        "Live hardware scanning is not implemented for this platform yet.",
    ))
}

pub fn attest_disk(disk: &str) -> Result<DiskAttestationReport> {
    let inventory = scan_host()?;
    let normalized = normalize_disk_selector(disk);
    let disk = inventory
        .disks
        .into_iter()
        .find(|candidate| {
            candidate.id == normalized
                || candidate.path == disk
                || candidate.path.strip_prefix("/dev/") == Some(normalized.as_str())
        })
        .with_context(|| format!("disk {disk:?} was not found in hardware inventory"))?;
    Ok(DiskAttestationReport {
        schema: "io.styrene.nex.disk-attestation.v1".to_string(),
        disk,
    })
}

fn normalize_disk_selector(disk: &str) -> String {
    disk.strip_prefix("/dev/").unwrap_or(disk).to_string()
}

fn degraded_inventory(
    platform: HardwarePlatform,
    code: impl Into<String>,
    message: impl Into<String>,
) -> HardwareInventory {
    HardwareInventory {
        schema: HARDWARE_INVENTORY_SCHEMA_V1.to_string(),
        platform,
        arch: std::env::consts::ARCH.to_string(),
        vendor: None,
        model: None,
        model_name: None,
        cpu: None,
        memory: None,
        disks: Vec::new(),
        evidence: HardwareEvidence::default(),
        warnings: vec![HardwareWarning {
            code: code.into(),
            message: message.into(),
        }],
    }
}

fn collect_darwin_inventory() -> Result<HardwareInventory> {
    let hardware_json = run_capture("system_profiler", &["SPHardwareDataType", "-json"])?;
    let hardware = parse_system_profiler_hardware_json(&hardware_json)?;

    let disk_list_plist = run_capture("diskutil", &["list", "-plist"])?;
    let whole_disks = parse_diskutil_whole_disks(&disk_list_plist)?;
    let mut disks = Vec::new();
    let mut warnings = Vec::new();
    for disk in whole_disks {
        match run_capture("diskutil", &["info", "-plist", disk.as_str()]) {
            Ok(info) => match parse_diskutil_info_plist(&info) {
                Ok(disk) => disks.push(disk),
                Err(error) => warnings.push(HardwareWarning {
                    code: "DISKUTIL_INFO_PARSE_FAILED".to_string(),
                    message: format!("Failed to parse diskutil info for {disk}: {error:#}"),
                }),
            },
            Err(error) => warnings.push(HardwareWarning {
                code: "DISKUTIL_INFO_FAILED".to_string(),
                message: format!("Failed to inspect {disk}: {error:#}"),
            }),
        }
    }

    Ok(HardwareInventory {
        schema: HARDWARE_INVENTORY_SCHEMA_V1.to_string(),
        platform: HardwarePlatform::Darwin,
        arch: std::env::consts::ARCH.to_string(),
        vendor: Some("Apple".to_string()),
        model: hardware.machine_model,
        model_name: hardware.machine_name,
        cpu: hardware
            .chip_type
            .map(|summary| HardwareCpuSummary { summary }),
        memory: hardware
            .physical_memory
            .map(|summary| HardwareMemorySummary { summary }),
        disks,
        evidence: HardwareEvidence {
            commands: vec![
                "system_profiler SPHardwareDataType -json".to_string(),
                "diskutil list -plist".to_string(),
                "diskutil info -plist <disk>".to_string(),
            ],
        },
        warnings,
    })
}

fn run_capture(program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("running {program}"))?;
    if !output.status.success() {
        bail!(
            "{} {} failed with status {}: {}",
            program,
            args.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    String::from_utf8(output.stdout).with_context(|| format!("decoding {program} stdout as UTF-8"))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DiskutilList {
    #[serde(default, rename = "AllDisksAndPartitions")]
    all_disks_and_partitions: Vec<DiskutilListDisk>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DiskutilListDisk {
    #[serde(rename = "DeviceIdentifier")]
    device_identifier: String,
    #[serde(default, rename = "Content")]
    content: Option<String>,
}

pub fn parse_diskutil_whole_disks(plist: &str) -> Result<Vec<String>> {
    let list: DiskutilList =
        plist::from_bytes(plist.as_bytes()).context("decoding diskutil list plist")?;
    Ok(list
        .all_disks_and_partitions
        .into_iter()
        .filter(|disk| {
            disk.content
                .as_deref()
                .is_some_and(|content| content.ends_with("partition_scheme"))
        })
        .map(|disk| disk.device_identifier)
        .collect())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SystemProfilerHardware {
    #[serde(rename = "SPHardwareDataType")]
    sp_hardware_data_type: Vec<SystemProfilerHardwareItem>,
}

#[derive(Debug, Deserialize)]
struct SystemProfilerHardwareItem {
    machine_model: Option<String>,
    machine_name: Option<String>,
    chip_type: Option<String>,
    physical_memory: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DarwinHardwareSummary {
    pub machine_model: Option<String>,
    pub machine_name: Option<String>,
    pub chip_type: Option<String>,
    pub physical_memory: Option<String>,
}

pub fn parse_system_profiler_hardware_json(input: &str) -> Result<DarwinHardwareSummary> {
    let parsed: SystemProfilerHardware =
        serde_json::from_str(input).context("decoding system_profiler hardware JSON")?;
    let item = parsed
        .sp_hardware_data_type
        .into_iter()
        .next()
        .context("system_profiler hardware JSON did not contain SPHardwareDataType")?;
    Ok(DarwinHardwareSummary {
        machine_model: item.machine_model,
        machine_name: item.machine_name,
        chip_type: item.chip_type,
        physical_memory: item.physical_memory,
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DiskutilInfo {
    #[serde(rename = "DeviceIdentifier")]
    device_identifier: String,
    #[serde(rename = "DeviceNode")]
    device_node: Option<String>,
    #[serde(rename = "WholeDisk")]
    whole_disk: Option<bool>,
    #[serde(rename = "Size")]
    size: Option<u64>,
    #[serde(rename = "Internal")]
    internal: Option<bool>,
    #[serde(rename = "RemovableMedia")]
    removable_media: Option<bool>,
    #[serde(rename = "Ejectable")]
    ejectable: Option<bool>,
    #[serde(rename = "SolidState")]
    solid_state: Option<bool>,
    #[serde(rename = "BusProtocol")]
    bus_protocol: Option<String>,
    #[serde(rename = "MediaName")]
    media_name: Option<String>,
    #[serde(rename = "IORegistryEntryName")]
    io_registry_entry_name: Option<String>,
    #[serde(rename = "DeviceTreePath")]
    device_tree_path: Option<String>,
}

pub fn parse_diskutil_info_plist(input: &str) -> Result<HardwareDisk> {
    let info: DiskutilInfo =
        plist::from_bytes(input.as_bytes()).context("decoding diskutil info plist")?;
    let evidence = DiskEvidence {
        id: info.device_identifier.clone(),
        path: info
            .device_node
            .clone()
            .unwrap_or_else(|| format!("/dev/{}", info.device_identifier)),
        whole_disk: info.whole_disk,
        size_bytes: info.size,
        internal: info.internal,
        removable: info.removable_media,
        ejectable: info.ejectable,
        solid_state: info.solid_state,
        bus: info.bus_protocol,
        media_name: info.media_name,
        io_registry_entry_name: info.io_registry_entry_name,
        device_tree_path: info.device_tree_path,
        evidence_sources: vec!["diskutil-info-plist".to_string()],
    };
    Ok(HardwareDisk {
        id: evidence.id.clone(),
        path: evidence.path.clone(),
        whole_disk: evidence.whole_disk,
        size_bytes: evidence.size_bytes,
        internal: evidence.internal,
        removable: evidence.removable,
        ejectable: evidence.ejectable,
        solid_state: evidence.solid_state,
        bus: evidence.bus.clone(),
        vendor: apple_storage_evidence(&evidence).then(|| "Apple".to_string()),
        model: evidence.media_name.clone(),
        media_name: evidence.media_name.clone(),
        classification: classify_disk(&evidence),
        evidence_sources: evidence.evidence_sources,
    })
}

pub fn classify_disk(evidence: &DiskEvidence) -> DiskClassification {
    let bus = evidence
        .bus
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if evidence.internal == Some(true) && apple_storage_evidence(evidence) {
        return DiskClassification {
            target_attestation: TargetAttestationCandidate::InternalAppleStorage,
            destructive_default: DestructiveDefault::Forbidden,
            confidence: ClassificationConfidence::Strong,
            reasons: vec![
                "disk is internal".to_string(),
                "disk evidence identifies Apple internal storage".to_string(),
            ],
        };
    }

    if evidence.internal == Some(false)
        && evidence.solid_state == Some(true)
        && (evidence.removable == Some(true) || evidence.ejectable == Some(true))
        && bus.contains("usb")
    {
        return DiskClassification {
            target_attestation: TargetAttestationCandidate::ExternalUsbSsd,
            destructive_default: DestructiveDefault::AllowedWithAttestation,
            confidence: ClassificationConfidence::Strong,
            reasons: vec![
                "disk is external or removable".to_string(),
                "bus protocol is USB".to_string(),
                "disk reports solid-state media".to_string(),
            ],
        };
    }

    if evidence.internal == Some(false)
        && evidence.solid_state == Some(true)
        && bus.contains("thunderbolt")
    {
        return DiskClassification {
            target_attestation: TargetAttestationCandidate::ExternalThunderboltSsd,
            destructive_default: DestructiveDefault::AllowedWithAttestation,
            confidence: ClassificationConfidence::Strong,
            reasons: vec![
                "disk is external".to_string(),
                "bus protocol is Thunderbolt".to_string(),
                "disk reports solid-state media".to_string(),
            ],
        };
    }

    DiskClassification {
        target_attestation: TargetAttestationCandidate::Unknown,
        destructive_default: DestructiveDefault::RequiresOperatorAttestation,
        confidence: ClassificationConfidence::Unknown,
        reasons: vec!["disk evidence is insufficient for safe target attestation".to_string()],
    }
}

fn apple_storage_evidence(evidence: &DiskEvidence) -> bool {
    let haystack = [
        evidence.bus.as_deref(),
        evidence.media_name.as_deref(),
        evidence.io_registry_entry_name.as_deref(),
        evidence.device_tree_path.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" ")
    .to_ascii_lowercase();
    haystack.contains("apple fabric")
        || haystack.contains("apple ssd")
        || haystack.contains("appleans")
        || haystack.contains("ans@")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SYSTEM_PROFILER_HARDWARE: &str = r#"{
      "SPHardwareDataType": [
        {
          "_name": "hardware_overview",
          "chip_type": "Apple M5 Max",
          "machine_model": "Mac17,7",
          "machine_name": "MacBook Pro",
          "physical_memory": "128 GB",
          "platform_UUID": "REDACTED",
          "serial_number": "REDACTED"
        }
      ]
    }"#;

    const DISKUTIL_INFO_INTERNAL_APPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
    <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
    <plist version="1.0"><dict>
      <key>DeviceIdentifier</key><string>disk0</string>
      <key>DeviceNode</key><string>/dev/disk0</string>
      <key>WholeDisk</key><true/>
      <key>Size</key><integer>4002222325760</integer>
      <key>Internal</key><true/>
      <key>SolidState</key><true/>
      <key>BusProtocol</key><string>Apple Fabric</string>
      <key>MediaName</key><string>APPLE SSD AP4096Z</string>
      <key>IORegistryEntryName</key><string>APPLE SSD AP4096Z Media</string>
      <key>DeviceTreePath</key><string>IODeviceTree:/arm-io@10F00000/ans@19600000/iop-ans-nub/AppleANS3CGv2Controller</string>
      <key>RemovableMedia</key><false/>
      <key>Ejectable</key><false/>
    </dict></plist>"#;

    #[test]
    fn parses_system_profiler_hardware_summary_without_sensitive_fields() -> Result<()> {
        let summary = parse_system_profiler_hardware_json(SYSTEM_PROFILER_HARDWARE)?;

        assert_eq!(summary.machine_model.as_deref(), Some("Mac17,7"));
        assert_eq!(summary.machine_name.as_deref(), Some("MacBook Pro"));
        assert_eq!(summary.chip_type.as_deref(), Some("Apple M5 Max"));
        assert_eq!(summary.physical_memory.as_deref(), Some("128 GB"));
        Ok(())
    }

    #[test]
    fn classifies_internal_apple_fabric_storage_as_forbidden() -> Result<()> {
        let disk = parse_diskutil_info_plist(DISKUTIL_INFO_INTERNAL_APPLE)?;

        assert_eq!(disk.id, "disk0");
        assert_eq!(disk.internal, Some(true));
        assert_eq!(
            disk.classification.target_attestation,
            TargetAttestationCandidate::InternalAppleStorage
        );
        assert_eq!(
            disk.classification.destructive_default,
            DestructiveDefault::Forbidden
        );
        assert_eq!(
            disk.classification.confidence,
            ClassificationConfidence::Strong
        );
        Ok(())
    }

    #[test]
    fn attestation_selector_normalizes_dev_paths() {
        assert_eq!(normalize_disk_selector("/dev/disk4"), "disk4");
        assert_eq!(normalize_disk_selector("disk4"), "disk4");
    }
}
