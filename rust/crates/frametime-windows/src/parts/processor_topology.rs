#[cfg(windows)]
#[derive(Debug, Default)]
pub struct WindowsProcessorTopologyProvider;

#[cfg(windows)]
impl ProcessorTopologyProvider for WindowsProcessorTopologyProvider {
    fn processor_topology(&self) -> Result<ProcessorTopology, DeviceBindingError> {
        use windows::Win32::System::Threading::{
            GetActiveProcessorCount, GetActiveProcessorGroupCount,
        };
        let group_count = unsafe { GetActiveProcessorGroupCount() };
        if group_count != 1 {
            return Err(DeviceBindingError::UnsupportedProcessorTopology);
        }
        let logical_processors = unsafe { GetActiveProcessorCount(0) };
        if !(1..=64).contains(&logical_processors) {
            return Err(DeviceBindingError::UnsupportedProcessorTopology);
        }
        Ok(ProcessorTopology {
            groups: vec![ProcessorGroup {
                group_number: 0,
                active_logical_processors: logical_processors as u8,
            }],
        })
    }
}
