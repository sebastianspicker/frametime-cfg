// P1:13's typed, fail-closed Windows debloat capability accepts no caller-supplied
// names, paths, commands, or policy data; its host boundary makes it testable.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AppxIdentity {
    BingNews,
    BingWeather,
    GetHelp,
    Getstarted,
    MicrosoftOfficeHub,
    MicrosoftSolitaireCollection,
    People,
    Todos,
    WindowsFeedbackHub,
    YourPhone,
    WindowsPhoneLink,
    MicrosoftCorporationPhoneLink,
    WindowsMaps,
    ZuneMusic,
    ZuneVideo,
    Clipchamp,
    Cortana,
    MixedRealityPortal,
    SkypeApp,
    WindowsCommunicationsApps,
    OutlookForWindows,
    WindowsDevHome,
    Teams,
    BingSearch,
    PowerAutomateDesktop,
}

impl AppxIdentity {
    pub const fn name(self) -> &'static str {
        match self {
            Self::BingNews => "Microsoft.BingNews",
            Self::BingWeather => "Microsoft.BingWeather",
            Self::GetHelp => "Microsoft.GetHelp",
            Self::Getstarted => "Microsoft.Getstarted",
            Self::MicrosoftOfficeHub => "Microsoft.MicrosoftOfficeHub",
            Self::MicrosoftSolitaireCollection => "Microsoft.MicrosoftSolitaireCollection",
            Self::People => "Microsoft.People",
            Self::Todos => "Microsoft.Todos",
            Self::WindowsFeedbackHub => "Microsoft.WindowsFeedbackHub",
            Self::YourPhone => "Microsoft.YourPhone",
            Self::WindowsPhoneLink => "Microsoft.Windows.PhoneLink",
            Self::MicrosoftCorporationPhoneLink => "MicrosoftCorporationII.PhoneLink",
            Self::WindowsMaps => "Microsoft.WindowsMaps",
            Self::ZuneMusic => "Microsoft.ZuneMusic",
            Self::ZuneVideo => "Microsoft.ZuneVideo",
            Self::Clipchamp => "Clipchamp.Clipchamp",
            Self::Cortana => "Microsoft.549981C3F5F10",
            Self::MixedRealityPortal => "Microsoft.MixedReality.Portal",
            Self::SkypeApp => "Microsoft.SkypeApp",
            Self::WindowsCommunicationsApps => "Microsoft.WindowsCommunicationsApps",
            Self::OutlookForWindows => "Microsoft.OutlookForWindows",
            Self::WindowsDevHome => "Microsoft.Windows.DevHome",
            Self::Teams => "MSTeams",
            Self::BingSearch => "Microsoft.BingSearch",
            Self::PowerAutomateDesktop => "Microsoft.PowerAutomateDesktop",
        }
    }
}

pub const APPX_ALLOWLIST: [AppxIdentity; 25] = [
    AppxIdentity::BingNews,
    AppxIdentity::BingWeather,
    AppxIdentity::GetHelp,
    AppxIdentity::Getstarted,
    AppxIdentity::MicrosoftOfficeHub,
    AppxIdentity::MicrosoftSolitaireCollection,
    AppxIdentity::People,
    AppxIdentity::Todos,
    AppxIdentity::WindowsFeedbackHub,
    AppxIdentity::YourPhone,
    AppxIdentity::WindowsPhoneLink,
    AppxIdentity::MicrosoftCorporationPhoneLink,
    AppxIdentity::WindowsMaps,
    AppxIdentity::ZuneMusic,
    AppxIdentity::ZuneVideo,
    AppxIdentity::Clipchamp,
    AppxIdentity::Cortana,
    AppxIdentity::MixedRealityPortal,
    AppxIdentity::SkypeApp,
    AppxIdentity::WindowsCommunicationsApps,
    AppxIdentity::OutlookForWindows,
    AppxIdentity::WindowsDevHome,
    AppxIdentity::Teams,
    AppxIdentity::BingSearch,
    AppxIdentity::PowerAutomateDesktop,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ServiceIdentity {
    DiagTrack,
    Dmwappushservice,
}
impl ServiceIdentity {
    pub const fn name(self) -> &'static str {
        match self {
            Self::DiagTrack => "DiagTrack",
            Self::Dmwappushservice => "dmwappushservice",
        }
    }
    const fn optional(self) -> bool {
        matches!(self, Self::Dmwappushservice)
    }
}
const SERVICE_ALLOWLIST: [ServiceIdentity; 2] = [
    ServiceIdentity::DiagTrack,
    ServiceIdentity::Dmwappushservice,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskFolder {
    ApplicationExperience,
    CustomerExperienceImprovementProgram,
}
impl TaskFolder {
    pub const fn path(self) -> &'static str {
        match self {
            Self::ApplicationExperience => r"\Microsoft\Windows\Application Experience\",
            Self::CustomerExperienceImprovementProgram => {
                r"\Microsoft\Windows\Customer Experience Improvement Program\"
            }
        }
    }
}
const TASK_FOLDERS: [TaskFolder; 2] = [
    TaskFolder::ApplicationExperience,
    TaskFolder::CustomerExperienceImprovementProgram,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PolicyIdentity {
    DisableWindowsConsumerFeatures,
    DisableSoftLanding,
    DisableAdvertisingId,
}
impl PolicyIdentity {
    pub const fn key(self) -> &'static str {
        match self {
            Self::DisableWindowsConsumerFeatures | Self::DisableSoftLanding => {
                r"SOFTWARE\Policies\Microsoft\Windows\CloudContent"
            }
            Self::DisableAdvertisingId => {
                r"SOFTWARE\Microsoft\Windows\CurrentVersion\AdvertisingInfo"
            }
        }
    }
    pub const fn name(self) -> &'static str {
        match self {
            Self::DisableWindowsConsumerFeatures => "DisableWindowsConsumerFeatures",
            Self::DisableSoftLanding => "DisableSoftLanding",
            Self::DisableAdvertisingId => "Enabled",
        }
    }
    pub const fn current_user(self) -> bool {
        matches!(self, Self::DisableAdvertisingId)
    }
    fn desired(self) -> RegistryValue {
        RegistryValue::dword(if self.current_user() { 0 } else { 1 })
    }
}
const POLICY_ALLOWLIST: [PolicyIdentity; 3] = [
    PolicyIdentity::DisableWindowsConsumerFeatures,
    PolicyIdentity::DisableSoftLanding,
    PolicyIdentity::DisableAdvertisingId,
];

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct AppxPackageCapture {
    pub identity: AppxIdentity,
    pub full_name: String,
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProvisionedPackageCapture {
    pub identity: AppxIdentity,
    pub package_name: String,
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ServiceCapture {
    pub identity: ServiceIdentity,
    pub start_type: ServiceStartType,
    pub delayed_auto_start: bool,
    pub status: ServiceStatus,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ServiceStartType {
    Automatic,
    Manual,
    Disabled,
    Boot,
    System,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ServiceStatus {
    Running,
    Stopped,
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TaskCapture {
    pub folder: TaskFolder,
    pub name: String,
    pub enabled: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryValue {
    pub kind: u32,
    pub bytes: Vec<u8>,
}
impl RegistryValue {
    pub fn dword(value: u32) -> Self {
        Self {
            kind: 4,
            bytes: value.to_le_bytes().to_vec(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyCapture {
    pub identity: PolicyIdentity,
    pub original: Option<RegistryValue>,
}

/// Exact P1:13 pre-mutation state. `installed` and `provisioned` are the
/// lossless manual-recovery facts for the core `Mixed` audit target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebloatSnapshot {
    pub installed: Vec<AppxPackageCapture>,
    pub provisioned: Vec<ProvisionedPackageCapture>,
    pub services: Vec<ServiceCapture>,
    pub tasks: Vec<TaskCapture>,
    pub policies: Vec<PolicyCapture>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MutationTarget {
    Installed(AppxPackageCapture),
    Provisioned(ProvisionedPackageCapture),
    Service(ServiceIdentity),
    Task(TaskCapture),
    Policy(PolicyIdentity),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MutationState {
    AlreadySatisfied,
    Verified,
    Failed(String),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationOutcome {
    pub target: MutationTarget,
    pub state: MutationState,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebloatRun {
    pub captured: DebloatSnapshot,
    pub outcomes: Vec<MutationOutcome>,
}

pub struct DebloatCapability<H> {
    host: H,
    captured: Option<DebloatSnapshot>,
    last_run: Option<DebloatRun>,
}
impl<H> std::fmt::Debug for DebloatCapability<H> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DebloatCapability")
            .field("captured", &self.captured)
            .field("last_run", &self.last_run)
            .finish_non_exhaustive()
    }
}
impl<H: DebloatHost> DebloatCapability<H> {
    pub const fn new(host: H) -> Self {
        Self {
            host,
            captured: None,
            last_run: None,
        }
    }
    pub fn host_mut(&mut self) -> &mut H {
        &mut self.host
    }
    pub fn last_run(&self) -> Option<&DebloatRun> {
        self.last_run.as_ref()
    }
    pub fn inspect(&mut self) -> Result<Inspection, String> {
        let state = self.observe()?;
        Ok(if needs_apply(&state) {
            Inspection::NeedsApply
        } else {
            Inspection::Satisfied
        })
    }
    pub fn capture(&mut self) -> Result<DebloatSnapshot, String> {
        let snapshot = self.observe()?;
        self.captured = Some(snapshot.clone());
        Ok(snapshot)
    }
    pub fn apply_and_verify(&mut self) -> Result<(), String> {
        let captured = self
            .captured
            .clone()
            .ok_or("P1:13 mutation requires an exact captured inventory")?;
        let mut run = DebloatRun {
            captured,
            outcomes: Vec::new(),
        };
        let targets = run.captured.clone();
        for package in targets.installed {
            self.apply_installed(&package, &mut run)?;
        }
        for package in targets.provisioned {
            self.apply_provisioned(&package, &mut run)?;
        }
        for service in targets.services {
            self.apply_service(service.identity, &mut run)?;
        }
        for task in targets.tasks {
            self.apply_task(&task, &mut run)?;
        }
        for policy in targets.policies {
            self.apply_policy(policy.identity, &mut run)?;
        }
        self.last_run = Some(run);
        Ok(())
    }
    fn observe(&mut self) -> Result<DebloatSnapshot, String> {
        let mut snapshot = DebloatSnapshot {
            installed: Vec::new(),
            provisioned: Vec::new(),
            services: Vec::new(),
            tasks: Vec::new(),
            policies: Vec::new(),
        };
        for identity in APPX_ALLOWLIST {
            let installed = self.host.installed(identity)?;
            if installed.iter().any(|item| item.identity != identity) {
                return Err("installed AppX API returned a mismatched allowlisted identity".into());
            }
            let provisioned = self.host.provisioned(identity)?;
            if provisioned.iter().any(|item| item.identity != identity) {
                return Err(
                    "provisioned AppX API returned a mismatched allowlisted identity".into(),
                );
            }
            snapshot.installed.extend(installed);
            snapshot.provisioned.extend(provisioned);
        }
        for identity in SERVICE_ALLOWLIST {
            if let Some(state) = self.host.service(identity)? {
                if state.identity != identity {
                    return Err("service API returned a mismatched allowlisted identity".into());
                }
                snapshot.services.push(state);
            } else if !identity.optional() {
                continue;
            }
        }
        for folder in TASK_FOLDERS {
            let tasks = self.host.tasks(folder)?;
            if tasks.iter().any(|item| item.folder != folder) {
                return Err("Task Scheduler API returned a mismatched folder identity".into());
            }
            snapshot.tasks.extend(tasks);
        }
        for identity in POLICY_ALLOWLIST {
            snapshot.policies.push(PolicyCapture {
                identity,
                original: self.host.policy(identity)?,
            });
        }
        validate_snapshot(&snapshot)?;
        Ok(snapshot)
    }
    fn fail(
        &mut self,
        run: &mut DebloatRun,
        target: MutationTarget,
        error: String,
    ) -> Result<(), String> {
        run.outcomes.push(MutationOutcome {
            target,
            state: MutationState::Failed(error.clone()),
        });
        self.last_run = Some(run.clone());
        Err(error)
    }
    fn apply_installed(
        &mut self,
        package: &AppxPackageCapture,
        run: &mut DebloatRun,
    ) -> Result<(), String> {
        let target = MutationTarget::Installed(package.clone());
        let present = match self
            .host
            .installed(package.identity)
            .and_then(|items| preflight_installed(items, package))
        {
            Ok(value) => value,
            Err(error) => return self.fail(run, target, error),
        };
        match present {
            false => run.outcomes.push(MutationOutcome {
                target,
                state: MutationState::AlreadySatisfied,
            }),
            true => {
                if let Err(error) = self.host.remove_installed(package) {
                    return self.fail(run, target, error);
                }
                let remained = match self
                    .host
                    .installed(package.identity)
                    .and_then(|items| preflight_installed(items, package))
                {
                    Ok(value) => value,
                    Err(error) => return self.fail(run, target, error),
                };
                if remained {
                    return self.fail(
                        run,
                        target,
                        "installed AppX package remained after removal".into(),
                    );
                }
                run.outcomes.push(MutationOutcome {
                    target,
                    state: MutationState::Verified,
                });
            }
        }
        Ok(())
    }
    fn apply_provisioned(
        &mut self,
        package: &ProvisionedPackageCapture,
        run: &mut DebloatRun,
    ) -> Result<(), String> {
        let target = MutationTarget::Provisioned(package.clone());
        let present = match self
            .host
            .provisioned(package.identity)
            .and_then(|items| preflight_provisioned(items, package))
        {
            Ok(value) => value,
            Err(error) => return self.fail(run, target, error),
        };
        match present {
            false => run.outcomes.push(MutationOutcome {
                target,
                state: MutationState::AlreadySatisfied,
            }),
            true => {
                if let Err(error) = self.host.remove_provisioned(package) {
                    return self.fail(run, target, error);
                }
                let remained = match self
                    .host
                    .provisioned(package.identity)
                    .and_then(|items| preflight_provisioned(items, package))
                {
                    Ok(value) => value,
                    Err(error) => return self.fail(run, target, error),
                };
                if remained {
                    return self.fail(
                        run,
                        target,
                        "provisioned AppX package remained after removal".into(),
                    );
                }
                run.outcomes.push(MutationOutcome {
                    target,
                    state: MutationState::Verified,
                });
            }
        }
        Ok(())
    }
    fn apply_service(
        &mut self,
        identity: ServiceIdentity,
        run: &mut DebloatRun,
    ) -> Result<(), String> {
        let target = MutationTarget::Service(identity);
        match self.host.service(identity)? {
            None => run.outcomes.push(MutationOutcome {
                target,
                state: MutationState::AlreadySatisfied,
            }),
            Some(current)
                if current.start_type == ServiceStartType::Disabled
                    && current.status == ServiceStatus::Stopped =>
            {
                run.outcomes.push(MutationOutcome {
                    target,
                    state: MutationState::AlreadySatisfied,
                })
            }
            Some(_) => {
                if let Err(error) = self.host.stop_and_disable(identity) {
                    return self.fail(run, target, error);
                }
                match self.host.service(identity)? {
                    Some(current)
                        if current.start_type == ServiceStartType::Disabled
                            && current.status == ServiceStatus::Stopped =>
                    {
                        run.outcomes.push(MutationOutcome {
                            target,
                            state: MutationState::Verified,
                        })
                    }
                    _ => {
                        return self.fail(
                            run,
                            target,
                            "service readback was not disabled and stopped".into(),
                        );
                    }
                }
            }
        }
        Ok(())
    }
    fn apply_task(&mut self, task: &TaskCapture, run: &mut DebloatRun) -> Result<(), String> {
        let target = MutationTarget::Task(task.clone());
        let current = exact_task(self.host.tasks(task.folder)?, task)?;
        if !current {
            return self.fail(
                run,
                target,
                "scheduled task identity disappeared or changed before mutation".into(),
            );
        }
        if !task.enabled {
            run.outcomes.push(MutationOutcome {
                target,
                state: MutationState::AlreadySatisfied,
            });
            return Ok(());
        }
        if let Err(error) = self.host.disable_task(task) {
            return self.fail(run, target, error);
        }
        match self
            .host
            .tasks(task.folder)?
            .into_iter()
            .find(|item| item.name == task.name)
        {
            Some(readback) if !readback.enabled => run.outcomes.push(MutationOutcome {
                target,
                state: MutationState::Verified,
            }),
            _ => {
                return self.fail(
                    run,
                    target,
                    "scheduled-task readback was not disabled".into(),
                );
            }
        }
        Ok(())
    }
    fn apply_policy(
        &mut self,
        identity: PolicyIdentity,
        run: &mut DebloatRun,
    ) -> Result<(), String> {
        let target = MutationTarget::Policy(identity);
        let desired = identity.desired();
        if self.host.policy(identity)? == Some(desired.clone()) {
            run.outcomes.push(MutationOutcome {
                target,
                state: MutationState::AlreadySatisfied,
            });
            return Ok(());
        }
        if let Err(error) = self.host.write_policy(identity, desired.clone()) {
            return self.fail(run, target, error);
        }
        if self.host.policy(identity)? != Some(desired) {
            return self.fail(
                run,
                target,
                "registry policy readback did not match exact DWORD target".into(),
            );
        }
        run.outcomes.push(MutationOutcome {
            target,
            state: MutationState::Verified,
        });
        Ok(())
    }
}
