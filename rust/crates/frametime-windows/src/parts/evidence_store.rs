fn load_evidence_file(trusted: &TrustedWorkDir) -> Result<frametime_core::EvidenceFile, String> {
    if trusted.path().join(EVIDENCE_FILE).exists() {
        read_json_trusted(trusted, EVIDENCE_FILE)
            .map_err(|error| format!("read prerequisite evidence: {error}"))
    } else {
        Ok(frametime_core::EvidenceFile {
            entries: Vec::new(),
            created: timestamp(),
            unknown: BTreeMap::new(),
        })
    }
}

fn persist_observation_receipt(
    trusted: &TrustedWorkDir,
    receipt: &frametime_core::ObservationReceipt,
) -> Result<(), String> {
    receipt
        .validate_for(&receipt.step)
        .map_err(|error| format!("validate prerequisite evidence: {error}"))?;
    let mut file = load_evidence_file(trusted)?;
    if !file.unknown.is_empty() {
        return Err("evidence document has unrecognized root fields".into());
    }
    file.replace_observation(receipt.clone());
    write_json_atomic_trusted(trusted, EVIDENCE_FILE, &file)
        .map_err(|error| format!("persist prerequisite evidence: {error}"))?;
    let verified: frametime_core::EvidenceFile = read_json_trusted(trusted, EVIDENCE_FILE)
        .map_err(|error| format!("read back prerequisite evidence: {error}"))?;
    if verified != file
        || verified
            .observation_for(&receipt.step)
            .map_err(|error| format!("validate persisted prerequisite evidence: {error}"))?
            != Some(receipt)
    {
        return Err("prerequisite evidence readback did not match the captured receipt".into());
    }
    Ok(())
}

fn load_observation_receipt(
    trusted: &TrustedWorkDir,
    step: &str,
) -> Result<Option<frametime_core::ObservationReceipt>, String> {
    let file = load_evidence_file(trusted)?;
    if !file.unknown.is_empty() {
        return Err("evidence document has unrecognized root fields".into());
    }
    file.observation_for(step)
        .map(|receipt| receipt.cloned())
        .map_err(|error| format!("validate prerequisite evidence: {error}"))
}
