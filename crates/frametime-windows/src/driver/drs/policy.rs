/// A requested NVAPI DRS DWORD setting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DrsTargetSetting {
    pub id: u32,
    pub value: u32,
}

/// Minimal per-application CS2 DRS baseline documented in
/// `docs/nvidia-drs-settings.md`.
///
/// Display synchronization, frame caps, low-latency mode, and performance
/// policy remain driver or game controlled. The transaction API has no
/// goal-specific display input, so it must not choose a raw-latency or VRR
/// strategy on the operator's behalf.
pub const CS2_SETTINGS: [DrsTargetSetting; 5] = [
    // Application-controlled anti-aliasing.
    DrsTargetSetting {
        id: 276_757_595,
        value: 0,
    },
    // Application-controlled anisotropic filtering and its companion mode.
    DrsTargetSetting {
        id: 270_426_537,
        value: 1,
    },
    DrsTargetSetting {
        id: 282_245_910,
        value: 0,
    },
    // Highest refresh rate reported by the driver.
    DrsTargetSetting {
        id: 6_600_001,
        value: 1,
    },
    // Keep a sufficiently sized shader cache without clearing it routinely.
    DrsTargetSetting {
        id: 11_306_135,
        value: 10_240,
    },
];

#[cfg(test)]
mod tests {
    use super::CS2_SETTINGS;

    #[test]
    pub(crate) fn cs2_policy_is_the_minimal_documented_baseline() {
        assert_eq!(
            CS2_SETTINGS.map(|setting| (setting.id, setting.value)),
            [
                (276_757_595, 0),
                (270_426_537, 1),
                (282_245_910, 0),
                (6_600_001, 1),
                (11_306_135, 10_240),
            ]
        );
    }

    #[test]
    pub(crate) fn cs2_policy_leaves_goal_specific_and_unrelated_driver_state_untouched() {
        const EXCLUDED_IDS: [u32; 23] = [
            274_197_361,   // power management
            549_528_094,   // threaded optimization
            11_041_231,    // V-Sync
            277_041_152,   // frame limiter low latency
            277_041_154,   // legacy frame limiter
            277_041_162,   // NVCPL frame limiter
            278_196_567,   // VRR
            278_196_727,   // VRR requested state
            279_476_652,   // G-SYNC
            279_476_687,   // G-SYNC secondary
            294_973_784,   // global G-SYNC
            5_912_412,     // V-Sync tear control
            983_226,       // ReBAR enable
            983_227,       // ReBAR options
            390_467,       // Ultra Low Latency Mode
            14_566_042,    // DXR
            276_158_834,   // Ansel
            271_965_065,   // predefined Ansel use
            274_606_621,   // Ansel/Freestyle
            549_198_379,   // Vulkan ray tracing
            1_343_646_814, // CUDA stable performance limit
            270_198_627,   // SLI/AFR
            2_156_231_208, // monitor-usage flag
        ];

        for id in EXCLUDED_IDS {
            assert!(CS2_SETTINGS.iter().all(|setting| setting.id != id));
        }
    }
}
