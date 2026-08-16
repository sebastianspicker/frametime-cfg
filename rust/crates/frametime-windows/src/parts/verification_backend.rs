impl LiveBackend {
    fn verify_observation_action(&self, action: &Action) -> Result<Option<()>, String> {
        match action {
            Action::ObserveConfigState => {
                verify_config_state(Some(self.config.value()), &self.state).map(Some)
            }
            Action::ObserveGpuInventory => {
                let observed = discover_hardware()?;
                verify_gpu_inventory(&self.hardware, &observed).map(Some)
            }
            Action::BaselineBenchmark => {
                if baseline_benchmark_is_persisted(&self.work_dir, &self._trusted_work_dir) {
                    Ok(Some(()))
                } else {
                    Err("P1:17 requires a coherent persisted baseline VProf capture".into())
                }
            }
            Action::FinalBenchmark => {
                Err("P3:13 is completed only by the standalone final-benchmark command".into())
            }
            Action::ObserveChipsetDriver => {
                let captured = self
                    .chipset_inventory
                    .as_ref()
                    .ok_or("P1:35 verification requires immutable inspected chipset records")?;
                verify_chipset_inventory(captured).map(Some)
            }
            Action::ObserveMemoryTopology => {
                let captured = self
                    .memory_topology
                    .as_ref()
                    .ok_or("P1:24 verification requires immutable inspected SMBIOS topology")?;
                verify_memory_topology(captured).map(Some)
            }
            Action::FpsCapInfo => verify_fps_cap_info(Some(self.config.value()), &self.state).map(Some),
            _ => Ok(None),
        }
    }
}
