/// A requested NVAPI DRS DWORD setting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DrsTargetSetting {
    pub id: u32,
    pub value: u32,
}

/// The documented 42-setting CS2 DRS policy from `docs/nvidia-drs-settings.md`.
pub const CS2_SETTINGS: [DrsTargetSetting; 42] = [
    DrsTargetSetting {
        id: 274_197_361,
        value: 1,
    },
    DrsTargetSetting {
        id: 8_102_046,
        value: 1,
    },
    DrsTargetSetting {
        id: 549_528_094,
        value: 1,
    },
    DrsTargetSetting {
        id: 553_505_273,
        value: 0,
    },
    DrsTargetSetting {
        id: 13_510_289,
        value: 20,
    },
    DrsTargetSetting {
        id: 1_686_376,
        value: 1,
    },
    DrsTargetSetting {
        id: 3_066_610,
        value: 0,
    },
    DrsTargetSetting {
        id: 8_703_344,
        value: 0,
    },
    DrsTargetSetting {
        id: 15_151_633,
        value: 0,
    },
    DrsTargetSetting {
        id: 6_524_559,
        value: 0,
    },
    DrsTargetSetting {
        id: 276_652_957,
        value: 0,
    },
    DrsTargetSetting {
        id: 276_757_595,
        value: 0,
    },
    DrsTargetSetting {
        id: 545_898_348,
        value: 0,
    },
    DrsTargetSetting {
        id: 270_426_537,
        value: 1,
    },
    DrsTargetSetting {
        id: 282_245_910,
        value: 0,
    },
    DrsTargetSetting {
        id: 276_089_202,
        value: 0,
    },
    DrsTargetSetting {
        id: 271_895_433,
        value: 0,
    },
    DrsTargetSetting {
        id: 11_041_231,
        value: 138_504_007,
    },
    DrsTargetSetting {
        id: 6_600_001,
        value: 1,
    },
    DrsTargetSetting {
        id: 277_041_152,
        value: 0,
    },
    DrsTargetSetting {
        id: 277_041_154,
        value: 0,
    },
    DrsTargetSetting {
        id: 277_041_162,
        value: 500,
    },
    DrsTargetSetting {
        id: 278_196_567,
        value: 0,
    },
    DrsTargetSetting {
        id: 278_196_727,
        value: 0,
    },
    DrsTargetSetting {
        id: 279_476_652,
        value: 1,
    },
    DrsTargetSetting {
        id: 279_476_687,
        value: 1,
    },
    DrsTargetSetting {
        id: 294_973_784,
        value: 0,
    },
    DrsTargetSetting {
        id: 5_912_412,
        value: 2_525_368_439,
    },
    DrsTargetSetting {
        id: 276_158_834,
        value: 0,
    },
    DrsTargetSetting {
        id: 271_965_065,
        value: 0,
    },
    DrsTargetSetting {
        id: 284_810_369,
        value: 17,
    },
    DrsTargetSetting {
        id: 284_810_372,
        value: 16_777_216,
    },
    DrsTargetSetting {
        id: 983_226,
        value: 0,
    },
    DrsTargetSetting {
        id: 983_227,
        value: 0,
    },
    DrsTargetSetting {
        id: 11_306_135,
        value: 10_240,
    },
    DrsTargetSetting {
        id: 270_198_627,
        value: 0,
    },
    DrsTargetSetting {
        id: 390_467,
        value: 1,
    },
    DrsTargetSetting {
        id: 14_566_042,
        value: 0,
    },
    DrsTargetSetting {
        id: 274_606_621,
        value: 4,
    },
    DrsTargetSetting {
        id: 549_198_379,
        value: 0,
    },
    DrsTargetSetting {
        id: 1_343_646_814,
        value: 0,
    },
    DrsTargetSetting {
        id: 2_156_231_208,
        value: 1,
    },
];
