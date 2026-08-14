impl LiveBackend {
    fn capture_hags_backup(&mut self, key: String) -> Result<Vec<BackupEntry>, String> {
        let (binding, entry) = HagsRegistryCompatibility::capture()
            .inspect_err(|_| self.transaction_lock = None)?;
        self.captured_hags.insert(key.clone(), binding);
        self.captured_steps.insert(key);
        Ok(vec![entry])
    }

    fn apply_hags(&self, key: &str) -> Result<(), String> {
        self.captured_hags
            .get(key)
            .ok_or("P1:7 mutation requires a captured HAGS compatibility binding")?
            .apply()
    }

    fn verify_hags_immediate(&self, key: &str) -> Result<(), String> {
        self.captured_hags
            .get(key)
            .ok_or("P1:7 verification requires a captured HAGS compatibility binding")?
            .verify_immediate()
    }
}
