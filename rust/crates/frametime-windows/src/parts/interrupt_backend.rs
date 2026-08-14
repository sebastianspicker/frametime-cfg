impl LiveBackend {
    fn inspect_interrupt_action(
        &mut self,
        action: &Action,
        operation: Operation,
    ) -> Result<Option<Inspection>, String> {
        let inspection = match action {
            Action::MsiPreparation => {
                self.observed_msi_preparation = Some(discover_native_msi_batches()?);
                Inspection::Satisfied
            }
            Action::NicAffinityPreparation => {
                self.observed_nic_affinity_preparation = Some(discover_native_nic_affinity()?);
                Inspection::Satisfied
            }
            Action::MsiInterrupts => {
                require_stored_preparation(&self._trusted_work_dir, "P1:21")?;
                let batches = discover_native_msi_batches()?;
                let satisfied = native_interrupt_batches_satisfied(&batches)?;
                self.captured_msi_batches
                    .insert(Self::key(operation), batches);
                if satisfied {
                    Inspection::Satisfied
                } else {
                    Inspection::NeedsApply
                }
            }
            Action::NicInterruptAffinity => {
                require_stored_preparation(&self._trusted_work_dir, "P1:22")?;
                let binding = discover_native_nic_affinity()?;
                let satisfied = native_interrupt_batches_satisfied(&[MsiDeviceBatch {
                    device: binding.device.clone(),
                    device_class: PciDeviceClass::Network,
                    changes: binding.changes.clone(),
                }])?;
                self.captured_nic_affinity_bindings
                    .insert(Self::key(operation), binding);
                if satisfied {
                    Inspection::Satisfied
                } else {
                    Inspection::NeedsApply
                }
            }
            _ => return Ok(None),
        };
        Ok(Some(inspection))
    }

    fn capture_interrupt_action(
        &mut self,
        action: &Action,
        key: &str,
    ) -> Result<Option<Vec<BackupEntry>>, String> {
        let entries = match action {
            Action::MsiInterrupts => {
                let batches = self
                    .captured_msi_batches
                    .get(key)
                    .ok_or("P3:2 backup requires inspected MSI device batches")?;
                capture_native_msi_backups(batches, &timestamp())
            }
            Action::NicInterruptAffinity => {
                let binding = self
                    .captured_nic_affinity_bindings
                    .get(key)
                    .ok_or("P3:3 backup requires an inspected NIC affinity binding")?;
                capture_native_nic_backups(binding, &timestamp())
            }
            _ => return Ok(None),
        }
        .inspect_err(|_| self.transaction_lock = None)?;
        self.captured_steps.insert(key.to_owned());
        Ok(Some(entries))
    }

    fn verify_interrupt_action(&self, action: &Action, key: &str) -> Option<Result<(), String>> {
        match action {
            Action::MsiInterrupts => Some(
                self.captured_msi_batches
                    .get(key)
                    .ok_or_else(|| "P3:2 verification requires captured MSI devices".to_owned())
                    .and_then(|batches| verify_native_msi_batches(batches)),
            ),
            Action::NicInterruptAffinity => Some(
                self.captured_nic_affinity_bindings
                    .get(key)
                    .ok_or_else(|| "P3:3 verification requires captured NIC binding".to_owned())
                    .and_then(verify_native_nic_affinity),
            ),
            _ => None,
        }
    }
}
