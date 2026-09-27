//! Fixed-authority NVIDIA artifact acquisition and retained-handle proof.

use std::collections::BTreeMap;

use frametime_domain::driver::{
    AdapterFailure, ArtifactLocator, AuthenticodeEvidence, AuthenticodeStatus, ExactGpuIdentity,
    GpuVendor, Sha256Digest, SignedArtifactDescriptor,
};

use super::{
    DRIVER_ARTIFACTS_LEAF, DriverArtifactStore, MAX_NVIDIA_ARTIFACT_BYTES, NvidiaArtifactLocation,
    NvidiaSignatureVerifier, VerifiedDriverArtifact, adapter, artifact,
};

/// Acquires only a source-policy-approved installer into the protected root.
/// The returned capability retains the file authority needed for installation.
pub struct NvidiaArtifactAcquirer<S, V> {
    store: S,
    verifier: V,
    location: NvidiaArtifactLocation,
}

impl<S, V> NvidiaArtifactAcquirer<S, V> {
    #[must_use]
    pub fn new(store: S, verifier: V, location: NvidiaArtifactLocation) -> Self {
        Self {
            store,
            verifier,
            location,
        }
    }
}

impl<S: DriverArtifactStore, V: NvidiaSignatureVerifier> NvidiaArtifactAcquirer<S, V> {
    pub fn acquire_verified(
        &self,
        locator: &ArtifactLocator,
        target: &ExactGpuIdentity,
    ) -> Result<(VerifiedDriverArtifact, SignedArtifactDescriptor), AdapterFailure> {
        locator
            .validate()
            .map_err(|error| adapter("acquire NVIDIA artifact", error.to_string()))?;
        if target.vendor != GpuVendor::Nvidia {
            return Err(adapter(
                "acquire driver",
                "AMD and Intel installation are unsupported without a signed artifact policy",
            ));
        }
        self.location.validate(locator)?;
        let protected_leaf = format!("{DRIVER_ARTIFACTS_LEAF}/{}", locator.artifact_file_name);
        let artifact =
            self.store
                .acquire(&self.location, &protected_leaf, MAX_NVIDIA_ARTIFACT_BYTES)?;
        let artifact_length = usize::try_from(artifact.length).map_err(|_| {
            adapter(
                "acquire NVIDIA artifact",
                "artifact length exceeds the supported address space",
            )
        })?;
        if artifact.protected_leaf != protected_leaf
            || artifact.length == 0
            || artifact_length > MAX_NVIDIA_ARTIFACT_BYTES
        {
            return Err(adapter(
                "acquire NVIDIA artifact",
                "artifact length is outside the bounded policy",
            ));
        }
        let (subject, thumbprint) = self.verifier.verify_nvidia(&artifact)?;
        if !artifact::accepts_compiled_nvidia_policy(&subject, &thumbprint) {
            return Err(adapter(
                "acquire NVIDIA artifact",
                "WinVerifyTrust signer violates exact NVIDIA policy",
            ));
        }
        let signer = Sha256Digest::parse(thumbprint.to_ascii_lowercase())
            .map_err(|error| adapter("verify NVIDIA artifact", error.to_string()))?;
        let descriptor = SignedArtifactDescriptor {
            locator: locator.clone(),
            target_gpu: target.clone(),
            payload_sha256: artifact.payload_sha256.clone(),
            authenticode: AuthenticodeEvidence {
                status: AuthenticodeStatus::Valid,
                signer_subject: subject,
                signer_thumbprint_sha256: signer,
                observed_at_utc: artifact::trusted_utc_timestamp(),
                extensions: BTreeMap::new(),
            },
            extensions: BTreeMap::new(),
        };
        artifact.revalidate()?;
        Ok((artifact, descriptor))
    }
}
