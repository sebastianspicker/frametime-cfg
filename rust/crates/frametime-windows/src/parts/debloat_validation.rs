const MAX_ITEMS_PER_IDENTITY: usize = 128;
const MAX_TASKS_PER_FOLDER: usize = 256;

/// Injectable OS boundary. Each method must use an authoritative API and must
/// return an error for denied, malformed, incomplete, or ambiguous state.
pub trait DebloatHost {
    fn installed(&mut self, identity: AppxIdentity) -> Result<Vec<AppxPackageCapture>, String>;
    fn provisioned(
        &mut self,
        identity: AppxIdentity,
    ) -> Result<Vec<ProvisionedPackageCapture>, String>;
    fn service(&mut self, identity: ServiceIdentity) -> Result<Option<ServiceCapture>, String>;
    fn tasks(&mut self, folder: TaskFolder) -> Result<Vec<TaskCapture>, String>;
    fn policy(&mut self, identity: PolicyIdentity) -> Result<Option<RegistryValue>, String>;
    fn remove_installed(&mut self, package: &AppxPackageCapture) -> Result<(), String>;
    fn remove_provisioned(&mut self, package: &ProvisionedPackageCapture) -> Result<(), String>;
    fn stop_and_disable(&mut self, identity: ServiceIdentity) -> Result<(), String>;
    fn disable_task(&mut self, task: &TaskCapture) -> Result<(), String>;
    fn write_policy(
        &mut self,
        identity: PolicyIdentity,
        value: RegistryValue,
    ) -> Result<(), String>;
}

fn needs_apply(snapshot: &DebloatSnapshot) -> bool {
    !snapshot.installed.is_empty()
        || !snapshot.provisioned.is_empty()
        || snapshot.services.iter().any(|item| {
            item.start_type != ServiceStartType::Disabled || item.status != ServiceStatus::Stopped
        })
        || snapshot.tasks.iter().any(|item| item.enabled)
        || snapshot
            .policies
            .iter()
            .any(|item| item.original.as_ref() != Some(&item.identity.desired()))
}

fn preflight_installed(
    items: Vec<AppxPackageCapture>,
    expected: &AppxPackageCapture,
) -> Result<bool, String> {
    validate_appx(&items)?;
    if items.iter().any(|item| item == expected) {
        Ok(true)
    } else if items.is_empty() {
        Ok(false)
    } else {
        Err("installed AppX identity changed since capture; refusing mutation".into())
    }
}

fn preflight_provisioned(
    items: Vec<ProvisionedPackageCapture>,
    expected: &ProvisionedPackageCapture,
) -> Result<bool, String> {
    validate_provisioned(&items)?;
    if items.iter().any(|item| item == expected) {
        Ok(true)
    } else if items.is_empty() {
        Ok(false)
    } else {
        Err("provisioned AppX identity changed since capture; refusing mutation".into())
    }
}

fn exact_task(items: Vec<TaskCapture>, expected: &TaskCapture) -> Result<bool, String> {
    validate_tasks(&items)?;
    Ok(items.into_iter().any(|item| item == *expected))
}

fn validate_snapshot(snapshot: &DebloatSnapshot) -> Result<(), String> {
    validate_appx(&snapshot.installed)?;
    validate_provisioned(&snapshot.provisioned)?;
    validate_tasks(&snapshot.tasks)?;
    if snapshot
        .services
        .iter()
        .any(|item| !SERVICE_ALLOWLIST.contains(&item.identity))
        || snapshot
            .policies
            .iter()
            .map(|item| item.identity)
            .collect::<BTreeSet<_>>()
            .len()
            != POLICY_ALLOWLIST.len()
    {
        return Err("debloat capture has an unknown or incomplete fixed identity set".into());
    }
    Ok(())
}

fn validate_appx(items: &[AppxPackageCapture]) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    if items.len() > APPX_ALLOWLIST.len() * MAX_ITEMS_PER_IDENTITY
        || items.iter().any(|item| {
            !APPX_ALLOWLIST.contains(&item.identity)
                || item.full_name.is_empty()
                || !seen.insert((item.identity, item.full_name.as_str()))
        })
    {
        Err(
            "installed AppX enumeration has an unknown, empty, duplicate, or excessive identity"
                .into(),
        )
    } else {
        Ok(())
    }
}

fn validate_provisioned(items: &[ProvisionedPackageCapture]) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    if items.len() > APPX_ALLOWLIST.len() * MAX_ITEMS_PER_IDENTITY
        || items.iter().any(|item| {
            !APPX_ALLOWLIST.contains(&item.identity)
                || item.package_name.is_empty()
                || !seen.insert((item.identity, item.package_name.as_str()))
        })
    {
        Err(
            "provisioned AppX enumeration has an unknown, empty, duplicate, or excessive identity"
                .into(),
        )
    } else {
        Ok(())
    }
}

fn validate_tasks(items: &[TaskCapture]) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    if items.len() > TASK_FOLDERS.len() * MAX_TASKS_PER_FOLDER
        || items.iter().any(|item| {
            !TASK_FOLDERS.contains(&item.folder)
                || item.name.is_empty()
                || item.name.contains(['\\', '/', '\0'])
                || !seen.insert((item.folder, item.name.as_str()))
        })
    {
        Err(
            "scheduled-task enumeration has an unknown, empty, duplicate, or excessive identity"
                .into(),
        )
    } else {
        Ok(())
    }
}
