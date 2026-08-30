use super::*;

fn device(inf: &str, instance: &str) -> PciDeviceBinding {
    PciDeviceBinding {
        schema_version: 1,
        instance_id: instance.into(),
        container_id: "{01234567-89ab-cdef-0123-456789abcdef}".into(),
        class_guid: "{4d36e968-e325-11ce-bfc1-08002be10318}".into(),
        vendor_id: 0x10de,
        device_id: 0x2684,
        subsystem_vendor_id: 0x1462,
        subsystem_device_id: 0x4710,
        revision_id: 0xa1,
        driver_provider: "NVIDIA".into(),
        driver_version: "32.0.15.1234".into(),
        published_inf: inf.into(),
        observed_at_utc: "2026-08-13T12:00:00Z".into(),
        unknown: BTreeMap::new(),
    }
}

fn gpu(inf: &str) -> PciDeviceBinding {
    device(
        inf,
        r"PCI\VEN_10DE&DEV_2684&SUBSYS_47101462&REV_A1\4&abc&0&0008",
    )
}

#[test]
fn receipt_is_step_bound_and_tamper_evident() {
    let mut receipt = ObservationReceipt::new(
        "2026-08-13T12:00:00Z",
        None,
        None,
        ObservationSubject::MsiDeviceSet {
            devices: vec![gpu("oem42.inf")],
        },
    )
    .expect("receipt");
    receipt.validate_for("P1:21").expect("valid receipt");
    assert_eq!(
        receipt.validate_for("P1:22"),
        Err(EvidenceError::StepMismatch)
    );
    receipt.captured_at_utc.push('x');
    assert_eq!(
        receipt.validate_for("P1:21"),
        Err(EvidenceError::ReceiptMismatch)
    );
}

#[test]
fn known_unknown_fields_round_trip_but_do_not_authorize() {
    let receipt = ObservationReceipt::new(
        "2026-08-13T12:00:00Z",
        None,
        None,
        ObservationSubject::MsiDeviceSet {
            devices: vec![gpu("oem42.inf")],
        },
    )
    .expect("receipt");
    let mut value = serde_json::to_value(receipt).expect("value");
    value["futureAuthority"] = Value::Bool(true);
    let parsed: ObservationReceipt = serde_json::from_value(value).expect("receipt");
    assert_eq!(parsed.unknown["futureAuthority"], true);
    assert_eq!(
        parsed.validate_for("P1:21"),
        Err(EvidenceError::UnknownFields)
    );
}

#[test]
fn device_sets_must_be_nonempty_sorted_and_unique() {
    let empty = ObservationReceipt::new(
        "2026-08-13T12:00:00Z",
        None,
        None,
        ObservationSubject::MsiDeviceSet { devices: vec![] },
    );
    assert_eq!(empty, Err(EvidenceError::InvalidSubjectSet));

    let duplicate = ObservationReceipt::new(
        "2026-08-13T12:00:00Z",
        None,
        None,
        ObservationSubject::MsiDeviceSet {
            devices: vec![gpu("oem42.inf"), gpu("oem42.inf")],
        },
    );
    assert_eq!(duplicate, Err(EvidenceError::InvalidSubjectSet));
}

#[test]
fn evidence_file_replaces_one_exact_step_and_rejects_ambiguous_json() {
    let receipt = ObservationReceipt::new(
        "2026-08-13T12:00:00Z",
        None,
        None,
        ObservationSubject::MsiDeviceSet {
            devices: vec![gpu("oem42.inf")],
        },
    )
    .expect("receipt");
    let mut file = EvidenceFile {
        entries: vec![],
        created: "now".into(),
        unknown: BTreeMap::new(),
    };
    file.replace_observation(receipt.clone());
    file.replace_observation(receipt.clone());
    assert_eq!(file.entries.len(), 1);
    assert_eq!(
        file.observation_for("P1:21").expect("lookup"),
        Some(&receipt)
    );
    file.entries
        .push(EvidenceEntry::Observation(Box::new(receipt)));
    assert_eq!(
        file.observation_for("P1:21"),
        Err(EvidenceError::InvalidSubjectSet)
    );
}

#[test]
fn affinity_receipt_rejects_a_multi_bit_or_out_of_range_mask() {
    let adapter = NetworkAdapterBinding {
        schema_version: 1,
        adapter_name: "{aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee}".into(),
        interface_guid: "{aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee}".into(),
        interface_luid: 7,
        interface_index: 9,
        friendly_name: "Ethernet".into(),
        interface_description: "PCIe Ethernet Controller".into(),
        physical_address: vec![1, 2, 3, 4, 5, 6],
        device: gpu("oem42.inf"),
        observed_at_utc: "2026-08-13T12:00:00Z".into(),
        unknown: BTreeMap::new(),
    };
    let result = ObservationReceipt::new(
        "2026-08-13T12:00:00Z",
        None,
        None,
        ObservationSubject::NicAffinityProposal {
            adapter: Box::new(adapter),
            processor_group: 0,
            logical_processor_count: 16,
            target_processor: 15,
            assignment_mask: 3,
        },
    );
    assert_eq!(
        result,
        Err(EvidenceError::InvalidField("processorTopology"))
    );
}
