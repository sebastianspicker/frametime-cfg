//! Exact identifiers used for descriptive overlap observations.

// Catalog v1, reviewed 2026-08-10. These are public executable or
// service/driver identities for third-party hardware-control utilities. Their
// presence is never evidence of hardware access or causation.
pub(super) const PROCESS_IDENTIFIERS: &[&str] = &[
    "msiafterburner.exe",
    "rtss.exe",
    "precisionx_x64.exe",
    "precisionxserver.exe",
    "gpu-tweak-iii.exe",
    "aorusengine.exe",
];

pub(super) const SERVICE_IDENTIFIERS: &[&str] = &[
    "rtcore64",
    "rtcore32",
    "precisionxserver",
    "asusgpufanservice",
    "aorusengine",
];
