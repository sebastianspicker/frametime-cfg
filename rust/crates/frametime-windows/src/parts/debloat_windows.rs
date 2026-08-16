// Native Windows adapter for P1:13. Its public host is available only on
// Windows; it uses no shell or PowerShell execution path.

#[cfg(windows)]
mod native_debloat {
    use super::*;
    use windows::{
        ApplicationModel::Package,
        Management::Deployment::PackageManager,
        Win32::{
            Foundation::VARIANT_BOOL,
            System::{
                Com::{
                    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance,
                    CoInitializeEx,
                },
                TaskScheduler::{ITaskService, TASK_ENUM_HIDDEN, TaskScheduler},
                Variant::VARIANT,
            },
        },
        core::{BSTR, HSTRING, IUnknown},
    };

    pub struct Host {
        packages: PackageManager,
    }
    impl Host {
        pub fn new() -> Result<Self, String> {
            PackageManager::new()
                .map(|packages| Self { packages })
                .map_err(|error| format!("create AppX PackageManager: {error}"))
        }
        fn installed_packages(&self) -> Result<Vec<Package>, String> {
            Ok(self
                .packages
                .FindPackages()
                .map_err(|error| format!("enumerate AppX packages: {error}"))?
                .into_iter()
                .collect())
        }
        fn full_name(package: &Package, identity: AppxIdentity) -> Result<Option<String>, String> {
            let id = package
                .Id()
                .map_err(|error| format!("read AppX identity: {error}"))?;
            if id
                .Name()
                .map_err(|error| format!("read AppX package name: {error}"))?
                != identity.name()
            {
                return Ok(None);
            }
            id.FullName()
                .map(|value| Some(value.to_string()))
                .map_err(|error| format!("read AppX package full name: {error}"))
        }
        fn task_service() -> Result<ITaskService, String> {
            let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            if initialized.is_err() && initialized.0 != 1 {
                return Err(format!("initialize Task Scheduler COM: {initialized:?}"));
            }
            let service: ITaskService = unsafe {
                CoCreateInstance(&TaskScheduler, None::<&IUnknown>, CLSCTX_INPROC_SERVER)
            }
            .map_err(|error| format!("create Task Scheduler service: {error}"))?;
            let empty = VARIANT::default();
            unsafe { service.Connect(&empty, &empty, &empty, &empty) }
                .map_err(|error| format!("connect Task Scheduler: {error}"))?;
            Ok(service)
        }
        fn registry_change(identity: PolicyIdentity) -> RegistryChange {
            RegistryChange {
                hive: if identity.current_user() {
                    Hive::CurrentUser
                } else {
                    Hive::LocalMachine
                },
                key: identity.key(),
                name: identity.name(),
                value: RegValue::Dword(if identity.current_user() { 0 } else { 1 }),
            }
        }
    }
    impl DebloatHost for Host {
        fn installed(&mut self, identity: AppxIdentity) -> Result<Vec<AppxPackageCapture>, String> {
            self.installed_packages()?
                .iter()
                .filter_map(|item| Self::full_name(item, identity).transpose())
                .collect::<Result<Vec<_>, _>>()
                .map(|full_names| {
                    full_names
                        .into_iter()
                        .map(|full_name| AppxPackageCapture {
                            identity,
                            full_name,
                        })
                        .collect()
                })
        }
        fn provisioned(
            &mut self,
            identity: AppxIdentity,
        ) -> Result<Vec<ProvisionedPackageCapture>, String> {
            let packages = self
                .packages
                .FindProvisionedPackages()
                .map_err(|error| format!("enumerate provisioned AppX packages: {error}"))?;
            (0..packages
                .Size()
                .map_err(|error| format!("count provisioned AppX packages: {error}"))?)
                .filter_map(|index| {
                    packages
                        .GetAt(index)
                        .map_err(|error| format!("read provisioned AppX package: {error}"))
                        .and_then(|item| Self::full_name(&item, identity))
                        .transpose()
                })
                .collect::<Result<Vec<_>, _>>()
                .map(|package_names| {
                    package_names
                        .into_iter()
                        .map(|package_name| ProvisionedPackageCapture {
                            identity,
                            package_name,
                        })
                        .collect()
                })
        }
        fn service(&mut self, identity: ServiceIdentity) -> Result<Option<ServiceCapture>, String> {
            let Some(snapshot) = native_services::capture_present(&[identity.name().to_owned()])?
                .into_iter()
                .next()
            else {
                return Ok(None);
            };
            Ok(Some(ServiceCapture {
                identity,
                start_type: match snapshot.start_type.as_str() {
                    "Automatic" => ServiceStartType::Automatic,
                    "Manual" => ServiceStartType::Manual,
                    "Disabled" => ServiceStartType::Disabled,
                    "Boot" => ServiceStartType::Boot,
                    "System" => ServiceStartType::System,
                    _ => return Err("SCM returned unsupported service startup type".into()),
                },
                delayed_auto_start: snapshot.delayed_auto_start,
                status: match snapshot.status.as_str() {
                    "Running" => ServiceStatus::Running,
                    "Stopped" => ServiceStatus::Stopped,
                    _ => return Err("SCM returned unsupported service status".into()),
                },
            }))
        }
        fn tasks(&mut self, folder: TaskFolder) -> Result<Vec<TaskCapture>, String> {
            let service = Self::task_service()?;
            let folder_api = unsafe { service.GetFolder(&BSTR::from(folder.path())) }
                .map_err(|error| format!("open Task Scheduler folder: {error}"))?;
            let tasks = unsafe { folder_api.GetTasks(TASK_ENUM_HIDDEN.0) }
                .map_err(|error| format!("enumerate scheduled tasks: {error}"))?;
            let count = unsafe { tasks.Count() }
                .map_err(|error| format!("count scheduled tasks: {error}"))?;
            if !(0..=MAX_TASKS_PER_FOLDER as i32).contains(&count) {
                return Err("Task Scheduler returned invalid task count".into());
            }
            (1..=count)
                .map(|index| {
                    let task = unsafe { tasks.get_Item(&VARIANT::from(index)) }
                        .map_err(|error| format!("read scheduled task: {error}"))?;
                    Ok(TaskCapture {
                        folder,
                        name: unsafe { task.Name() }
                            .map_err(|error| format!("read scheduled task name: {error}"))?
                            .to_string(),
                        enabled: unsafe { task.Enabled() }
                            .map_err(|error| format!("read scheduled task enabled state: {error}"))?
                            .as_bool(),
                    })
                })
                .collect()
        }
        fn policy(&mut self, identity: PolicyIdentity) -> Result<Option<RegistryValue>, String> {
            match registry_read_exact(&Self::registry_change(identity))? {
                None => Ok(None),
                Some(RegValue::Dword(value)) => Ok(Some(RegistryValue::dword(value))),
                Some(_) => Err("P1:13 policy has non-DWORD registry value".into()),
            }
        }
        fn remove_installed(&mut self, package: &AppxPackageCapture) -> Result<(), String> {
            self.packages
                .RemovePackageAsync(&HSTRING::from(&package.full_name))
                .and_then(|operation| operation.join())
                .map(|_| ())
                .map_err(|error| format!("remove installed AppX package: {error}"))
        }
        fn remove_provisioned(
            &mut self,
            package: &ProvisionedPackageCapture,
        ) -> Result<(), String> {
            let packages = self.packages.FindProvisionedPackages().map_err(|error| {
                format!("enumerate provisioned AppX packages before removal: {error}")
            })?;
            for index in 0..packages
                .Size()
                .map_err(|error| format!("count provisioned AppX packages: {error}"))?
            {
                let item = packages
                    .GetAt(index)
                    .map_err(|error| format!("read provisioned AppX package: {error}"))?;
                let id = item
                    .Id()
                    .map_err(|error| format!("read provisioned AppX identity: {error}"))?;
                if id
                    .FullName()
                    .map_err(|error| format!("read provisioned AppX full name: {error}"))?
                    == package.package_name
                {
                    return self
                        .packages
                        .DeprovisionPackageForAllUsersAsync(
                            &id.FamilyName().map_err(|error| {
                                format!("read provisioned AppX family: {error}")
                            })?,
                        )
                        .and_then(|operation| operation.join())
                        .map(|_| ())
                        .map_err(|error| format!("deprovision AppX package: {error}"));
                }
            }
            Err("captured provisioned AppX package disappeared before removal".into())
        }
        fn stop_and_disable(&mut self, identity: ServiceIdentity) -> Result<(), String> {
            native_services::disable_stop_batch(&[identity.name().to_owned()])
        }
        fn disable_task(&mut self, task: &TaskCapture) -> Result<(), String> {
            let service = Self::task_service()?;
            let folder = unsafe { service.GetFolder(&BSTR::from(task.folder.path())) }
                .map_err(|error| format!("open Task Scheduler folder: {error}"))?;
            let registered = unsafe { folder.GetTask(&BSTR::from(&task.name)) }
                .map_err(|error| format!("open captured scheduled task: {error}"))?;
            unsafe { registered.SetEnabled(VARIANT_BOOL::from(false)) }
                .map_err(|error| format!("disable scheduled task: {error}"))
        }
        fn write_policy(
            &mut self,
            identity: PolicyIdentity,
            value: RegistryValue,
        ) -> Result<(), String> {
            if value != identity.desired() {
                return Err("P1:13 registry policy is not the exact compiled DWORD value".into());
            }
            registry_write(&Self::registry_change(identity))
        }
    }
}
#[cfg(windows)]
pub use native_debloat::Host as NativeDebloatHost;
