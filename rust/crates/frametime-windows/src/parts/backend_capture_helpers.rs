impl LiveBackend {
    fn capture_cs2_registry_backup(
        &mut self,
        key: String,
        action: Cs2RegistryAction,
    ) -> Result<Vec<BackupEntry>, String> {
        let (binding, entries) = capture_cs2_registry(key.clone(), action)
            .inspect_err(|_| self.transaction_lock = None)?;
        self.captured_cs2_bindings.insert(key.clone(), binding);
        self.captured_steps.insert(key);
        Ok(entries)
    }

    fn capture_cs2_config_backup(&mut self, key: String) -> Result<Vec<BackupEntry>, String> {
        let (binding, entry) =
            capture_cs2_config().inspect_err(|_| self.transaction_lock = None)?;
        self.captured_cs2_config_bindings
            .insert(key.clone(), binding);
        self.captured_steps.insert(key);
        Ok(vec![entry])
    }

    fn capture_network_stack_backup(&mut self, key: String) -> Result<Vec<BackupEntry>, String> {
        let (binding, entry) = capture_network_stack(&NativeNetworkStackHost, key.clone())
            .inspect_err(|_| self.transaction_lock = None)?;
        self.captured_network_stack_bindings
            .insert(key.clone(), binding);
        self.captured_steps.insert(key);
        Ok(vec![entry])
    }

    fn capture_autostart_backup(&mut self, key: String) -> Result<Vec<BackupEntry>, String> {
        let (binding, entries) = capture_autostart(key.clone(), self.config.as_ref())
            .inspect_err(|_| self.transaction_lock = None)?;
        self.captured_autostart_bindings
            .insert(key.clone(), binding);
        self.captured_steps.insert(key);
        Ok(entries)
    }

    fn capture_power_plan_backup(&mut self, key: String) -> Result<Vec<BackupEntry>, String> {
        let (binding, entry) = capture_power_plan(key.clone(), self.state.profile)
            .inspect_err(|_| self.transaction_lock = None)?;
        self.captured_power_plan_bindings
            .insert(key.clone(), binding);
        self.captured_steps.insert(key);
        Ok(vec![entry])
    }

    fn capture_pagefile_backup(&mut self, key: String) -> Result<Vec<BackupEntry>, String> {
        let inventory =
            native_pagefile_inventory().inspect_err(|_| self.transaction_lock = None)?;
        if self.state.pagefile_mb == 0 && inventory.physical_ram_mb >= 32 * 1024 {
            self.transaction_lock = None;
            return Err("P1:8 is inapplicable with 32 GiB or more physical RAM and no explicit pagefile override".into());
        }
        let binding = capture_pagefile_binding(key.clone(), &self.state, inventory)
            .inspect_err(|_| self.transaction_lock = None)?;
        let entry = pagefile_backup_entry(&binding);
        self.captured_pagefile_bindings.insert(key.clone(), binding);
        self.captured_steps.insert(key);
        Ok(vec![entry])
    }

    fn capture_dns_backup(&mut self, key: String) -> Result<Vec<BackupEntry>, String> {
        let config = self
            .config
            .as_ref()
            .ok_or("P3:9 requires a validated frametime.toml DNS provider selection")?;
        let (bindings, entries) = capture_native_dns(key.clone(), config)
            .inspect_err(|_| self.transaction_lock = None)?;
        self.captured_dns_bindings.insert(key.clone(), bindings);
        self.captured_steps.insert(key);
        Ok(entries)
    }
}
