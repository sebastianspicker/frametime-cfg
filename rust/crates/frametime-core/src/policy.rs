use serde::{Deserialize, Serialize};

use crate::catalog::Risk;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum Profile {
    Safe,
    Recommended,
    Competitive,
    Custom,
    Yolo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Auto,
    Prompt,
    Skip,
}

impl Profile {
    #[must_use]
    pub const fn maximum_risk(self) -> Risk {
        match self {
            Self::Safe => Risk::Safe,
            Self::Recommended => Risk::Moderate,
            Self::Competitive | Self::Yolo => Risk::Aggressive,
            Self::Custom => Risk::Critical,
        }
    }

    #[must_use]
    pub const fn decision(self, tier: u8, risk: Risk) -> Decision {
        // Legacy T1 baseline operations bypass the profile risk ceiling.
        if tier == 1 {
            return match self {
                Self::Custom => Decision::Prompt,
                _ => Decision::Auto,
            };
        }
        if risk.rank() > self.maximum_risk().rank() {
            return Decision::Skip;
        }
        match self {
            Self::Safe => {
                if tier == 2 && matches!(risk, Risk::Safe) {
                    Decision::Auto
                } else {
                    Decision::Skip
                }
            }
            Self::Recommended => {
                if tier == 2 {
                    Decision::Prompt
                } else {
                    Decision::Skip
                }
            }
            Self::Competitive => Decision::Prompt,
            Self::Custom => Decision::Prompt,
            Self::Yolo => Decision::Auto,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_matrix_matches_legacy_contract() {
        assert_eq!(Profile::Safe.decision(2, Risk::Safe), Decision::Auto);
        assert_eq!(Profile::Safe.decision(2, Risk::Moderate), Decision::Skip);
        assert_eq!(
            Profile::Recommended.decision(2, Risk::Moderate),
            Decision::Prompt
        );
        assert_eq!(Profile::Recommended.decision(3, Risk::Safe), Decision::Skip);
        assert_eq!(
            Profile::Competitive.decision(3, Risk::Aggressive),
            Decision::Prompt
        );
        assert_eq!(
            Profile::Custom.decision(1, Risk::Critical),
            Decision::Prompt
        );
        assert_eq!(Profile::Yolo.decision(3, Risk::Critical), Decision::Skip);
        assert_eq!(Profile::Safe.decision(1, Risk::Critical), Decision::Auto);
    }
}
