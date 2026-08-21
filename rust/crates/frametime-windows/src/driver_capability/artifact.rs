use frametime_driver::{
    AdapterFailure, ArtifactAcquisitionAuthorization, AuthenticodeEvidence, AuthenticodeStatus,
    Sha256Digest, SignedArtifactDescriptor,
};

#[cfg(windows)]
use super::native;
use super::{
    DRIVER_ARTIFACTS_LEAF, NVIDIA_SIGNER_SHA256, NVIDIA_SUBJECT, NvidiaArtifactLocation,
    ProcessOutcome, adapter,
};

/// A non-cloneable file capability. It is the only value accepted by the
/// installer, keeping the validated handle alive across verification and child
/// execution. It intentionally exposes neither a writable handle nor bytes.
#[derive(Debug)]
pub struct VerifiedDriverArtifact {
    pub(super) protected_leaf: String,
    pub(super) length: u64,
    pub(super) payload_sha256: Sha256Digest,
    #[cfg(windows)]
    pub(super) retained: Option<native::RetainedArtifact>,
}

impl VerifiedDriverArtifact {
    #[must_use]
    pub fn payload_sha256(&self) -> &Sha256Digest {
        &self.payload_sha256
    }

    pub(super) fn validate_descriptor(
        &self,
        descriptor: &SignedArtifactDescriptor,
    ) -> Result<(), AdapterFailure> {
        let expected = format!(
            "{DRIVER_ARTIFACTS_LEAF}/{}",
            descriptor.locator.artifact_file_name
        );
        if self.protected_leaf != expected || self.payload_sha256 != descriptor.payload_sha256 {
            return Err(adapter(
                "install NVIDIA artifact",
                "artifact capability does not match descriptor",
            ));
        }
        self.revalidate()
    }

    pub(crate) fn revalidate(&self) -> Result<(), AdapterFailure> {
        #[cfg(windows)]
        if let Some(retained) = &self.retained {
            return retained.revalidate(&self.payload_sha256, self.length);
        }
        #[cfg(not(windows))]
        Err(adapter("revalidate artifact", "supported only on Windows"))
    }

    #[cfg(windows)]
    pub(super) fn retained(&self) -> Result<&native::RetainedArtifact, AdapterFailure> {
        self.retained.as_ref().ok_or_else(|| {
            adapter(
                "use retained artifact",
                "test-only artifact has no Windows file capability",
            )
        })
    }
}

/// The source returns an owned capability, not detached bytes. Native sources
/// must create/open only the fixed protected artifact child.
pub trait DriverArtifactStore {
    fn acquire(
        &self,
        location: &NvidiaArtifactLocation,
        protected_leaf: &str,
        maximum_bytes: usize,
    ) -> Result<VerifiedDriverArtifact, AdapterFailure>;
}

#[derive(Debug)]
pub struct NativeNvidiaArtifactStore {
    root: crate::TrustedWorkDir,
}
impl NativeNvidiaArtifactStore {
    pub fn acquire_fixed_root() -> Result<Self, AdapterFailure> {
        Ok(Self {
            root: crate::TrustedWorkDir::acquire_fixed()
                .map_err(|e| adapter("open artifact store", e))?,
        })
    }
}
impl DriverArtifactStore for NativeNvidiaArtifactStore {
    fn acquire(
        &self,
        location: &NvidiaArtifactLocation,
        protected_leaf: &str,
        maximum_bytes: usize,
    ) -> Result<VerifiedDriverArtifact, AdapterFailure> {
        #[cfg(windows)]
        return native::acquire(&self.root, location, protected_leaf, maximum_bytes);
        #[cfg(not(windows))]
        {
            let _ = (&self.root, location, protected_leaf, maximum_bytes);
            Err(adapter("acquire artifact", "supported only on Windows"))
        }
    }
}

const MAX_NVIDIA_AUTHORIZATION_SECONDS: i64 = 24 * 60 * 60;

#[cfg(windows)]
pub(crate) fn authorization_expiry_after(
    authorized_at_utc: &str,
) -> Result<String, AdapterFailure> {
    let authorized = parse_utc_seconds(authorized_at_utc)?;
    format_utc_seconds(
        authorized
            .checked_add(MAX_NVIDIA_AUTHORIZATION_SECONDS)
            .ok_or_else(|| adapter("authorize NVIDIA artifact", "authorization expiry overflow"))?,
    )
}

pub(crate) fn validate_bounded_nvidia_authorization(
    authorization: &ArtifactAcquisitionAuthorization,
    artifact: &SignedArtifactDescriptor,
    now_utc: &str,
) -> Result<(), AdapterFailure> {
    if authorization.schema_version != frametime_driver::SCHEMA_VERSION {
        return Err(adapter(
            "authorize NVIDIA artifact",
            "authorization schema is unsupported",
        ));
    }
    authorization
        .artifact
        .validate_matches(artifact)
        .map_err(|error| adapter("authorize NVIDIA artifact", error.to_string()))?;
    let authorized = parse_utc_seconds(&authorization.authorized_at_utc)?;
    let expires = parse_utc_seconds(&authorization.expires_at_utc)?;
    let now = parse_utc_seconds(now_utc)?;
    if authorized > now
        || expires < now
        || expires <= authorized
        || expires - authorized > MAX_NVIDIA_AUTHORIZATION_SECONDS
    {
        return Err(adapter(
            "authorize NVIDIA artifact",
            "authorization is expired, not yet valid, or exceeds the 24-hour limit",
        ));
    }
    Ok(())
}

pub(super) fn accepts_compiled_nvidia_policy(subject: &str, thumbprint: &str) -> bool {
    subject == NVIDIA_SUBJECT
        && (NVIDIA_SIGNER_SHA256.contains(&thumbprint.to_ascii_lowercase().as_str())
            || (cfg!(test)
                && thumbprint
                    == "abababababababababababababababababababababababababababababababab"))
}

pub(super) fn verify_nvidia_signature_against_policy<V: NvidiaSignatureVerifier>(
    verifier: &V,
    capability: &VerifiedDriverArtifact,
    artifact: &SignedArtifactDescriptor,
    observed_at_utc: &str,
) -> Result<AuthenticodeEvidence, AdapterFailure> {
    let (subject, thumbprint) = verifier.verify_nvidia(capability)?;
    if !accepts_compiled_nvidia_policy(&subject, &thumbprint) {
        return Err(adapter(
            "install NVIDIA artifact",
            "fresh WinVerifyTrust signer violates exact NVIDIA policy",
        ));
    }
    let thumbprint = Sha256Digest::parse(thumbprint.to_ascii_lowercase())
        .map_err(|error| adapter("verify NVIDIA artifact", error.to_string()))?;
    if subject != artifact.authenticode.signer_subject
        || thumbprint != artifact.authenticode.signer_thumbprint_sha256
    {
        return Err(adapter(
            "install NVIDIA artifact",
            "fresh WinVerifyTrust signer does not match acquisition evidence",
        ));
    }
    Ok(AuthenticodeEvidence {
        status: AuthenticodeStatus::Valid,
        signer_subject: subject,
        signer_thumbprint_sha256: thumbprint,
        observed_at_utc: observed_at_utc.into(),
        extensions: Default::default(),
    })
}

fn parse_utc_seconds(value: &str) -> Result<i64, AdapterFailure> {
    let bytes = value.as_bytes();
    if bytes.len() != 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes[19] != b'Z'
    {
        return Err(adapter(
            "authorize NVIDIA artifact",
            "timestamp must be UTC RFC3339",
        ));
    }
    let number = |start: usize, end: usize| {
        bytes[start..end]
            .iter()
            .try_fold(0_i64, |number, byte| match byte {
                b'0'..=b'9' => Ok(number * 10 + i64::from(byte - b'0')),
                _ => Err(adapter(
                    "authorize NVIDIA artifact",
                    "timestamp contains non-digits",
                )),
            })
    };
    let year = number(0, 4)?;
    let month = number(5, 7)?;
    let day = number(8, 10)?;
    let hour = number(11, 13)?;
    let minute = number(14, 16)?;
    let second = number(17, 19)?;
    let days_in_month = days_in_month(year, month).ok_or_else(|| {
        adapter(
            "authorize NVIDIA artifact",
            "timestamp has an invalid calendar date",
        )
    })?;
    if day == 0 || day > days_in_month || hour > 23 || minute > 59 || second > 59 {
        return Err(adapter(
            "authorize NVIDIA artifact",
            "timestamp has an invalid time",
        ));
    }
    let completed_years = year - 1;
    let month_days = [0_i64, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let mut days = completed_years * 365 + completed_years / 4 - completed_years / 100
        + completed_years / 400
        + month_days[(month - 1) as usize]
        + day
        - 1;
    if month > 2 && is_leap_year(year) {
        days += 1;
    }
    Ok(days * 86_400 + hour * 3_600 + minute * 60 + second)
}

#[cfg(windows)]
fn format_utc_seconds(seconds: i64) -> Result<String, AdapterFailure> {
    if seconds < 0 {
        return Err(adapter(
            "authorize NVIDIA artifact",
            "timestamp predates supported UTC",
        ));
    }
    let days = seconds / 86_400;
    let time = seconds % 86_400;
    let mut year = 1_i64;
    let mut remaining_days = days;
    while remaining_days >= days_in_year(year) {
        remaining_days -= days_in_year(year);
        year += 1;
    }
    if year > 9999 {
        return Err(adapter(
            "authorize NVIDIA artifact",
            "authorization expiry exceeds UTC format",
        ));
    }
    let mut month = 1_i64;
    while remaining_days >= days_in_month(year, month).expect("month is bounded") {
        remaining_days -= days_in_month(year, month).expect("month is bounded");
        month += 1;
    }
    Ok(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z",
        day = remaining_days + 1,
        hour = time / 3_600,
        minute = time % 3_600 / 60,
        second = time % 60,
    ))
}

#[cfg(windows)]
fn days_in_year(year: i64) -> i64 {
    if is_leap_year(year) { 366 } else { 365 }
}

fn days_in_month(year: i64, month: i64) -> Option<i64> {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => Some(31),
        4 | 6 | 9 | 11 => Some(30),
        2 if is_leap_year(year) => Some(29),
        2 => Some(28),
        _ => None,
    }
}

fn is_leap_year(year: i64) -> bool {
    year > 0 && year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

#[cfg(windows)]
pub(super) fn trusted_utc_timestamp() -> String {
    use windows::Win32::System::SystemInformation::GetSystemTime;

    let now = unsafe { GetSystemTime() };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        now.wYear, now.wMonth, now.wDay, now.wHour, now.wMinute, now.wSecond
    )
}

#[cfg(not(windows))]
pub(super) fn trusted_utc_timestamp() -> String {
    crate::timestamp()
}

/// Signature verification is capability-based: a verifier cannot be handed a
/// different path or copied byte buffer after acquisition.
pub trait NvidiaSignatureVerifier {
    fn verify_nvidia(
        &self,
        artifact: &VerifiedDriverArtifact,
    ) -> Result<(String, String), AdapterFailure>;
}

#[derive(Debug, Default)]
pub struct NativeNvidiaSignatureVerifier;
impl NvidiaSignatureVerifier for NativeNvidiaSignatureVerifier {
    fn verify_nvidia(
        &self,
        artifact: &VerifiedDriverArtifact,
    ) -> Result<(String, String), AdapterFailure> {
        artifact.revalidate()?;
        #[cfg(windows)]
        return artifact.retained()?.verify_signature();
        #[cfg(not(windows))]
        {
            let _ = artifact;
            Err(adapter(
                "verify NVIDIA artifact",
                "supported only on Windows",
            ))
        }
    }
}

/// Only a capability may be launched. Implementations must keep it borrowed
/// until the child exits and must revalidate it immediately before launch.
pub trait NvidiaInstallerRunner {
    fn launch_verified(
        &self,
        artifact: &VerifiedDriverArtifact,
        argv: &[String],
    ) -> Result<ProcessOutcome, AdapterFailure>;
}

/// Native argv-only launcher for a retained, verified package executable.
#[derive(Debug, Default)]
pub struct NativeNvidiaInstallerRunner;
impl NvidiaInstallerRunner for NativeNvidiaInstallerRunner {
    fn launch_verified(
        &self,
        artifact: &VerifiedDriverArtifact,
        argv: &[String],
    ) -> Result<ProcessOutcome, AdapterFailure> {
        #[cfg(windows)]
        return native::launch(artifact, argv);
        #[cfg(not(windows))]
        {
            let _ = (artifact, argv);
            Err(adapter(
                "launch NVIDIA artifact",
                "supported only on Windows",
            ))
        }
    }
}
