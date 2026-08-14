#[path = "../src/parts/native_drs.rs"]
mod native_drs;

use std::collections::BTreeMap;

use native_drs::{
    CS2_PROFILE_NAME, CS2_SETTINGS, DrsBackup, DrsError, DrsOriginalSetting, DrsPreparation,
    NvapiDrs, apply_cs2_profile, capture_cs2_backup, prepare_cs2_profile, restore_cs2_profile,
    verify_cs2_profile,
};

#[derive(Default)]
struct FakeDrs {
    profiles: BTreeMap<String, BTreeMap<u32, u32>>,
    applications: BTreeMap<String, String>,
    events: Vec<String>,
    session: u32,
}

impl FakeDrs {
    fn with_existing_cs2() -> Self {
        let mut fake = Self::default();
        fake.profiles.insert(
            CS2_PROFILE_NAME.into(),
            [(CS2_SETTINGS[0].id, 99), (CS2_SETTINGS[1].id, 88)]
                .into_iter()
                .collect(),
        );
        fake.applications
            .insert("cs2.exe".into(), CS2_PROFILE_NAME.into());
        fake.applications
            .insert("csgos2.exe".into(), CS2_PROFILE_NAME.into());
        fake
    }
}

impl NvapiDrs for FakeDrs {
    type Session = u32;
    type Profile = String;

    fn initialize(&mut self) -> Result<(), DrsError> {
        self.events.push("initialize".into());
        Ok(())
    }

    fn create_session(&mut self) -> Result<Self::Session, DrsError> {
        self.session += 1;
        self.events.push("create-session".into());
        Ok(self.session)
    }

    fn destroy_session(&mut self, _: Self::Session) -> Result<(), DrsError> {
        self.events.push("destroy-session".into());
        Ok(())
    }

    fn load_settings(&mut self, _: &Self::Session) -> Result<(), DrsError> {
        self.events.push("load".into());
        Ok(())
    }

    fn save_settings(&mut self, _: &Self::Session) -> Result<(), DrsError> {
        self.events.push("save".into());
        Ok(())
    }

    fn find_profile_by_name(
        &mut self,
        _: &Self::Session,
        name: &str,
    ) -> Result<Option<Self::Profile>, DrsError> {
        Ok(self.profiles.contains_key(name).then(|| name.into()))
    }

    fn profile_name(
        &mut self,
        _: &Self::Session,
        profile: &Self::Profile,
    ) -> Result<String, DrsError> {
        Ok(profile.clone())
    }

    fn find_application_profile(
        &mut self,
        _: &Self::Session,
        application: &str,
    ) -> Result<Option<Self::Profile>, DrsError> {
        Ok(self.applications.get(application).cloned())
    }

    fn create_profile(&mut self, _: &Self::Session, name: &str) -> Result<Self::Profile, DrsError> {
        self.events.push(format!("create-profile:{name}"));
        self.profiles.insert(name.into(), BTreeMap::new());
        Ok(name.into())
    }

    fn bind_application(
        &mut self,
        _: &Self::Session,
        profile: &Self::Profile,
        application: &str,
    ) -> Result<(), DrsError> {
        self.events.push(format!("bind:{application}"));
        self.applications
            .insert(application.into(), profile.clone());
        Ok(())
    }

    fn delete_application(
        &mut self,
        _: &Self::Session,
        profile: &Self::Profile,
        application: &str,
    ) -> Result<(), DrsError> {
        self.events.push(format!("delete-app:{application}"));
        if self.applications.get(application) == Some(profile) {
            self.applications.remove(application);
        }
        Ok(())
    }

    fn delete_profile(
        &mut self,
        _: &Self::Session,
        profile: &Self::Profile,
    ) -> Result<(), DrsError> {
        self.events.push(format!("delete-profile:{profile}"));
        self.profiles.remove(profile);
        self.applications.retain(|_, owner| owner != profile);
        Ok(())
    }

    fn read_dword(
        &mut self,
        _: &Self::Session,
        profile: &Self::Profile,
        id: u32,
    ) -> Result<Option<u32>, DrsError> {
        Ok(self
            .profiles
            .get(profile)
            .and_then(|settings| settings.get(&id))
            .copied())
    }

    fn set_dword(
        &mut self,
        _: &Self::Session,
        profile: &Self::Profile,
        id: u32,
        value: u32,
    ) -> Result<(), DrsError> {
        self.events.push(format!("set:{id}"));
        self.profiles
            .entry(profile.clone())
            .or_default()
            .insert(id, value);
        Ok(())
    }

    fn restore_dword(
        &mut self,
        _: &Self::Session,
        profile: &Self::Profile,
        original: DrsOriginalSetting,
    ) -> Result<(), DrsError> {
        let settings = self.profiles.entry(profile.clone()).or_default();
        match original.value {
            Some(value) => {
                settings.insert(original.id, value);
            }
            None => {
                settings.remove(&original.id);
            }
        }
        Ok(())
    }
}

#[test]
fn preparation_reads_a_dedicated_profile_without_mutating() {
    let mut api = FakeDrs::with_existing_cs2();

    assert_eq!(
        prepare_cs2_profile(&mut api),
        Ok(DrsPreparation::ExistingProfile {
            profile: CS2_PROFILE_NAME.into(),
        })
    );
    assert_eq!(
        api.events,
        ["initialize", "create-session", "load", "destroy-session"]
    );
}

#[test]
fn apply_persists_all_originals_before_writing_and_verifies_after_save() {
    let mut api = FakeDrs::with_existing_cs2();
    let backup = capture_cs2_backup(&mut api).expect("capture backup");
    let report = apply_cs2_profile(&mut api, &backup).expect("apply profile");

    assert_eq!(report.verified_settings, 42);
    assert_eq!(backup, report.backup);
    assert_eq!(report.backup.settings.len(), 42);
    assert_eq!(report.backup.settings[0].value, Some(99));
    assert_eq!(report.backup.settings[1].value, Some(88));
    assert_eq!(report.backup.settings[2].value, None);
    assert!(
        api.events.iter().position(|event| event == "save").unwrap()
            < api
                .events
                .iter()
                .rposition(|event| event == "load")
                .unwrap()
    );
    assert_eq!(
        api.profiles[CS2_PROFILE_NAME][&CS2_SETTINGS[0].id],
        CS2_SETTINGS[0].value
    );
    verify_cs2_profile(&mut api, &report.backup).expect("verify");
}

#[test]
fn restore_returns_every_existing_and_absent_setting_to_the_captured_state() {
    let mut api = FakeDrs::with_existing_cs2();
    let backup = capture_cs2_backup(&mut api).expect("capture");
    let report = apply_cs2_profile(&mut api, &backup).expect("apply");

    restore_cs2_profile(&mut api, &report.backup).expect("restore");

    assert_eq!(api.profiles[CS2_PROFILE_NAME][&CS2_SETTINGS[0].id], 99);
    assert_eq!(api.profiles[CS2_PROFILE_NAME][&CS2_SETTINGS[1].id], 88);
    assert!(!api.profiles[CS2_PROFILE_NAME].contains_key(&CS2_SETTINGS[2].id));
}

#[test]
fn restore_removes_only_suite_added_application_registrations() {
    let mut api = FakeDrs::with_existing_cs2();
    api.applications.remove("csgos2.exe");
    let backup = capture_cs2_backup(&mut api).expect("capture");
    assert_eq!(backup.applications[1].profile, None);

    apply_cs2_profile(&mut api, &backup).expect("apply");
    restore_cs2_profile(&mut api, &backup).expect("restore");

    assert!(
        api.applications
            .get("cs2.exe")
            .is_some_and(|profile| profile == CS2_PROFILE_NAME)
    );
    assert!(!api.applications.contains_key("csgos2.exe"));
    assert!(
        api.events
            .iter()
            .any(|event| event == "delete-app:csgos2.exe")
    );
}

#[test]
fn capture_of_a_missing_profile_is_read_only_until_apply() {
    let mut api = FakeDrs::default();
    let backup = capture_cs2_backup(&mut api).expect("capture");

    assert!(backup.profile_created);
    assert!(api.profiles.is_empty());
    assert!(api.applications.is_empty());
    assert!(
        !api.events
            .iter()
            .any(|event| event.starts_with("create-profile:") || event.starts_with("bind:"))
    );

    apply_cs2_profile(&mut api, &backup).expect("apply");
    assert!(api.profiles.contains_key(CS2_PROFILE_NAME));
}

#[test]
fn restore_deletes_only_the_profile_created_by_this_transaction() {
    let mut api = FakeDrs::default();
    let backup = capture_cs2_backup(&mut api).expect("capture");
    let report = apply_cs2_profile(&mut api, &backup).expect("apply");
    assert!(report.backup.profile_created);

    restore_cs2_profile(&mut api, &report.backup).expect("restore");

    assert!(!api.profiles.contains_key(CS2_PROFILE_NAME));
    assert!(!api.applications.contains_key("cs2.exe"));
    assert!(
        api.events
            .iter()
            .any(|event| event == "delete-profile:Counter-strike 2")
    );
}

#[test]
fn rejects_an_application_owned_by_another_profile() {
    let mut api = FakeDrs::with_existing_cs2();
    api.profiles.insert("Other".into(), BTreeMap::new());
    api.applications.insert("csgos2.exe".into(), "Other".into());

    let error = capture_cs2_backup(&mut api).expect_err("must not repurpose profile");

    assert!(error.to_string().contains("different DRS profile"));
    assert!(!api.events.iter().any(|event| event.starts_with("set:")));
}

#[test]
fn backup_requires_the_complete_policy() {
    let mut api = FakeDrs::with_existing_cs2();
    let backup = DrsBackup {
        profile: CS2_PROFILE_NAME.into(),
        profile_created: false,
        settings: Vec::new(),
        applications: Vec::new(),
    };

    assert!(restore_cs2_profile(&mut api, &backup).is_err());
}
