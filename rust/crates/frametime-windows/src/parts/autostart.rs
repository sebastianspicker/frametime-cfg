const AUTOSTART_RUN_KEY: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run";

#[derive(Debug, Clone, PartialEq, Eq)]
struct AutostartTarget {
    hive: Hive,
    name: String,
    original: RegValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AutostartBinding {
    targets: Vec<AutostartTarget>,
}

fn autostart_names(config: Option<&Config>) -> Result<Vec<String>, String> {
    let config = config.ok_or("P1:14 requires validated frametime.toml autostart_remove")?;
    config
        .validate()
        .map_err(|error| format!("invalid config: {error}"))?;
    let mut names = Vec::with_capacity(config.autostart_remove.len());
    for candidate in &config.autostart_remove {
        if !valid_autostart_name(candidate) {
            return Err("P1:14 autostart name is empty, unsafe, or outside the bounded contract".into());
        }
        if names.iter().any(|known: &String| known.eq_ignore_ascii_case(candidate)) {
            return Err(format!("P1:14 config contains duplicate autostart name: {candidate}"));
        }
        names.push(candidate.clone());
    }
    Ok(names)
}

fn valid_autostart_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, ' ' | '_' | '-' | '.')
        })
}

fn autostart_change(hive: Hive, name: &str) -> RegistryChange {
    RegistryChange {
        hive,
        key: AUTOSTART_RUN_KEY,
        name: Box::leak(name.to_owned().into_boxed_str()),
        // The value never reaches a setter; reads and deletes are bound by the
        // captured identity and original typed value below.
        value: RegValue::Dword(0),
    }
}

fn autostart_targets(config: Option<&Config>) -> Result<Vec<(Hive, String)>, String> {
    let names = autostart_names(config)?;
    Ok([Hive::CurrentUser, Hive::LocalMachine]
        .into_iter()
        .flat_map(|hive| names.iter().cloned().map(move |name| (hive, name)))
        .collect())
}

fn inspect_autostart(config: Option<&Config>) -> Result<Inspection, String> {
    let present = autostart_targets(config)?
        .into_iter()
        .map(|(hive, name)| registry_read_exact(&autostart_change(hive, &name)))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .any(|value| value.is_some());
    Ok(if present {
        Inspection::NeedsApply
    } else {
        Inspection::Inapplicable
    })
}

fn capture_autostart(
    step: String,
    config: Option<&Config>,
) -> Result<(AutostartBinding, Vec<BackupEntry>), String> {
    let mut targets = Vec::new();
    let mut entries = Vec::new();
    for (hive, name) in autostart_targets(config)? {
        let change = autostart_change(hive, &name);
        let Some(original) = registry_read_exact(&change)? else {
            continue;
        };
        let (original_value, original_type) = match &original {
            RegValue::Dword(value) => (Value::from(*value), "DWord"),
            RegValue::String(value) => (Value::String((*value).into()), "String"),
            RegValue::Binary(value) => (
                Value::Array(value.iter().copied().map(Value::from).collect()),
                "Binary",
            ),
        };
        entries.push(BackupEntry::Registry {
            step: step.clone(),
            timestamp: timestamp(),
            path: format!(
                "{}:\\{AUTOSTART_RUN_KEY}",
                if hive == Hive::CurrentUser { "HKCU" } else { "HKLM" }
            ),
            name: name.clone(),
            original_value,
            original_type: Some(original_type.into()),
            existed: true,
            unknown: BTreeMap::new(),
        });
        targets.push(AutostartTarget { hive, name, original });
    }
    if targets.is_empty() {
        return Err("P1:14 found no exact configured Run value to capture".into());
    }
    Ok((AutostartBinding { targets }, entries))
}

fn apply_autostart(binding: &AutostartBinding) -> Result<(), String> {
    for target in &binding.targets {
        let change = autostart_change(target.hive, &target.name);
        if registry_read_exact(&change)?.as_ref() != Some(&target.original) {
            return Err("P1:14 Run value changed after capture; refusing deletion".into());
        }
    }
    for target in &binding.targets {
        registry_delete(target.hive, AUTOSTART_RUN_KEY, &target.name)?;
    }
    Ok(())
}

fn verify_autostart(binding: &AutostartBinding) -> Result<(), String> {
    for target in &binding.targets {
        if registry_read_exact(&autostart_change(target.hive, &target.name))?.is_some() {
            return Err("P1:14 configured Run value remains after deletion".into());
        }
    }
    Ok(())
}

fn validate_autostart_restore_binding(
    config: &Config,
    hive: Hive,
    key: &str,
    name: &str,
) -> Result<(), String> {
    if !matches!(hive, Hive::CurrentUser | Hive::LocalMachine) || key != AUTOSTART_RUN_KEY {
        return Err("P1:14 restore key is not an exact Run binding".into());
    }
    if !valid_autostart_name(name) {
        return Err("P1:14 restore name is unsafe".into());
    }
    let names = autostart_names(Some(config))?;
    if names.iter().any(|configured| configured == name) {
        Ok(())
    } else {
        Err("P1:14 restore name is not present in validated config".into())
    }
}
