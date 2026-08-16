#[derive(Debug)]
pub(crate) enum AppError {
    Invalid(String),
    Failed(String),
}

impl AppError {
    pub(crate) fn failed(value: impl Into<String>) -> Self {
        Self::Failed(value.into())
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(value) | Self::Failed(value) => formatter.write_str(value),
        }
    }
}
