use crate::*;
impl LiveBackend<'_> {
    pub(crate) fn clear_transaction_state(&mut self) {
        self.transaction_lock = None;
        self.captured_steps.clear();
        self.captured_msi_batches.clear();
        self.captured_nic_affinity_bindings.clear();
        self.created_pagefile_tokens.clear();
        self.captured_cs2_bindings.clear();
        self.captured_cs2_config_bindings.clear();
        self.captured_drs_backups.clear();
        self.captured_network_stack_bindings.clear();
    }

    pub(crate) fn abandon_transaction(&mut self, key: &str) {
        self.transaction_lock = None;
        self.captured_steps.remove(key);
        self.captured_msi_batches.remove(key);
        self.captured_nic_affinity_bindings.remove(key);
        self.created_pagefile_tokens.remove(key);
        self.captured_cs2_bindings.remove(key);
        self.captured_cs2_config_bindings.remove(key);
        self.captured_drs_backups.remove(key);
        self.captured_network_stack_bindings.remove(key);
    }
}
