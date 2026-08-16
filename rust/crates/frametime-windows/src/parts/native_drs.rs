//! Typed NVIDIA DRS transaction boundary for the CS2 profile.
//!
//! The portable transaction makes backup persistence and post-save readback
//! mandatory. Its Windows adapter mirrors the public NVIDIA SDK ABI from a
//! pinned upstream revision and loads only the absolute System32 driver DLL.

use std::{error::Error, fmt};

/// The canonical DRS profile created when CS2 has no dedicated profile.
pub const CS2_PROFILE_NAME: &str = "Counter-strike 2";
const CS2_PROFILE_ALIASES: [&str; 2] = [CS2_PROFILE_NAME, "Counter-Strike 2"];
const GLOBAL_PROFILE_NAME: &str = "_GLOBAL_DRIVER_PROFILE";
const CS2_APPLICATIONS: [&str; 2] = ["cs2.exe", "csgos2.exe"];
include!("native_drs_policy.rs");

/// A lossless original DWORD value. `None` means the setting did not exist.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DrsOriginalSetting {
    pub id: u32,
    pub value: Option<u32>,
}

/// The original owner of an application registration. `None` means no DRS
/// profile owned that executable before this transaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DrsApplicationOriginal {
    pub application: String,
    pub profile: Option<String>,
}

/// The complete recovery record required before the profile is changed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DrsBackup {
    pub profile: String,
    pub profile_created: bool,
    pub settings: Vec<DrsOriginalSetting>,
    pub applications: Vec<DrsApplicationOriginal>,
}

/// Preparation result for P1:20.  It is read-only.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DrsPreparation {
    ExistingProfile { profile: String },
    DedicatedProfileWillBeCreated,
}

/// Successful P3:4 application evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DrsApplyReport {
    pub backup: DrsBackup,
    pub verified_settings: usize,
}

/// Failure that identifies the guarded DRS operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DrsError {
    operation: &'static str,
    reason: String,
}

impl DrsError {
    #[must_use]
    pub fn new(operation: &'static str, reason: impl Into<String>) -> Self {
        Self {
            operation,
            reason: reason.into(),
        }
    }
}

impl fmt::Display for DrsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.operation, self.reason)
    }
}

impl Error for DrsError {}

/// Narrow adapter contract for an already-validated NVAPI DRS implementation.
///
/// The native implementation must load the installed `nvapi64.dll` from the
/// Windows System32 directory using an absolute trusted identity.  It must map
/// `restore_dword(..., None)` to the SDK's exact per-setting default-restore
/// operation, never a broad profile/global reset.
pub trait NvapiDrs {
    type Session;
    type Profile: Clone + Eq;

    fn initialize(&mut self) -> Result<(), DrsError>;
    fn create_session(&mut self) -> Result<Self::Session, DrsError>;
    fn destroy_session(&mut self, session: Self::Session) -> Result<(), DrsError>;
    fn load_settings(&mut self, session: &Self::Session) -> Result<(), DrsError>;
    fn save_settings(&mut self, session: &Self::Session) -> Result<(), DrsError>;
    fn find_profile_by_name(
        &mut self,
        session: &Self::Session,
        name: &str,
    ) -> Result<Option<Self::Profile>, DrsError>;
    fn profile_name(
        &mut self,
        session: &Self::Session,
        profile: &Self::Profile,
    ) -> Result<String, DrsError>;
    fn find_application_profile(
        &mut self,
        session: &Self::Session,
        application: &str,
    ) -> Result<Option<Self::Profile>, DrsError>;
    fn create_profile(
        &mut self,
        session: &Self::Session,
        name: &str,
    ) -> Result<Self::Profile, DrsError>;
    fn bind_application(
        &mut self,
        session: &Self::Session,
        profile: &Self::Profile,
        application: &str,
    ) -> Result<(), DrsError>;
    fn delete_application(
        &mut self,
        session: &Self::Session,
        profile: &Self::Profile,
        application: &str,
    ) -> Result<(), DrsError>;
    fn delete_profile(
        &mut self,
        session: &Self::Session,
        profile: &Self::Profile,
    ) -> Result<(), DrsError>;
    fn read_dword(
        &mut self,
        session: &Self::Session,
        profile: &Self::Profile,
        id: u32,
    ) -> Result<Option<u32>, DrsError>;
    fn set_dword(
        &mut self,
        session: &Self::Session,
        profile: &Self::Profile,
        id: u32,
        value: u32,
    ) -> Result<(), DrsError>;
    fn restore_dword(
        &mut self,
        session: &Self::Session,
        profile: &Self::Profile,
        original: DrsOriginalSetting,
    ) -> Result<(), DrsError>;
}

/// Reads whether P1:20 has a dedicated CS2 target without mutating DRS.
pub fn prepare_cs2_profile<A: NvapiDrs>(api: &mut A) -> Result<DrsPreparation, DrsError> {
    with_session(api, |api, session| {
        let profile = find_existing_profile(api, session)?;
        Ok(match profile {
            Some(profile) => DrsPreparation::ExistingProfile {
                profile: api.profile_name(session, &profile)?,
            },
            None => DrsPreparation::DedicatedProfileWillBeCreated,
        })
    })
}

/// Captures every mutable DRS datum before the profile is changed.
///
/// This is deliberately read-only. A caller must durably persist the returned
/// record before passing it to [`apply_cs2_profile`].
pub fn capture_cs2_backup<A: NvapiDrs>(api: &mut A) -> Result<DrsBackup, DrsError> {
    with_session(api, |api, session| {
        let profile = find_existing_profile(api, session)?;
        let profile_name = match profile.as_ref() {
            Some(profile) => api.profile_name(session, profile)?,
            None => CS2_PROFILE_NAME.into(),
        };
        let applications = capture_application_originals(api, session, profile.as_ref())?;
        let settings = match profile.as_ref() {
            Some(profile) => capture_originals(api, session, profile)?,
            None => CS2_SETTINGS
                .iter()
                .map(|setting| DrsOriginalSetting {
                    id: setting.id,
                    value: None,
                })
                .collect(),
        };
        Ok(DrsBackup {
            profile: profile_name,
            profile_created: profile.is_none(),
            settings,
            applications,
        })
    })
}

/// Applies the exact policy only after the caller has durably stored `backup`.
pub fn apply_cs2_profile<A: NvapiDrs>(
    api: &mut A,
    backup: &DrsBackup,
) -> Result<DrsApplyReport, DrsError> {
    validate_backup(backup)?;
    with_session(api, |api, session| {
        validate_current_originals(api, session, backup)?;
        let profile = if backup.profile_created {
            api.create_profile(session, &backup.profile)?
        } else {
            api.find_profile_by_name(session, &backup.profile)?
                .ok_or_else(|| DrsError::new("apply DRS", "captured profile no longer exists"))?
        };
        ensure_missing_applications(api, session, &profile, &backup.applications)?;
        for setting in CS2_SETTINGS {
            api.set_dword(session, &profile, setting.id, setting.value)?;
        }
        api.save_settings(session)?;
        api.load_settings(session)?;
        verify_settings(api, session, &profile, &CS2_SETTINGS)?;
        Ok(DrsApplyReport {
            backup: backup.clone(),
            verified_settings: CS2_SETTINGS.len(),
        })
    })
}

/// Restores exactly the captured values, or deletes a profile this transaction made.
pub fn restore_cs2_profile<A: NvapiDrs>(api: &mut A, backup: &DrsBackup) -> Result<(), DrsError> {
    validate_backup(backup)?;
    with_session(api, |api, session| {
        let profile = api
            .find_profile_by_name(session, &backup.profile)?
            .ok_or_else(|| DrsError::new("restore DRS", "captured profile no longer exists"))?;
        if backup.profile_created {
            api.delete_profile(session, &profile)?;
            api.save_settings(session)?;
            if api
                .find_profile_by_name(session, &backup.profile)?
                .is_some()
            {
                return Err(DrsError::new(
                    "verify DRS restore",
                    "profile created by this transaction still exists after deletion",
                ));
            }
            return Ok(());
        }
        for original in &backup.settings {
            api.restore_dword(session, &profile, *original)?;
        }
        for application in &backup.applications {
            if application.profile.is_none() {
                api.delete_application(session, &profile, &application.application)?;
            }
        }
        api.save_settings(session)?;
        api.load_settings(session)?;
        for original in &backup.settings {
            let actual = api.read_dword(session, &profile, original.id)?;
            if actual != original.value {
                return Err(DrsError::new(
                    "verify DRS restore",
                    format!(
                        "setting {:#010x} did not return to its captured value",
                        original.id
                    ),
                ));
            }
        }
        for application in &backup.applications {
            let actual = api.find_application_profile(session, &application.application)?;
            if application.profile.is_none() && actual.is_some() {
                return Err(DrsError::new(
                    "verify DRS restore",
                    format!("suite-added {} remains registered", application.application),
                ));
            }
        }
        Ok(())
    })
}

/// Re-reads the policy after application without changing DRS state.
pub fn verify_cs2_profile<A: NvapiDrs>(api: &mut A, backup: &DrsBackup) -> Result<(), DrsError> {
    validate_backup(backup)?;
    with_session(api, |api, session| {
        let profile = api
            .find_profile_by_name(session, &backup.profile)?
            .ok_or_else(|| DrsError::new("verify DRS", "target profile is missing"))?;
        verify_settings(api, session, &profile, &CS2_SETTINGS)?;
        for application in &backup.applications {
            if api.find_application_profile(session, &application.application)?
                != Some(profile.clone())
            {
                return Err(DrsError::new(
                    "verify DRS application binding",
                    format!(
                        "{} is not bound to the target profile",
                        application.application
                    ),
                ));
            }
        }
        Ok(())
    })
}

fn with_session<A: NvapiDrs, T>(
    api: &mut A,
    operation: impl FnOnce(&mut A, &A::Session) -> Result<T, DrsError>,
) -> Result<T, DrsError> {
    api.initialize()?;
    let session = api.create_session()?;
    let result = api
        .load_settings(&session)
        .and_then(|()| operation(api, &session));
    let destroy_result = api.destroy_session(session);
    match (result, destroy_result) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

fn find_existing_profile<A: NvapiDrs>(
    api: &mut A,
    session: &A::Session,
) -> Result<Option<A::Profile>, DrsError> {
    if let Some(profile) = api.find_application_profile(session, CS2_APPLICATIONS[0])?
        && api.profile_name(session, &profile)? != GLOBAL_PROFILE_NAME
    {
        return Ok(Some(profile));
    }
    for name in CS2_PROFILE_ALIASES {
        if let Some(profile) = api.find_profile_by_name(session, name)? {
            return Ok(Some(profile));
        }
    }
    Ok(None)
}

fn capture_application_originals<A: NvapiDrs>(
    api: &mut A,
    session: &A::Session,
    target: Option<&A::Profile>,
) -> Result<Vec<DrsApplicationOriginal>, DrsError> {
    for application in CS2_APPLICATIONS {
        let owner = api.find_application_profile(session, application)?;
        match (&owner, target) {
            (Some(owner), Some(profile)) if owner == profile => {}
            (Some(_), _) => {
                return Err(DrsError::new(
                    "validate CS2 application binding",
                    format!("{application} belongs to a different DRS profile"),
                ));
            }
            (None, _) => {}
        }
    }
    CS2_APPLICATIONS
        .iter()
        .map(|application| {
            Ok(DrsApplicationOriginal {
                application: (*application).into(),
                profile: api
                    .find_application_profile(session, application)?
                    .map(|owner| api.profile_name(session, &owner))
                    .transpose()?,
            })
        })
        .collect()
}

fn ensure_missing_applications<A: NvapiDrs>(
    api: &mut A,
    session: &A::Session,
    profile: &A::Profile,
    originals: &[DrsApplicationOriginal],
) -> Result<(), DrsError> {
    for original in originals {
        if original.profile.is_none() {
            api.bind_application(session, profile, &original.application)?;
        }
        if api.find_application_profile(session, &original.application)? != Some(profile.clone()) {
            return Err(DrsError::new(
                "verify CS2 application binding",
                format!(
                    "{} is not bound to the target DRS profile",
                    original.application
                ),
            ));
        }
    }
    Ok(())
}

fn validate_backup(backup: &DrsBackup) -> Result<(), DrsError> {
    let settings_complete = backup.settings.len() == CS2_SETTINGS.len()
        && backup
            .settings
            .iter()
            .zip(CS2_SETTINGS)
            .all(|(original, target)| original.id == target.id);
    let applications_complete = backup.applications.len() == CS2_APPLICATIONS.len()
        && backup
            .applications
            .iter()
            .zip(CS2_APPLICATIONS)
            .all(|(original, expected)| original.application == expected);
    if backup.profile.is_empty() || !settings_complete || !applications_complete {
        return Err(DrsError::new("validate DRS backup", "backup is incomplete"));
    }
    if backup.profile_created
        && backup
            .applications
            .iter()
            .any(|original| original.profile.is_some())
    {
        return Err(DrsError::new(
            "validate DRS backup",
            "new-profile backup has a preexisting application binding",
        ));
    }
    if !backup.profile_created
        && backup.applications.iter().any(|original| {
            original
                .profile
                .as_deref()
                .is_some_and(|profile| profile != backup.profile)
        })
    {
        return Err(DrsError::new(
            "validate DRS backup",
            "application binding does not belong to the captured profile",
        ));
    }
    Ok(())
}

fn validate_current_originals<A: NvapiDrs>(
    api: &mut A,
    session: &A::Session,
    backup: &DrsBackup,
) -> Result<(), DrsError> {
    let profile = api.find_profile_by_name(session, &backup.profile)?;
    if backup.profile_created != profile.is_none() {
        return Err(DrsError::new(
            "apply DRS",
            "profile state changed after the durable backup was captured",
        ));
    }
    for original in &backup.applications {
        let actual = api
            .find_application_profile(session, &original.application)?
            .map(|profile| api.profile_name(session, &profile))
            .transpose()?;
        if actual != original.profile {
            return Err(DrsError::new(
                "apply DRS",
                format!("{} binding changed after backup", original.application),
            ));
        }
    }
    if let Some(profile) = profile {
        for original in &backup.settings {
            if api.read_dword(session, &profile, original.id)? != original.value {
                return Err(DrsError::new(
                    "apply DRS",
                    format!("setting {:#010x} changed after backup", original.id),
                ));
            }
        }
    }
    Ok(())
}

fn capture_originals<A: NvapiDrs>(
    api: &mut A,
    session: &A::Session,
    profile: &A::Profile,
) -> Result<Vec<DrsOriginalSetting>, DrsError> {
    CS2_SETTINGS
        .iter()
        .map(|setting| {
            Ok(DrsOriginalSetting {
                id: setting.id,
                value: api.read_dword(session, profile, setting.id)?,
            })
        })
        .collect()
}

fn verify_settings<A: NvapiDrs>(
    api: &mut A,
    session: &A::Session,
    profile: &A::Profile,
    settings: &[DrsTargetSetting],
) -> Result<(), DrsError> {
    for setting in settings {
        if api.read_dword(session, profile, setting.id)? != Some(setting.value) {
            return Err(DrsError::new(
                "verify DRS settings",
                format!(
                    "setting {:#010x} differs from the saved CS2 policy",
                    setting.id
                ),
            ));
        }
    }
    Ok(())
}
