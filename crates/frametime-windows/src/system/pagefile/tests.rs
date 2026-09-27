use super::*;
use crate::*;
#[cfg(test)]
mod pagefile_tests {
    use super::*;

    #[derive(Clone)]
    struct MockStore {
        inventory: PagefileInventory,
        fail: Option<&'static str>,
    }
    impl PagefileStore for MockStore {
        fn inventory(&mut self) -> Result<PagefileInventory, String> {
            Ok(self.inventory.clone())
        }
        fn set_automatic(
            &mut self,
            _: &str,
            _: &str,
            expected: Option<bool>,
            value: bool,
        ) -> Result<(), String> {
            if self.fail == Some("automatic")
                || expected.is_some_and(|expected| self.inventory.automatic_managed != expected)
            {
                return Err("fault".into());
            }
            self.inventory.automatic_managed = value;
            Ok(())
        }
        fn update(
            &mut self,
            setting: &PagefileSetting,
            initial: u32,
            maximum: u32,
        ) -> Result<(), String> {
            if self.fail == Some("update") {
                return Err("fault".into());
            }
            let item = self
                .inventory
                .settings
                .iter_mut()
                .find(|candidate| candidate.object_path == setting.object_path)
                .ok_or("missing")?;
            item.initial_size = initial;
            item.maximum_size = maximum;
            Ok(())
        }
        fn create(
            &mut self,
            path: &str,
            initial: u32,
            maximum: u32,
        ) -> Result<CreatedPagefileToken, String> {
            if self.fail == Some("create") {
                return Err("fault".into());
            }
            let token = CreatedPagefileToken {
                object_path: "Win32_PageFileSetting.Name=\"C:\\\\pagefile.sys\"".into(),
                relative_path: "Win32_PageFileSetting.Name=\"C:\\\\pagefile.sys\"".into(),
                path: path.into(),
                initial_size: initial,
                maximum_size: maximum,
            };
            self.inventory.settings.push(PagefileSetting {
                path: path.into(),
                initial_size: initial,
                maximum_size: maximum,
                object_path: token.object_path.clone(),
                relative_path: token.relative_path.clone(),
            });
            Ok(token)
        }
        fn delete(&mut self, created: &CreatedPagefileToken) -> Result<(), String> {
            self.inventory
                .settings
                .retain(|item| item.object_path != created.object_path);
            Ok(())
        }
    }
    pub(crate) fn inventory() -> PagefileInventory {
        PagefileInventory {
            automatic_managed: true,
            computer_object_path: "Win32_ComputerSystem.Name=\"HOST\"".into(),
            computer_relative_path: "Win32_ComputerSystem.Name=\"HOST\"".into(),
            system_drive: "C:".into(),
            physical_ram_mb: 16 * GIB_MB,
            free_space_mb: 100_000,
            settings: vec![PagefileSetting {
                path: "D:\\foreign.sys".into(),
                initial_size: 99,
                maximum_size: 100,
                object_path: "foreign-object".into(),
                relative_path: "foreign-relative".into(),
            }],
        }
    }
    pub(crate) fn explicit_pagefile_state() -> State {
        State {
            pagefile_mb: 4096,
            ..State::default()
        }
    }
    #[test]
    pub(crate) fn creates_only_after_full_capture_and_preserves_foreign_setting() {
        let binding =
            capture_pagefile_binding("P1:8".into(), &explicit_pagefile_state(), inventory())
                .expect("binding");
        let mut store = MockStore {
            inventory: binding.before.clone(),
            fail: None,
        };
        let created = begin_pagefile_mutation(&mut store, &binding).expect("mutate");
        verify_pagefile_mutation(&mut store, &binding, created.as_ref()).expect("verify");
        assert_eq!(store.inventory.settings[0].initial_size, 99);
        assert!(created.is_some());
    }
    #[test]
    pub(crate) fn compensation_deletes_only_opaque_created_identity_and_automatic_last() {
        let binding =
            capture_pagefile_binding("P1:8".into(), &explicit_pagefile_state(), inventory())
                .expect("binding");
        let mut store = MockStore {
            inventory: binding.before.clone(),
            fail: None,
        };
        let created = begin_pagefile_mutation(&mut store, &binding).expect("mutate");
        compensate_pagefile_mutation(&mut store, &binding, created.as_ref()).expect("compensate");
        assert!(store.inventory.automatic_managed);
        assert_eq!(store.inventory.settings.len(), 1);
    }
    #[test]
    pub(crate) fn restore_uses_only_created_token_and_preserves_foreign_setting() {
        let binding =
            capture_pagefile_binding("P1:8".into(), &explicit_pagefile_state(), inventory())
                .expect("binding");
        let mut store = MockStore {
            inventory: binding.before.clone(),
            fail: None,
        };
        let created = begin_pagefile_mutation(&mut store, &binding)
            .expect("mutate")
            .expect("created");
        let mut entry = pagefile_backup_entry(&binding);
        if let BackupEntry::PagefileTransaction {
            created_object_path,
            created_relative_path,
            created_initial_size,
            created_maximum_size,
            mutation_intent,
            ..
        } = &mut entry
        {
            *created_object_path = Some(created.object_path);
            *created_relative_path = Some(created.relative_path);
            *created_initial_size = Some(u64::from(created.initial_size));
            *created_maximum_size = Some(u64::from(created.maximum_size));
            *mutation_intent = Some("created".into());
        }
        restore_pagefile_transaction(&mut store, &entry).expect("restore");
        assert_eq!(store.inventory.settings.len(), 1);
        assert_eq!(store.inventory.settings[0].path, "D:\\foreign.sys");
    }
    #[test]
    pub(crate) fn incomplete_create_journal_fails_closed() {
        let binding =
            capture_pagefile_binding("P1:8".into(), &explicit_pagefile_state(), inventory())
                .expect("binding");
        let entry = pagefile_backup_entry(&binding);
        let mut store = MockStore {
            inventory: binding.before,
            fail: None,
        };
        assert!(restore_pagefile_transaction(&mut store, &entry).is_err());
    }
    #[test]
    pub(crate) fn rejects_duplicate_or_hostile_cim_identities() {
        let mut state = inventory();
        state.settings.push(state.settings[0].clone());
        assert!(
            capture_pagefile_binding("P1:8".into(), &explicit_pagefile_state(), state).is_err()
        );
        assert!(pagefile_target("\\\\evil").is_err());
    }
    #[test]
    pub(crate) fn fixed_pagefile_requires_explicit_override() {
        assert!(pagefile_size_mb(&State::default(), 3 * GIB_MB).is_err());
        assert_eq!(
            pagefile_size_mb(&explicit_pagefile_state(), 0).expect("size"),
            4096
        );
    }
}
