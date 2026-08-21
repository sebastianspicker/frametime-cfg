use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
mod cs2_config;
mod interrupt;
mod network_stack;
pub use cs2_config::{
    CS2_CONFIG_MAX_FILE_BYTES, CS2_CONFIG_MAX_TOTAL_BYTES, CS2_CONFIG_TRANSACTION_STEP,
    Cs2ConfigBackupError, Cs2ConfigSnapshot, Cs2InstallIdentity,
};
pub use interrupt::{
    InterruptPolicyBackup, InterruptPolicyBackupError, InterruptPolicyKind, InterruptPolicyValue,
};
pub use network_stack::{
    NETWORK_STACK_TRANSACTION_STEP, NetworkStackBackupError, NetworkStackNlaBackup,
    NetworkStackPolicy, NetworkStackPolicyBackup, NetworkStackPolicySnapshot,
    NetworkStackRawRegistryValue, NetworkStackSetting, NetworkStackSettingBackup,
    NetworkStackTransaction, NetworkStackValue,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BackupFile {
    #[serde(default)]
    pub entries: Vec<BackupEntry>,
    pub created: String,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BackupEntry {
    Registry {
        step: String,
        timestamp: String,
        path: String,
        name: String,
        #[serde(rename = "originalValue")]
        original_value: Value,
        #[serde(rename = "originalType")]
        original_type: Option<String>,
        existed: bool,
        #[serde(flatten)]
        unknown: BTreeMap<String, Value>,
    },
    /// P1:7's fixed, global HwSchMode transaction.  This is deliberately not
    /// represented as a generic registry record: the pending WDDM check must
    /// survive the reboot which makes the registry request effective.
    Hags {
        step: String,
        timestamp: String,
        #[serde(rename = "originalValue")]
        original_value: Option<u32>,
        #[serde(rename = "targetValue")]
        target_value: u32,
        #[serde(rename = "adapterIds")]
        adapter_ids: Vec<String>,
        #[serde(rename = "effectiveVerificationPending")]
        effective_verification_pending: bool,
        #[serde(flatten)]
        unknown: BTreeMap<String, Value>,
    },
    Service {
        step: String,
        timestamp: String,
        name: String,
        #[serde(rename = "originalStartType")]
        original_start_type: String,
        #[serde(rename = "delayedAutoStart")]
        delayed_auto_start: bool,
        #[serde(rename = "originalStatus")]
        original_status: String,
        #[serde(flatten)]
        unknown: BTreeMap<String, Value>,
    },
    Powerplan {
        step: String,
        timestamp: String,
        #[serde(rename = "originalGuid")]
        original_guid: String,
        #[serde(rename = "originalName")]
        original_name: Option<String>,
        #[serde(rename = "suiteOwnedGuids", default)]
        suite_owned_guids: Vec<String>,
        #[serde(flatten)]
        unknown: BTreeMap<String, Value>,
    },
    Bootconfig {
        step: String,
        timestamp: String,
        key: String,
        #[serde(rename = "originalValue")]
        original_value: Value,
        existed: bool,
        #[serde(flatten)]
        unknown: BTreeMap<String, Value>,
    },
    Scheduledtask {
        step: String,
        timestamp: String,
        #[serde(rename = "taskName")]
        task_name: String,
        #[serde(rename = "taskPath")]
        task_path: String,
        existed: bool,
        #[serde(rename = "wasEnabled")]
        was_enabled: bool,
        #[serde(rename = "scriptPath", default)]
        script_path: Option<String>,
        #[serde(flatten)]
        unknown: BTreeMap<String, Value>,
    },
    NicAdapter {
        step: String,
        timestamp: String,
        #[serde(rename = "adapterName")]
        adapter_name: String,
        #[serde(rename = "interfaceDescription")]
        interface_description: String,
        #[serde(rename = "propertyName")]
        property_name: String,
        #[serde(rename = "originalValue")]
        original_value: Value,
        #[serde(rename = "propertyType")]
        property_type: String,
        #[serde(flatten)]
        unknown: BTreeMap<String, Value>,
    },
    QosUro {
        step: String,
        timestamp: String,
        policies: Vec<String>,
        #[serde(rename = "uroState")]
        uro_state: Value,
        #[serde(flatten)]
        unknown: BTreeMap<String, Value>,
    },
    Defender {
        step: String,
        timestamp: String,
        #[serde(rename = "exclusionPaths", default)]
        exclusion_paths: Vec<String>,
        #[serde(rename = "exclusionProcesses", default)]
        exclusion_processes: Vec<String>,
        #[serde(flatten)]
        unknown: BTreeMap<String, Value>,
    },
    Pagefile {
        step: String,
        timestamp: String,
        #[serde(rename = "automaticManaged")]
        automatic_managed: bool,
        #[serde(rename = "pagefilePath")]
        pagefile_path: String,
        #[serde(rename = "initialSize")]
        initial_size: u64,
        #[serde(rename = "maximumSize")]
        maximum_size: u64,
        #[serde(flatten)]
        unknown: BTreeMap<String, Value>,
    },
    PagefileTransaction {
        step: String,
        timestamp: String,
        #[serde(rename = "automaticManaged")]
        automatic_managed: bool,
        #[serde(rename = "targetPath")]
        target_path: String,
        #[serde(rename = "targetExisted")]
        target_existed: bool,
        settings: Vec<PagefileTransactionSetting>,
        #[serde(
            default,
            rename = "computerObjectPath",
            skip_serializing_if = "Option::is_none"
        )]
        computer_object_path: Option<String>,
        #[serde(
            default,
            rename = "computerRelativePath",
            skip_serializing_if = "Option::is_none"
        )]
        computer_relative_path: Option<String>,
        #[serde(
            default,
            rename = "createdObjectPath",
            skip_serializing_if = "Option::is_none"
        )]
        created_object_path: Option<String>,
        #[serde(
            default,
            rename = "createdRelativePath",
            skip_serializing_if = "Option::is_none"
        )]
        created_relative_path: Option<String>,
        #[serde(
            default,
            rename = "createdInitialSize",
            skip_serializing_if = "Option::is_none"
        )]
        created_initial_size: Option<u64>,
        #[serde(
            default,
            rename = "createdMaximumSize",
            skip_serializing_if = "Option::is_none"
        )]
        created_maximum_size: Option<u64>,
        #[serde(
            default,
            rename = "mutationIntent",
            skip_serializing_if = "Option::is_none"
        )]
        mutation_intent: Option<String>,
        #[serde(flatten)]
        unknown: BTreeMap<String, Value>,
    },
    Dns {
        step: String,
        timestamp: String,
        #[serde(rename = "adapterName")]
        adapter_name: String,
        #[serde(rename = "interfaceIndex")]
        interface_index: u32,
        /// Durable IP Helper identity. Legacy name/index-only records are
        /// retained but cannot authorize native P3:9 recovery.
        #[serde(
            rename = "adapterGuid",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        adapter_guid: Option<String>,
        #[serde(
            rename = "interfaceGuid",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        interface_guid: Option<String>,
        #[serde(
            rename = "interfaceLuid",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        interface_luid: Option<u64>,
        #[serde(
            rename = "physicalAddress",
            default,
            skip_serializing_if = "Vec::is_empty"
        )]
        physical_address: Vec<u8>,
        #[serde(rename = "originalDnsServers", default)]
        original_dns_servers: Vec<String>,
        #[serde(flatten)]
        unknown: BTreeMap<String, Value>,
    },
    /// Exact, device-bound recovery state for P3:2/P3:3 interrupt policy.
    InterruptPolicy {
        #[serde(flatten)]
        backup: Box<InterruptPolicyBackup>,
    },
    /// Lossless, exact P1:16 state authorized by a native adapter binding.
    NetworkStackTransaction {
        #[serde(flatten)]
        transaction: Box<NetworkStackTransaction>,
    },
    Drs {
        step: String,
        timestamp: String,
        profile: String,
        #[serde(rename = "profileCreated")]
        profile_created: bool,
        settings: Vec<DrsSetting>,
        #[serde(rename = "applicationBindings", default)]
        application_bindings: Vec<DrsApplicationBinding>,
        #[serde(flatten)]
        unknown: BTreeMap<String, Value>,
    },
    /// Complete, path-free pre-mutation state for the closed P1:34 CFG write set.
    Cs2ConfigTransaction {
        step: String,
        timestamp: String,
        #[serde(rename = "installIdentity")]
        install_identity: Cs2InstallIdentity,
        targets: Vec<Cs2ConfigSnapshot>,
        #[serde(flatten)]
        unknown: BTreeMap<String, Value>,
    },
    #[serde(untagged)]
    Unknown(Value),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DrsSetting {
    pub id: Value,
    #[serde(rename = "previousValue")]
    pub previous_value: Value,
    pub existed: bool,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}

/// Exact pre-mutation ownership for a CS2 DRS executable registration.
/// `original_profile: None` means the suite added that registration and must
/// remove only that registration during recovery.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DrsApplicationBinding {
    pub application: String,
    #[serde(
        rename = "originalProfile",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub original_profile: Option<String>,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}

/// One original pagefile setting captured as part of a complete CIM transaction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PagefileTransactionSetting {
    pub path: String,
    #[serde(rename = "initialSize")]
    pub initial_size: u64,
    #[serde(rename = "maximumSize")]
    pub maximum_size: u64,
    #[serde(
        default,
        rename = "objectPath",
        skip_serializing_if = "Option::is_none"
    )]
    pub object_path: Option<String>,
    #[serde(
        default,
        rename = "relativePath",
        skip_serializing_if = "Option::is_none"
    )]
    pub relative_path: Option<String>,
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}

impl BackupEntry {
    #[must_use]
    pub fn step(&self) -> Option<&str> {
        match self {
            Self::Registry { step, .. }
            | Self::Hags { step, .. }
            | Self::Service { step, .. }
            | Self::Powerplan { step, .. }
            | Self::Bootconfig { step, .. }
            | Self::Scheduledtask { step, .. }
            | Self::NicAdapter { step, .. }
            | Self::QosUro { step, .. }
            | Self::Defender { step, .. }
            | Self::Pagefile { step, .. }
            | Self::PagefileTransaction { step, .. }
            | Self::Dns { step, .. }
            | Self::Drs { step, .. }
            | Self::Cs2ConfigTransaction { step, .. } => Some(step),
            Self::InterruptPolicy { backup } => Some(&backup.step),
            Self::NetworkStackTransaction { transaction } => Some(&transaction.step),
            Self::Unknown(_) => None,
        }
    }
}

impl BackupFile {
    pub fn push_first_value(&mut self, entry: BackupEntry) {
        let identity = dedupe_identity(&entry);
        if identity.is_none()
            || !self
                .entries
                .iter()
                .any(|existing| dedupe_identity(existing) == identity)
        {
            self.entries.push(entry);
        }
    }

    pub fn restore_order(&self) -> impl Iterator<Item = &BackupEntry> {
        self.entries.iter().rev()
    }
}

fn dedupe_identity(entry: &BackupEntry) -> Option<String> {
    let step = entry.step()?.to_ascii_lowercase();
    match entry {
        BackupEntry::Registry { path, name, .. } => Some(format!(
            "{step}:registry:{}:{}",
            path.to_ascii_lowercase(),
            name.to_ascii_lowercase()
        )),
        BackupEntry::Hags { .. } => Some(format!("{step}:hags")),
        BackupEntry::Service { name, .. } => {
            Some(format!("{step}:service:{}", name.to_ascii_lowercase()))
        }
        BackupEntry::Powerplan { original_guid, .. } => Some(format!(
            "{step}:powerplan:{}",
            original_guid.to_ascii_lowercase()
        )),
        BackupEntry::Bootconfig { key, .. } => {
            Some(format!("{step}:bootconfig:{}", key.to_ascii_lowercase()))
        }
        BackupEntry::Scheduledtask {
            task_path,
            task_name,
            ..
        } => Some(format!(
            "{step}:scheduledtask:{}{}",
            task_path.to_ascii_lowercase(),
            task_name.to_ascii_lowercase()
        )),
        BackupEntry::NicAdapter {
            adapter_name,
            property_name,
            ..
        } => Some(format!(
            "{step}:nic_adapter:{}:{}",
            adapter_name.to_ascii_lowercase(),
            property_name.to_ascii_lowercase()
        )),
        BackupEntry::Dns {
            adapter_guid,
            adapter_name,
            ..
        } => Some(format!(
            "{step}:dns:{}",
            adapter_guid
                .as_deref()
                .unwrap_or(adapter_name)
                .to_ascii_lowercase()
        )),
        BackupEntry::InterruptPolicy { backup } => Some(format!(
            "{step}:interrupt:{}:{}",
            backup.device.instance_id.to_ascii_lowercase(),
            backup.policy.value_name().to_ascii_lowercase()
        )),
        BackupEntry::NetworkStackTransaction { transaction } => Some(format!(
            "{step}:network_stack:{}",
            transaction.adapter.adapter_name.to_ascii_lowercase()
        )),
        BackupEntry::PagefileTransaction { .. } => Some(format!("{step}:pagefile_transaction")),
        BackupEntry::Cs2ConfigTransaction {
            install_identity, ..
        } => Some(format!(
            "{step}:cs2_config_transaction:{}",
            install_identity.install_fingerprint.to_ascii_lowercase()
        )),
        BackupEntry::QosUro { .. }
        | BackupEntry::Defender { .. }
        | BackupEntry::Pagefile { .. }
        | BackupEntry::Drs { .. }
        | BackupEntry::Unknown(_) => None,
    }
}
