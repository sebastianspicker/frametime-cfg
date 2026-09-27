/// External launch and mutation stay unavailable unless package authentication
/// has bound the current GUI, its sibling CLI, and every packaged payload to the
/// configured release publisher while retaining the authenticated file objects.
pub const EXTERNAL_EXECUTION_UNAVAILABLE: &str = "Native CLI launch, GUI self-elevation, and GUI mutations require an authenticated package capability.";
#[derive(Debug)]
pub struct PackageAuthentication<T> {
    package: Option<T>,
    failure: Option<String>,
}
impl<T> PackageAuthentication<T> {
    pub fn authenticate(authenticate: impl FnOnce() -> Result<T, String>) -> Self {
        match authenticate() {
            Ok(package) => Self {
                package: Some(package),
                failure: None,
            },
            Err(failure) => Self {
                package: None,
                failure: Some(failure),
            },
        }
    }
    #[must_use]
    pub const fn has_capability(&self) -> bool {
        self.package.is_some()
    }
    #[must_use]
    pub fn package(&self) -> Option<&T> {
        self.package.as_ref()
    }
    #[must_use]
    pub fn unavailable_detail(&self) -> String {
        match &self.failure {
            Some(failure) => {
                format!("{EXTERNAL_EXECUTION_UNAVAILABLE} Authentication failed: {failure}")
            }
            None => EXTERNAL_EXECUTION_UNAVAILABLE.into(),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authentication_is_host_injectable_and_fail_closed() {
        let authenticated = PackageAuthentication::authenticate(|| Ok("capability"));
        assert!(authenticated.has_capability());
        assert_eq!(authenticated.package(), Some(&"capability"));
        let rejected = PackageAuthentication::<()>::authenticate(|| Err("missing pin".into()));
        assert!(!rejected.has_capability());
        assert!(rejected.package().is_none());
        assert!(rejected.unavailable_detail().contains("missing pin"));
    }
}
