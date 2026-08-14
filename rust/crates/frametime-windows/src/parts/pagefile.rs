use frametime_core::PagefileTransactionSetting;

const MAX_PAGEFILE_MB: u64 = 1_048_576;
const GIB_MB: u64 = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
struct PagefileSetting {
    path: String,
    initial_size: u32,
    maximum_size: u32,
    object_path: String,
    relative_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PagefileInventory {
    automatic_managed: bool,
    computer_object_path: String,
    computer_relative_path: String,
    system_drive: String,
    physical_ram_mb: u64,
    free_space_mb: u64,
    settings: Vec<PagefileSetting>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PagefileBinding {
    step: String,
    target_path: String,
    initial_size: u32,
    maximum_size: u32,
    before: PagefileInventory,
    target: Option<PagefileSetting>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CreatedPagefileToken {
    object_path: String,
    relative_path: String,
    path: String,
    initial_size: u32,
    maximum_size: u32,
}

#[cfg(any(test, windows))]
trait PagefileStore {
    fn inventory(&mut self) -> Result<PagefileInventory, String>;
    fn set_automatic(&mut self, object_path: &str, relative_path: &str, expected: Option<bool>, value: bool) -> Result<(), String>;
    fn update(&mut self, setting: &PagefileSetting, initial: u32, maximum: u32) -> Result<(), String>;
    fn create(&mut self, path: &str, initial: u32, maximum: u32) -> Result<CreatedPagefileToken, String>;
    fn delete(&mut self, created: &CreatedPagefileToken) -> Result<(), String>;
}

fn pagefile_target(system_drive: &str) -> Result<String, String> {
    let bytes = system_drive.as_bytes();
    if bytes.len() != 2 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' {
        return Err("SystemDrive is not an exact local drive designator".into());
    }
    Ok(format!("{}\\pagefile.sys", system_drive.to_ascii_uppercase()))
}

fn pagefile_size_mb(state: &State, ram_mb: u64) -> Result<u32, String> {
    if state.pagefile_mb != 0 {
        return u32::try_from(state.pagefile_mb)
            .map_err(|_| "configured pagefile size does not fit Win32 uint32".into());
    }
    if ram_mb == 0 {
        return Err("physical RAM inventory is zero or unavailable".into());
    }
    let gib = ram_mb
        .checked_add(GIB_MB / 2)
        .ok_or("physical RAM rounding overflow")?
        / GIB_MB;
    u32::try_from(
        gib.checked_mul(2)
            .and_then(|value| value.checked_mul(GIB_MB))
            .ok_or("derived pagefile size overflow")?,
    )
    .map_err(|_| "derived pagefile size does not fit Win32 uint32".into())
}

fn validate_inventory(inventory: &PagefileInventory) -> Result<(), String> {
    if inventory.computer_object_path.is_empty()
        || inventory.computer_relative_path.is_empty()
        || inventory.system_drive.is_empty()
    {
        return Err("CIM computer-system identity or SystemDrive is missing".into());
    }
    let mut object_paths = std::collections::BTreeSet::new();
    let mut relative_paths = std::collections::BTreeSet::new();
    let mut pagefile_paths = std::collections::BTreeSet::new();
    for setting in &inventory.settings {
        validate_pagefile_identity(setting)?;
        if !object_paths.insert(setting.object_path.to_ascii_lowercase())
            || !relative_paths.insert(setting.relative_path.to_ascii_lowercase())
            || !pagefile_paths.insert(setting.path.to_ascii_lowercase())
        {
            return Err("CIM pagefile inventory contains duplicate identities".into());
        }
    }
    Ok(())
}

fn validate_pagefile_identity(setting: &PagefileSetting) -> Result<(), String> {
    if setting.path.is_empty()
        || setting.object_path.is_empty()
        || setting.relative_path.is_empty()
        || [setting.path.as_str(), setting.object_path.as_str(), setting.relative_path.as_str()]
            .iter()
            .any(|value| value.contains('\0') || value.contains('\r') || value.contains('\n'))
    {
        return Err("CIM pagefile instance has an unsafe or incomplete identity".into());
    }
    if setting.initial_size > setting.maximum_size {
        return Err("CIM pagefile instance has invalid size ordering".into());
    }
    Ok(())
}

fn capture_pagefile_binding(step: String, state: &State, inventory: PagefileInventory) -> Result<PagefileBinding, String> {
    validate_inventory(&inventory)?;
    let target_path = pagefile_target(&inventory.system_drive)?;
    let initial_size = pagefile_size_mb(state, inventory.physical_ram_mb)?;
    if initial_size as u64 > MAX_PAGEFILE_MB || inventory.free_space_mb < initial_size as u64 {
        return Err("pagefile target size exceeds validated free space or safe bounds".into());
    }
    let target = inventory
        .settings
        .iter()
        .filter(|setting| setting.path.eq_ignore_ascii_case(&target_path))
        .cloned()
        .collect::<Vec<_>>();
    if target.len() > 1 {
        return Err("CIM inventory contains duplicate target pagefile instances".into());
    }
    Ok(PagefileBinding {
        step,
        target_path,
        initial_size,
        maximum_size: initial_size,
        before: inventory,
        target: target.into_iter().next(),
    })
}

fn pagefile_backup_entry(binding: &PagefileBinding) -> BackupEntry {
    BackupEntry::PagefileTransaction {
        step: binding.step.clone(),
        timestamp: timestamp(),
        automatic_managed: binding.before.automatic_managed,
        target_path: binding.target_path.clone(),
        target_existed: binding.target.is_some(),
        computer_object_path: Some(binding.before.computer_object_path.clone()),
        computer_relative_path: Some(binding.before.computer_relative_path.clone()),
        created_object_path: None,
        created_relative_path: None,
        created_initial_size: None,
        created_maximum_size: None,
        mutation_intent: Some(if binding.target.is_some() { "update_pending" } else { "create_pending" }.into()),
        settings: binding.before.settings.iter().map(pagefile_setting_backup).collect(),
        unknown: BTreeMap::new(),
    }
}

fn pagefile_setting_backup(setting: &PagefileSetting) -> PagefileTransactionSetting {
    PagefileTransactionSetting {
        path: setting.path.clone(),
        initial_size: u64::from(setting.initial_size),
        maximum_size: u64::from(setting.maximum_size),
        object_path: Some(setting.object_path.clone()),
        relative_path: Some(setting.relative_path.clone()),
        unknown: BTreeMap::new(),
    }
}

fn inspect_pagefile(state: &State) -> Result<Inspection, String> {
    let inventory = match native_pagefile_inventory() {
        Ok(inventory) => inventory,
        Err(_) => return Ok(Inspection::Unsupported),
    };
    if inventory.physical_ram_mb == 0 {
        return Ok(Inspection::Unsupported);
    }
    if state.pagefile_mb == 0 && inventory.physical_ram_mb >= 32 * GIB_MB {
        return Ok(Inspection::Inapplicable);
    }
    capture_pagefile_binding("P1:8".into(), state, inventory)?;
    Ok(Inspection::NeedsApply)
}

fn persist_created_pagefile_token(
    trusted: &TrustedWorkDir,
    created: &CreatedPagefileToken,
) -> Result<(), String> {
    if created.object_path.is_empty() || created.relative_path.is_empty() {
        return Err("created pagefile token is incomplete".into());
    }
    let mut backup: BackupFile = read_json_trusted(trusted, BACKUP_FILE)
        .map_err(|error| format!("read persisted P1:8 backup: {error}"))?;
    let matching = backup.entries.iter_mut().filter_map(|entry| match entry {
        BackupEntry::PagefileTransaction {
            step,
            created_object_path,
            created_relative_path,
            created_initial_size,
            created_maximum_size,
            mutation_intent,
            unknown,
            ..
        } if step == "P1:8" => Some((created_object_path, created_relative_path, created_initial_size, created_maximum_size, mutation_intent, unknown)),
        _ => None,
    }).collect::<Vec<_>>();
    if matching.len() != 1 {
        return Err("persisted backup does not contain exactly one P1:8 transaction".into());
    }
    let (object_path, relative_path, initial_size, maximum_size, intent, unknown) = matching.into_iter().next().expect("checked length");
    if !unknown.is_empty() || object_path.is_some() || relative_path.is_some() || initial_size.is_some() || maximum_size.is_some() || intent.as_deref() != Some("create_pending") {
        return Err("persisted P1:8 create journal has unsafe provenance".into());
    }
    *object_path = Some(created.object_path.clone());
    *relative_path = Some(created.relative_path.clone());
    *initial_size = Some(u64::from(created.initial_size));
    *maximum_size = Some(u64::from(created.maximum_size));
    *intent = Some("created".into());
    write_json_atomic_trusted(trusted, BACKUP_FILE, &backup)
        .map_err(|error| format!("atomically persist exact created pagefile token: {error}"))
}

fn compound_pagefile_error(prefix: &str, primary: String, rollback: Result<(), String>) -> String {
    match rollback {
        Ok(()) => format!("{prefix}: {primary}; target and automatic-management compensation completed"),
        Err(rollback) => format!("{prefix}: {primary}; compensation also failed: {rollback}"),
    }
}

impl LiveBackend {
    fn apply_pagefile_action(&mut self, key: &str) -> Result<(), String> {
        let binding = self.captured_pagefile_bindings.get(key)
            .ok_or("P1:8 mutation requires a captured CIM pagefile binding")?;
        match native_pagefile_begin(binding) {
            Ok(Some(created)) => {
                if let Err(error) = persist_created_pagefile_token(&self._trusted_work_dir, &created) {
                    let rollback = native_pagefile_compensate(binding, Some(&created));
                    return Err(compound_pagefile_error("persist exact created pagefile token", error, rollback));
                }
                self.created_pagefile_tokens.insert(key.into(), created);
                Ok(())
            }
            Ok(None) => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn verify_pagefile_action(&mut self, key: &str) -> Result<(), String> {
        let binding = self.captured_pagefile_bindings.get(key)
            .ok_or("P1:8 verification requires a captured CIM pagefile binding")?;
        let created = self.created_pagefile_tokens.get(key);
        match native_pagefile_verify(binding, created) {
            Ok(()) => Ok(()),
            Err(error) => Err(compound_pagefile_error(
                "verify P1:8 postconditions", error, native_pagefile_compensate(binding, created),
            )),
        }
    }
}

#[cfg(any(test, windows))]
fn restore_pagefile_transaction<S: PagefileStore>(store: &mut S, entry: &BackupEntry) -> Result<(), String> {
    let BackupEntry::PagefileTransaction {
        step, automatic_managed, target_path, target_existed, settings, unknown,
        computer_object_path, computer_relative_path, created_object_path,
        created_relative_path, created_initial_size, created_maximum_size,
        mutation_intent, ..
    } = entry else { return Err("pagefile transaction restore received the wrong entry type".into()); };
    if step != "P1:8" || !unknown.is_empty() || computer_object_path.as_deref().filter(|v| !v.is_empty()).is_none()
        || computer_relative_path.as_deref().filter(|v| !v.is_empty()).is_none() {
        return Err("P1:8 backup has incomplete or untrusted transaction provenance".into());
    }
    let created = match (created_object_path, created_relative_path, mutation_intent.as_deref()) {
        (None, None, Some("update_pending")) if *target_existed => None,
        (Some(object_path), Some(relative_path), Some("created")) if !*target_existed && !object_path.is_empty() && !relative_path.is_empty() => {
            let initial_size = u32::try_from(created_initial_size.ok_or("P1:8 created token lacks its expected initial size")?)
                .map_err(|_| "P1:8 created initial size exceeds uint32")?;
            let maximum_size = u32::try_from(created_maximum_size.ok_or("P1:8 created token lacks its expected maximum size")?)
                .map_err(|_| "P1:8 created maximum size exceeds uint32")?;
            if initial_size > maximum_size { return Err("P1:8 created token has invalid expected sizes".into()); }
            Some(CreatedPagefileToken { object_path: object_path.clone(), relative_path: relative_path.clone(), path: target_path.clone(), initial_size, maximum_size })
        },
        _ => return Err("P1:8 backup has no exact deletion authority; manual recovery is required".into()),
    };
    let before = PagefileInventory {
        automatic_managed: *automatic_managed,
        computer_object_path: computer_object_path.clone().expect("checked"),
        computer_relative_path: computer_relative_path.clone().expect("checked"),
        system_drive: target_path.get(0..2).ok_or("P1:8 target path is malformed")?.to_owned(),
        physical_ram_mb: 1,
        free_space_mb: u64::MAX,
        settings: settings.iter().map(|setting| {
            let initial_size = u32::try_from(setting.initial_size).map_err(|_| "P1:8 initial size exceeds uint32")?;
            let maximum_size = u32::try_from(setting.maximum_size).map_err(|_| "P1:8 maximum size exceeds uint32")?;
            let item = PagefileSetting { path: setting.path.clone(), initial_size, maximum_size,
                object_path: setting.object_path.clone().ok_or("P1:8 setting lacks exact object token")?,
                relative_path: setting.relative_path.clone().ok_or("P1:8 setting lacks exact relative token")? };
            validate_pagefile_identity(&item)?; Ok(item)
        }).collect::<Result<Vec<_>, String>>()?,
    };
    validate_inventory(&before)?;
    let live = store.inventory()?;
    validate_inventory(&live)?;
    if live.computer_object_path != before.computer_object_path || live.computer_relative_path != before.computer_relative_path {
        return Err("P1:8 computer-system token no longer matches; manual recovery is required".into());
    }
    if let Some(created) = &created {
        let matches = live.settings.iter().filter(|setting| setting.object_path == created.object_path && setting.relative_path == created.relative_path && setting.path.eq_ignore_ascii_case(target_path)).collect::<Vec<_>>();
        if matches.len() != 1 { return Err("P1:8 created token is absent or ambiguous; refusing deletion".into()); }
        if matches[0].initial_size != created.initial_size || matches[0].maximum_size != created.maximum_size {
            return Err("P1:8 created token sizes changed; refusing deletion".into());
        }
        store.delete(created)?;
    }
    if *target_existed {
        let target = before.settings.iter().find(|setting| setting.path.eq_ignore_ascii_case(target_path))
            .ok_or("P1:8 target token is absent from the complete original inventory")?;
        store.update(target, target.initial_size, target.maximum_size)?;
    }
    let restored = store.inventory()?;
    for setting in &before.settings {
        let current = restored.settings.iter().find(|current| current.object_path == setting.object_path);
        if setting.path.eq_ignore_ascii_case(target_path) && *target_existed {
            if current != Some(setting) { return Err("P1:8 target setting did not restore exactly".into()); }
        } else if current != Some(setting) { return Err("P1:8 foreign pagefile setting changed during restore".into()); }
    }
    if let Some(created) = &created
        && restored.settings.iter().any(|setting| setting.object_path == created.object_path || setting.relative_path == created.relative_path) {
        return Err("P1:8 exact created setting remains after deletion".into());
    }
    store.set_automatic(&before.computer_object_path, &before.computer_relative_path, None, before.automatic_managed)?;
    if store.inventory()?.automatic_managed != before.automatic_managed { return Err("P1:8 AutomaticManagedPagefile restore did not read back exactly".into()); }
    Ok(())
}

#[cfg(any(test, windows))]
fn check_foreign_preservation(binding: &PagefileBinding, after: &PagefileInventory) -> Result<(), String> {
    validate_inventory(after)?;
    for original in &binding.before.settings {
        if binding.target.as_ref().is_some_and(|target| target.object_path == original.object_path) {
            continue;
        }
        let found = after.settings.iter().find(|setting| setting.object_path == original.object_path);
        if found != Some(original) {
            return Err("a non-target pagefile setting changed or disappeared".into());
        }
    }
    Ok(())
}

#[cfg(any(test, windows))]
fn begin_pagefile_mutation<S: PagefileStore>(store: &mut S, binding: &PagefileBinding) -> Result<Option<CreatedPagefileToken>, String> {
    store.set_automatic(&binding.before.computer_object_path, &binding.before.computer_relative_path, Some(binding.before.automatic_managed), false)?;
    let disabled = store.inventory()?;
    if disabled.automatic_managed {
        return Err("AutomaticManagedPagefile did not read back false".into());
    }
    check_foreign_preservation(binding, &disabled)?;
    if let Some(target) = &binding.target {
        store.update(target, binding.initial_size, binding.maximum_size)?;
        Ok(None)
    } else {
        store
            .create(&binding.target_path, binding.initial_size, binding.maximum_size)
            .map(Some)
    }
}

#[cfg(any(test, windows))]
fn verify_pagefile_mutation<S: PagefileStore>(store: &mut S, binding: &PagefileBinding, created: Option<&CreatedPagefileToken>) -> Result<(), String> {
    let after = store.inventory()?;
    if after.automatic_managed {
        return Err("AutomaticManagedPagefile changed during pagefile transaction".into());
    }
    check_foreign_preservation(binding, &after)?;
    let target = after.settings.iter().find(|setting| setting.path.eq_ignore_ascii_case(&binding.target_path))
        .ok_or("target pagefile setting is absent after mutation")?;
    if target.initial_size != binding.initial_size || target.maximum_size != binding.maximum_size {
        return Err("target pagefile sizes did not read back exactly".into());
    }
    if let Some(created) = created
        && (target.object_path != created.object_path || target.relative_path != created.relative_path)
    {
        return Err("created pagefile identity did not read back exactly".into());
    }
    Ok(())
}

#[cfg(any(test, windows))]
fn compensate_pagefile_mutation<S: PagefileStore>(store: &mut S, binding: &PagefileBinding, created: Option<&CreatedPagefileToken>) -> Result<(), String> {
    let mut failures = Vec::new();
    if let Some(created) = created {
        if let Err(error) = store.delete(created) {
            failures.push(format!("delete exact created pagefile: {error}"));
        }
    } else if let Some(target) = &binding.target
        && let Err(error) = store.update(target, target.initial_size, target.maximum_size)
    {
        failures.push(format!("restore target pagefile: {error}"));
    }
    if let Err(error) = store.set_automatic(&binding.before.computer_object_path, &binding.before.computer_relative_path, None, binding.before.automatic_managed) {
        failures.push(format!("restore AutomaticManagedPagefile last: {error}"));
    }
    if failures.is_empty() { Ok(()) } else { Err(failures.join("; ")) }
}

#[cfg(test)]
mod pagefile_tests {
    use super::*;

    #[derive(Clone)]
    struct MockStore { inventory: PagefileInventory, fail: Option<&'static str> }
    impl PagefileStore for MockStore {
        fn inventory(&mut self) -> Result<PagefileInventory, String> { Ok(self.inventory.clone()) }
        fn set_automatic(&mut self, _: &str, _: &str, expected: Option<bool>, value: bool) -> Result<(), String> { if self.fail == Some("automatic") || expected.is_some_and(|expected| self.inventory.automatic_managed != expected) { return Err("fault".into()); } self.inventory.automatic_managed = value; Ok(()) }
        fn update(&mut self, setting: &PagefileSetting, initial: u32, maximum: u32) -> Result<(), String> { if self.fail == Some("update") { return Err("fault".into()); } let item = self.inventory.settings.iter_mut().find(|candidate| candidate.object_path == setting.object_path).ok_or("missing")?; item.initial_size=initial; item.maximum_size=maximum; Ok(()) }
        fn create(&mut self, path: &str, initial: u32, maximum: u32) -> Result<CreatedPagefileToken, String> { if self.fail == Some("create") { return Err("fault".into()); } let token=CreatedPagefileToken { object_path: "Win32_PageFileSetting.Name=\"C:\\\\pagefile.sys\"".into(), relative_path: "Win32_PageFileSetting.Name=\"C:\\\\pagefile.sys\"".into(), path:path.into(), initial_size:initial, maximum_size:maximum }; self.inventory.settings.push(PagefileSetting { path:path.into(), initial_size:initial, maximum_size:maximum, object_path:token.object_path.clone(), relative_path:token.relative_path.clone() }); Ok(token) }
        fn delete(&mut self, created: &CreatedPagefileToken) -> Result<(), String> { self.inventory.settings.retain(|item| item.object_path != created.object_path); Ok(()) }
    }
    fn inventory() -> PagefileInventory { PagefileInventory { automatic_managed:true, computer_object_path:"Win32_ComputerSystem.Name=\"HOST\"".into(), computer_relative_path:"Win32_ComputerSystem.Name=\"HOST\"".into(), system_drive:"C:".into(), physical_ram_mb: 16*GIB_MB, free_space_mb: 100_000, settings: vec![PagefileSetting { path:"D:\\foreign.sys".into(), initial_size:99, maximum_size:100, object_path:"foreign-object".into(), relative_path:"foreign-relative".into() }] } }
    #[test] fn creates_only_after_full_capture_and_preserves_foreign_setting() { let binding=capture_pagefile_binding("P1:8".into(), &State::default(), inventory()).expect("binding"); let mut store=MockStore { inventory: binding.before.clone(), fail:None }; let created=begin_pagefile_mutation(&mut store,&binding).expect("mutate"); verify_pagefile_mutation(&mut store,&binding,created.as_ref()).expect("verify"); assert_eq!(store.inventory.settings[0].initial_size,99); assert!(created.is_some()); }
    #[test] fn compensation_deletes_only_opaque_created_identity_and_automatic_last() { let binding=capture_pagefile_binding("P1:8".into(), &State::default(), inventory()).expect("binding"); let mut store=MockStore { inventory: binding.before.clone(), fail:None }; let created=begin_pagefile_mutation(&mut store,&binding).expect("mutate"); compensate_pagefile_mutation(&mut store,&binding,created.as_ref()).expect("compensate"); assert!(store.inventory.automatic_managed); assert_eq!(store.inventory.settings.len(),1); }
    #[test] fn restore_uses_only_created_token_and_preserves_foreign_setting() { let binding=capture_pagefile_binding("P1:8".into(), &State::default(), inventory()).expect("binding"); let mut store=MockStore { inventory: binding.before.clone(), fail:None }; let created=begin_pagefile_mutation(&mut store,&binding).expect("mutate").expect("created"); let mut entry=pagefile_backup_entry(&binding); if let BackupEntry::PagefileTransaction { created_object_path,created_relative_path,created_initial_size,created_maximum_size,mutation_intent,.. }=&mut entry { *created_object_path=Some(created.object_path); *created_relative_path=Some(created.relative_path); *created_initial_size=Some(u64::from(created.initial_size)); *created_maximum_size=Some(u64::from(created.maximum_size)); *mutation_intent=Some("created".into()); } restore_pagefile_transaction(&mut store,&entry).expect("restore"); assert_eq!(store.inventory.settings.len(),1); assert_eq!(store.inventory.settings[0].path,"D:\\foreign.sys"); }
    #[test] fn incomplete_create_journal_fails_closed() { let binding=capture_pagefile_binding("P1:8".into(), &State::default(), inventory()).expect("binding"); let entry=pagefile_backup_entry(&binding); let mut store=MockStore { inventory: binding.before, fail:None }; assert!(restore_pagefile_transaction(&mut store,&entry).is_err()); }
    #[test] fn rejects_duplicate_or_hostile_cim_identities() { let mut state=inventory(); state.settings.push(state.settings[0].clone()); assert!(capture_pagefile_binding("P1:8".into(),&State::default(),state).is_err()); assert!(pagefile_target("\\\\evil").is_err()); }
    #[test] fn low_ram_derives_and_high_ram_is_classified_by_caller() { assert_eq!(pagefile_size_mb(&State::default(), 3*GIB_MB).expect("size"), 6*1024); assert!(pagefile_size_mb(&State::default(),0).is_err()); }
}
