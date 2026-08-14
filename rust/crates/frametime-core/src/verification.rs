#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationStatus {
    Ok,
    Changed,
    Missing,
    Info,
}

impl VerificationStatus {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::Changed => "CHANGED",
            Self::Missing => "MISSING",
            Self::Info => "INFO",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationItem {
    pub status: VerificationStatus,
    pub name: String,
    pub detail: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VerificationReport {
    pub items: Vec<VerificationItem>,
}

impl VerificationReport {
    #[must_use]
    pub fn counts(&self) -> (usize, usize, usize) {
        let mut ok = 0;
        let mut changed = 0;
        let mut missing = 0;
        for item in &self.items {
            match item.status {
                VerificationStatus::Ok => ok += 1,
                VerificationStatus::Changed => changed += 1,
                VerificationStatus::Missing => missing += 1,
                VerificationStatus::Info => {}
            }
        }
        (ok, changed, missing)
    }

    #[must_use]
    pub fn has_drift(&self) -> bool {
        let (_, changed, missing) = self.counts();
        changed > 0 || missing > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_is_not_counted_as_setting_result() {
        let report = VerificationReport {
            items: vec![
                VerificationItem {
                    status: VerificationStatus::Ok,
                    name: "one".into(),
                    detail: String::new(),
                },
                VerificationItem {
                    status: VerificationStatus::Info,
                    name: "host".into(),
                    detail: String::new(),
                },
            ],
        };
        assert_eq!(report.counts(), (1, 0, 0));
        assert!(!report.has_drift());
    }
}
