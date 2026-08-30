# Hardware support

CI and local mocks are the current acceptance environment. No support row below
has physical Windows evidence, so every hardware-specific row remains
`hardware-unverified`.

| Area | Intended backend | Current behavior | Verification |
| --- | --- | --- | --- |
| Ryzen identity and topology | CPUID, `GetLogicalProcessorInformationEx`, and `GetActiveProcessorCount` | Implemented for Windows x64 without topology fallbacks | hardware-unverified |
| CPU utilization and memory use | `GetSystemTimes` and `GlobalMemoryStatusEx` | Implemented for Windows | hardware-unverified |
| Ryzen telemetry tables | Model-specific performance tables | No registered backend | hardware-unverified |
| Curve Optimizer writes | Bounded experimental KMDF protocol | Protocol validation only; no loadable driver | hardware-unverified |
| GPU inventory | DXGI | Implemented for Windows | hardware-unverified |
| Radeon telemetry and tuning | Installed AMD ADLX API | DLL presence reported; Rust-only telemetry ABI not registered | hardware-unverified |
| GeForce telemetry | Installed NVIDIA NVAPI | Doctor resolves and initializes the Release 590 ABI before reporting availability; read-only load and temperature are implemented | hardware-unverified |
| GeForce tuning | Installed NVIDIA NVAPI | No write backend registered | hardware-unverified |
| VRAM validation | Isolated D3D12 worker | Bounded upload, device-local copy, readback, timeout, and device-removal handling implemented | hardware-unverified |
| System memory validation | Bounded Rust workload and Windows Event Log API | Measured timing and error counts with an explicit WHEA correlation result | hardware-unverified |
| Power plans | Windows power APIs | Native GUID and friendly-name enumeration implemented | hardware-unverified |
| Task scheduling | Task Scheduler 2.0 COM | Read-only inspection of the exact `\Northclock` folder and its registered tasks | hardware-unverified |
| VBS and device security | `Win32_DeviceGuard` WMI | Read-only runtime-status observation with the raw value retained | hardware-unverified |
| Conflict observation | Tool Help, Service Control Manager, SetupAPI, and Configuration Manager | Bounded read-only matching of known overlapping control processes, services, and drivers; device findings require Windows PnP Code 12 | hardware-unverified |
| WHEA observation | Windows Event Log API | Bounded native query and XML parsing implemented | hardware-unverified |
| Frame capture | DxgKrnl ETW present events | Bounded native `Present_Start` interval capture implemented | hardware-unverified |
| Overlay | Transparent egui viewport with measured data | Standalone read-only overlay mode implemented | hardware-unverified |
| ROM inspection | Bounds-checked PCI ROM parser | Read-only parser implemented | hardware-unverified |

`northclock doctor` is the authority for the running build. It reports each
capability's exact backend and one of `available`, `unsupported`, `experimental`,
`permission_required`, or `unverified`. An unavailable measurement has no value
field and is not persisted.

`northclock system status` is descriptive. A reported potential conflict means
only that a known overlapping hardware-control component was observed; it does
not prove that the component is writing hardware or causing instability. A
device finding means Windows reports a Code 12 resource conflict, but does not
identify the competing device. A missing observation never means the machine is
safe for tuning.

[AMD ADLX](https://github.com/GPUOpen-LibrariesAndSDKs/ADLX),
[NVIDIA NVAPI](https://github.com/NVIDIA/nvapi), and
[PresentMon](https://github.com/GameTechDev/PresentMon) are the upstream
references for vendor and ETW behavior. Northclock does not bundle their
libraries or executables.
