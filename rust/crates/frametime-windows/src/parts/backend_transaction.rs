impl LiveBackend {
    fn clear_transaction_state(&mut self) {
        self.transaction_lock = None;
        self.captured_steps.clear();
        self.captured_service_batches.clear();
        self.captured_nagle_bindings.clear();
        self.captured_dns_bindings.clear();
        self.captured_msi_batches.clear();
        self.captured_nic_affinity_bindings.clear();
        self.captured_autostart_bindings.clear();
        self.captured_power_plan_bindings.clear();
        self.created_pagefile_tokens.clear();
        self.captured_cs2_bindings.clear();
        self.captured_cs2_config_bindings.clear();
        self.captured_drs_backups.clear();
        self.captured_hags.clear();
        self.captured_network_stack_bindings.clear();
        self.shader_cache_inventory = None;
        #[cfg(windows)]
        {
            self.debloat = None;
        }
        self.debloat_appx_subjects = None;
    }

    fn abandon_transaction(&mut self, key: &str) {
        self.transaction_lock = None;
        self.captured_steps.remove(key);
        self.captured_service_batches.remove(key);
        self.captured_nagle_bindings.remove(key);
        self.captured_dns_bindings.remove(key);
        self.captured_msi_batches.remove(key);
        self.captured_nic_affinity_bindings.remove(key);
        self.captured_autostart_bindings.remove(key);
        self.captured_power_plan_bindings.remove(key);
        self.created_pagefile_tokens.remove(key);
        self.captured_cs2_bindings.remove(key);
        self.captured_cs2_config_bindings.remove(key);
        self.captured_drs_backups.remove(key);
        self.captured_hags.remove(key);
        self.captured_network_stack_bindings.remove(key);
        self.shader_cache_inventory = None;
        #[cfg(windows)]
        {
            self.debloat = None;
        }
        self.debloat_appx_subjects = None;
    }
}
