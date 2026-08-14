use std::collections::BTreeMap;

#[derive(Default)]
struct Host {
    installed: BTreeMap<AppxIdentity, Vec<AppxPackageCapture>>,
    provisioned: BTreeMap<AppxIdentity, Vec<ProvisionedPackageCapture>>,
    services: BTreeMap<ServiceIdentity, ServiceCapture>,
    tasks: BTreeMap<TaskFolder, Vec<TaskCapture>>,
    policies: BTreeMap<PolicyIdentity, RegistryValue>,
    fail_remove: bool,
}
impl DebloatHost for Host {
    fn installed(&mut self, id: AppxIdentity) -> Result<Vec<AppxPackageCapture>, String> {
        Ok(self.installed.get(&id).cloned().unwrap_or_default())
    }
    fn provisioned(&mut self, id: AppxIdentity) -> Result<Vec<ProvisionedPackageCapture>, String> {
        Ok(self.provisioned.get(&id).cloned().unwrap_or_default())
    }
    fn service(&mut self, id: ServiceIdentity) -> Result<Option<ServiceCapture>, String> {
        Ok(self.services.get(&id).cloned())
    }
    fn tasks(&mut self, folder: TaskFolder) -> Result<Vec<TaskCapture>, String> {
        Ok(self.tasks.get(&folder).cloned().unwrap_or_default())
    }
    fn policy(&mut self, id: PolicyIdentity) -> Result<Option<RegistryValue>, String> {
        Ok(self.policies.get(&id).cloned())
    }
    fn remove_installed(&mut self, package: &AppxPackageCapture) -> Result<(), String> {
        if self.fail_remove {
            return Err("injected remove failure".into());
        }
        self.installed
            .entry(package.identity)
            .or_default()
            .retain(|item| item != package);
        Ok(())
    }
    fn remove_provisioned(&mut self, package: &ProvisionedPackageCapture) -> Result<(), String> {
        self.provisioned
            .entry(package.identity)
            .or_default()
            .retain(|item| item != package);
        Ok(())
    }
    fn stop_and_disable(&mut self, id: ServiceIdentity) -> Result<(), String> {
        if let Some(item) = self.services.get_mut(&id) {
            item.start_type = ServiceStartType::Disabled;
            item.status = ServiceStatus::Stopped;
        }
        Ok(())
    }
    fn disable_task(&mut self, task: &TaskCapture) -> Result<(), String> {
        if let Some(item) = self
            .tasks
            .get_mut(&task.folder)
            .and_then(|items| items.iter_mut().find(|item| item.name == task.name))
        {
            item.enabled = false;
        }
        Ok(())
    }
    fn write_policy(&mut self, id: PolicyIdentity, value: RegistryValue) -> Result<(), String> {
        self.policies.insert(id, value);
        Ok(())
    }
}

#[test]
fn debloat_has_fixed_exact_oracle_identities() {
    assert_eq!(APPX_ALLOWLIST.len(), 25);
    assert_eq!(
        AppxIdentity::OutlookForWindows.name(),
        "Microsoft.OutlookForWindows"
    );
    assert_eq!(
        TaskFolder::ApplicationExperience.path(),
        r"\Microsoft\Windows\Application Experience\"
    );
    assert_eq!(
        PolicyIdentity::DisableAdvertisingId.desired(),
        RegistryValue::dword(0)
    );
}

#[test]
fn capture_then_apply_is_lossless_for_reversible_state_and_records_appx_identity() {
    let app = AppxPackageCapture {
        identity: AppxIdentity::BingNews,
        full_name: "Microsoft.BingNews_1.0_x64__8wekyb3d8bbwe".into(),
    };
    let provisioned = ProvisionedPackageCapture {
        identity: AppxIdentity::BingNews,
        package_name: "Microsoft.BingNews_1.0_neutral_~_8wekyb3d8bbwe".into(),
    };
    let task = TaskCapture {
        folder: TaskFolder::ApplicationExperience,
        name: "ProgramDataUpdater".into(),
        enabled: true,
    };
    let service = ServiceCapture {
        identity: ServiceIdentity::DiagTrack,
        start_type: ServiceStartType::Automatic,
        delayed_auto_start: false,
        status: ServiceStatus::Running,
    };
    let mut host = Host::default();
    host.installed.insert(app.identity, vec![app.clone()]);
    host.provisioned
        .insert(provisioned.identity, vec![provisioned.clone()]);
    host.tasks.insert(task.folder, vec![task]);
    host.services.insert(service.identity, service);
    let mut capability = DebloatCapability::new(host);
    let captured = capability.capture().expect("capture");
    assert_eq!(captured.installed, vec![app]);
    assert_eq!(captured.provisioned, vec![provisioned]);
    capability.apply_and_verify().expect("apply");
    assert!(matches!(capability.inspect(), Ok(Inspection::Satisfied)));
    assert!(
        capability
            .last_run()
            .expect("run")
            .outcomes
            .iter()
            .all(|item| !matches!(item.state, MutationState::Failed(_)))
    );
}

#[test]
fn adversarial_duplicate_or_changed_identity_fails_closed_and_retains_failure() {
    let app = AppxPackageCapture {
        identity: AppxIdentity::BingNews,
        full_name: "same".into(),
    };
    let mut host = Host::default();
    host.installed.insert(app.identity, vec![app.clone(), app]);
    assert!(DebloatCapability::new(host).capture().is_err());

    let mut host = Host::default();
    host.installed.insert(
        AppxIdentity::BingNews,
        vec![AppxPackageCapture {
            identity: AppxIdentity::BingNews,
            full_name: "first".into(),
        }],
    );
    let mut capability = DebloatCapability::new(host);
    capability.capture().expect("capture");
    capability.host_mut().installed.insert(
        AppxIdentity::BingNews,
        vec![AppxPackageCapture {
            identity: AppxIdentity::BingNews,
            full_name: "changed".into(),
        }],
    );
    assert!(capability.apply_and_verify().is_err());
    assert!(matches!(
        capability
            .last_run()
            .expect("partial run")
            .outcomes
            .last()
            .expect("outcome")
            .state,
        MutationState::Failed(_)
    ));
}

#[test]
fn p1_13_backup_and_manual_audit_separate_lossless_and_irreversible_state() {
    let snapshot = DebloatSnapshot {
        installed: vec![AppxPackageCapture {
            identity: AppxIdentity::BingNews,
            full_name: "Microsoft.BingNews_1.0_x64__8wekyb3d8bbwe".into(),
        }],
        provisioned: vec![ProvisionedPackageCapture {
            identity: AppxIdentity::BingNews,
            package_name: "Microsoft.BingNews_1.0_neutral_~_8wekyb3d8bbwe".into(),
        }],
        services: vec![ServiceCapture {
            identity: ServiceIdentity::DiagTrack,
            start_type: ServiceStartType::Automatic,
            delayed_auto_start: true,
            status: ServiceStatus::Running,
        }],
        tasks: vec![TaskCapture {
            folder: TaskFolder::ApplicationExperience,
            name: "ProgramDataUpdater".into(),
            enabled: true,
        }],
        policies: vec![PolicyCapture {
            identity: PolicyIdentity::DisableSoftLanding,
            original: Some(RegistryValue::dword(0)),
        }],
    };
    let entries = debloat_backup_entries(&snapshot, "P1:13").expect("lossless capture");
    assert!(matches!(
        entries[0],
        BackupEntry::Service {
            delayed_auto_start: true,
            ..
        }
    ));
    assert!(matches!(
        entries[1],
        BackupEntry::Scheduledtask {
            was_enabled: true,
            ..
        }
    ));
    assert!(matches!(
        entries[2],
        BackupEntry::Registry { existed: true, .. }
    ));
    let audit = frametime_core::MixedRecoveryAudit::pending_with_appx_subjects(
        "P1:13",
        "captured",
        debloat_appx_subjects(&snapshot),
    )
    .expect("manual AppX subjects");
    assert_eq!(audit.manual_recovery_subjects.len(), 2);
    assert!(
        frametime_core::IrreversibleAudit::Mixed(audit)
            .is_valid_pending_for(frametime_core::RecoveryRequirement::Mixed, "P1:13")
    );
}
