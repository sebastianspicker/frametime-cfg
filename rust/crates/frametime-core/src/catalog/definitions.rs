use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Phase {
    One = 1,
    Two = 2,
    Three = 3,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "UPPERCASE")]
pub enum Risk {
    Safe,
    Moderate,
    Aggressive,
    Critical,
}

impl Risk {
    pub(crate) const fn rank(self) -> u8 {
        self as u8
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum Depth {
    Setup,
    Check,
    Registry,
    Service,
    Boot,
    Driver,
    Network,
    Filesystem,
    App,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Step {
    pub phase: Phase,
    pub number: u8,
    pub category: &'static str,
    pub title: &'static str,
    pub tier: u8,
    pub risk: Risk,
    pub depth: Depth,
    pub check_only: bool,
    pub reboot: bool,
}
