use crate::{DeviceIdentity, Measurement, NorthclockError, Result};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

const MAX_TEST_BYTES: usize = 512 * 1024 * 1024;
const MIN_TEST_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MemoryTestConfig {
    pub bytes: usize,
    pub passes: u32,
    pub timeout_ms: u64,
}

impl Default for MemoryTestConfig {
    fn default() -> Self {
        Self {
            bytes: 64 * 1024 * 1024,
            passes: 2,
            timeout_ms: 30_000,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MemoryTestReport {
    pub tested_bytes: usize,
    pub completed_passes: u32,
    pub errors: u64,
    pub timed_out: bool,
    pub elapsed_ms: u128,
    pub throughput: Measurement<f64>,
}

pub fn run_system_memory_test(config: MemoryTestConfig) -> Result<MemoryTestReport> {
    if !(MIN_TEST_BYTES..=MAX_TEST_BYTES).contains(&config.bytes) {
        return Err(NorthclockError::InvalidUsage(format!(
            "memory test size must be between {MIN_TEST_BYTES} and {MAX_TEST_BYTES} bytes"
        )));
    }
    if config.passes == 0 || config.passes > 100 {
        return Err(NorthclockError::InvalidUsage(
            "memory test passes must be between 1 and 100".into(),
        ));
    }
    if config.timeout_ms == 0 {
        return Err(NorthclockError::InvalidUsage(
            "memory test timeout must be non-zero".into(),
        ));
    }

    let mut buffer = vec![0_u64; config.bytes.div_ceil(8)];
    let deadline = Duration::from_millis(config.timeout_ms);
    let started = Instant::now();
    let mut errors = 0_u64;
    let mut completed_passes = 0_u32;
    let mut timed_out = false;

    for pass in 0..config.passes {
        let salt = u64::from(pass).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        for (index, slot) in buffer.iter_mut().enumerate() {
            *slot = pattern(index, salt);
        }
        for (index, slot) in buffer.iter().enumerate() {
            if *slot != pattern(index, salt) {
                errors = errors.saturating_add(1);
            }
        }
        completed_passes += 1;
        if started.elapsed() >= deadline {
            timed_out = completed_passes < config.passes;
            break;
        }
    }

    let elapsed = started.elapsed();
    let processed = (config.bytes as f64) * f64::from(completed_passes) * 2.0;
    let throughput_mib_s = processed / elapsed.as_secs_f64().max(f64::EPSILON) / 1_048_576.0;
    let device = DeviceIdentity::new("system_memory", "system-memory", "System memory", None);
    let throughput = Measurement::now(
        throughput_mib_s,
        "MiB/s",
        device,
        "northclock-bounded-memory-workload",
    )?;

    Ok(MemoryTestReport {
        tested_bytes: config.bytes,
        completed_passes,
        errors,
        timed_out,
        elapsed_ms: elapsed.as_millis(),
        throughput,
    })
}

fn pattern(index: usize, salt: u64) -> u64 {
    (index as u64)
        .wrapping_mul(0xD6E8_FEB8_6659_FD93)
        .rotate_left(17)
        ^ salt
        ^ 0xA5A5_5A5A_C3C3_3C3C
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_test_measures_real_work() {
        let report = run_system_memory_test(MemoryTestConfig {
            bytes: MIN_TEST_BYTES,
            passes: 1,
            timeout_ms: 5_000,
        })
        .unwrap_or_else(|error| panic!("memory test failed: {error}"));
        assert_eq!(report.errors, 0);
        assert_eq!(report.completed_passes, 1);
        assert_eq!(
            report.throughput.source,
            "northclock-bounded-memory-workload"
        );
        assert!(report.throughput.value > 0.0);
    }

    #[test]
    fn rejects_unbounded_allocation() {
        let error = run_system_memory_test(MemoryTestConfig {
            bytes: MAX_TEST_BYTES + 1,
            ..MemoryTestConfig::default()
        })
        .err()
        .unwrap_or_else(|| panic!("oversized workload unexpectedly succeeded"));
        assert_eq!(error.exit_code(), 2);
    }
}
