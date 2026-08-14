mod network_stack_tests {
    use std::{cell::RefCell, collections::BTreeMap};

    use frametime_core::{
        NATIVE_BINDING_SCHEMA_VERSION, NetworkAdapterBinding, NetworkStackNlaBackup,
        NetworkStackPolicy, NetworkStackPolicySnapshot, NetworkStackRawRegistryValue,
        NetworkStackSetting, NetworkStackValue, PciDeviceBinding,
    };

    use super::super::{
        NetworkStackHost, SETTINGS, apply_network_stack, capture_network_stack,
        restore_network_stack, verify_network_stack,
    };

    struct MockNetworkStackHost {
        adapters: RefCell<Vec<NetworkAdapterBinding>>,
        settings: RefCell<BTreeMap<NetworkStackSetting, NetworkStackValue>>,
        policies: RefCell<BTreeMap<NetworkStackPolicy, NetworkStackPolicySnapshot>>,
        writes: RefCell<Vec<NetworkStackSetting>>,
        foreign_policy: RefCell<bool>,
        nla: RefCell<NetworkStackNlaBackup>,
    }

    impl Default for MockNetworkStackHost {
        fn default() -> Self {
            Self {
                adapters: RefCell::default(),
                settings: RefCell::default(),
                policies: RefCell::default(),
                writes: RefCell::default(),
                foreign_policy: RefCell::default(),
                nla: RefCell::new(NetworkStackNlaBackup {
                    key_existed: false,
                    value_existed: false,
                    original_value: None,
                }),
            }
        }
    }

    impl NetworkStackHost for MockNetworkStackHost {
        fn discover_active_wired(&self) -> Result<Vec<NetworkAdapterBinding>, String> {
            Ok(self.adapters.borrow().clone())
        }
        fn link_speed_bps(&self, _: &NetworkAdapterBinding) -> Result<u64, String> {
            Ok(1_000_000_000)
        }
        fn read_setting(
            &self,
            _: &NetworkAdapterBinding,
            setting: NetworkStackSetting,
        ) -> Result<Option<NetworkStackValue>, String> {
            Ok(self.settings.borrow().get(&setting).cloned())
        }
        fn write_setting(
            &self,
            _: &NetworkAdapterBinding,
            setting: NetworkStackSetting,
            value: &NetworkStackValue,
        ) -> Result<(), String> {
            self.writes.borrow_mut().push(setting);
            self.settings.borrow_mut().insert(setting, value.clone());
            Ok(())
        }
        fn read_nla(&self) -> Result<NetworkStackNlaBackup, String> {
            Ok(self.nla.borrow().clone())
        }
        fn write_nla_target(&self) -> Result<(), String> {
            self.writes
                .borrow_mut()
                .push(NetworkStackSetting::QosNlaBypass);
            *self.nla.borrow_mut() = NetworkStackNlaBackup {
                key_existed: true,
                value_existed: true,
                original_value: Some(nla_target()),
            };
            Ok(())
        }
        fn nla_is_fixed_target(&self) -> Result<bool, String> {
            Ok(self.nla.borrow().original_value.as_ref() == Some(&nla_target()))
        }
        fn restore_nla(&self, captured: &NetworkStackNlaBackup) -> Result<(), String> {
            let current = self.nla.borrow().clone();
            if !captured.value_existed
                && current.original_value.is_some()
                && !self.nla_is_fixed_target()?
            {
                return Err("NLA changed outside suite lifecycle".into());
            }
            *self.nla.borrow_mut() = captured.clone();
            Ok(())
        }
        fn read_policy(
            &self,
            policy: NetworkStackPolicy,
        ) -> Result<Option<NetworkStackPolicySnapshot>, String> {
            Ok(self.policies.borrow().get(&policy).cloned())
        }
        fn policy_is_repository_owned(
            &self,
            _: NetworkStackPolicy,
            _: &NetworkStackPolicySnapshot,
        ) -> Result<bool, String> {
            Ok(!*self.foreign_policy.borrow())
        }
        fn write_policy(&self, policy: NetworkStackPolicy) -> Result<(), String> {
            self.policies.borrow_mut().insert(policy, policy_snapshot());
            Ok(())
        }
        fn delete_policy(&self, policy: NetworkStackPolicy) -> Result<(), String> {
            self.policies.borrow_mut().remove(&policy);
            Ok(())
        }
        fn restore_policy(
            &self,
            policy: NetworkStackPolicy,
            snapshot: &NetworkStackPolicySnapshot,
        ) -> Result<(), String> {
            self.policies.borrow_mut().insert(policy, snapshot.clone());
            Ok(())
        }
    }

    fn test_adapter() -> NetworkAdapterBinding {
        NetworkAdapterBinding {
            schema_version: NATIVE_BINDING_SCHEMA_VERSION,
            adapter_name: "{11111111-1111-1111-1111-111111111111}".into(),
            interface_guid: "{11111111-1111-1111-1111-111111111111}".into(),
            interface_luid: 1,
            interface_index: 1,
            friendly_name: "Ethernet".into(),
            interface_description: "Test Ethernet".into(),
            physical_address: vec![1, 2, 3, 4, 5, 6],
            device: PciDeviceBinding {
                schema_version: NATIVE_BINDING_SCHEMA_VERSION,
                instance_id: "PCI\\VEN_1234&DEV_5678&SUBSYS_56781234&REV_01\\1".into(),
                container_id: "{22222222-2222-2222-2222-222222222222}".into(),
                class_guid: "{4d36e972-e325-11ce-bfc1-08002be10318}".into(),
                vendor_id: 0x1234,
                device_id: 0x5678,
                subsystem_vendor_id: 0x1234,
                subsystem_device_id: 0x5678,
                revision_id: 1,
                driver_provider: "Test".into(),
                driver_version: "1.0".into(),
                published_inf: "oem1.inf".into(),
                observed_at_utc: "2026-08-13T00:00:00Z".into(),
                unknown: BTreeMap::new(),
            },
            observed_at_utc: "2026-08-13T00:00:00Z".into(),
            unknown: BTreeMap::new(),
        }
    }

    fn seeded_network_stack_host() -> MockNetworkStackHost {
        let host = MockNetworkStackHost::default();
        *host.adapters.borrow_mut() = vec![test_adapter()];
        for setting in SETTINGS {
            let value = match setting {
                NetworkStackSetting::Eee | NetworkStackSetting::FlowControl => {
                    NetworkStackValue::Dword(9)
                }
                NetworkStackSetting::RssEnabled | NetworkStackSetting::UroEnabled => {
                    NetworkStackValue::Dword(9)
                }
                NetworkStackSetting::QosNlaBypass => continue,
                _ => NetworkStackValue::Dword(9),
            };
            host.settings.borrow_mut().insert(setting, value);
        }
        *host.nla.borrow_mut() = NetworkStackNlaBackup {
            key_existed: true,
            value_existed: true,
            original_value: Some(NetworkStackRawRegistryValue {
                value_type: 1,
                bytes: vec![b'0', 0, 0, 0],
            }),
        };
        host
    }

    fn nla_target() -> NetworkStackRawRegistryValue {
        NetworkStackRawRegistryValue {
            value_type: 1,
            bytes: vec![b'1', 0, 0, 0],
        }
    }

    fn policy_snapshot() -> NetworkStackPolicySnapshot {
        NetworkStackPolicySnapshot {
            network_profile: 7,
            precedence: 127,
            template_match_condition: 0,
            user_match_condition: String::new(),
            ip_protocol: 2,
            ip_port_match_condition: 0,
            source_prefix_match_condition: String::new(),
            source_port_start: 0,
            source_port_end: 0,
            destination_prefix_match_condition: String::new(),
            destination_port_start: 27015,
            destination_port_end: 27036,
            app_path_match_condition: String::new(),
            uri_match_condition: String::new(),
            uri_recursive_match_condition: false,
            net_direct_port_match_condition: 0,
            priority_value_8021_action: -1,
            dscp_action: 46,
            min_bandwidth_weight_action: 0,
            throttle_rate_action: 0,
        }
    }

    #[test]
    fn transaction_captures_before_writes_reobserves_and_restores() {
        let host = seeded_network_stack_host();
        host.policies
            .borrow_mut()
            .insert(NetworkStackPolicy::Cs2App, policy_snapshot());
        let (binding, entry) = capture_network_stack(&host, "P1:16".into()).unwrap();
        apply_network_stack(&host, &binding).unwrap();
        verify_network_stack(&host, &binding).unwrap();
        assert_eq!(host.writes.borrow().len(), SETTINGS.len());
        restore_network_stack(&host, &entry).unwrap();
        assert_eq!(
            host.settings.borrow()[&NetworkStackSetting::RssEnabled],
            NetworkStackValue::Dword(9)
        );
        assert_eq!(
            host.policies.borrow()[&NetworkStackPolicy::Cs2App],
            policy_snapshot()
        );
        assert!(
            !host
                .policies
                .borrow()
                .contains_key(&NetworkStackPolicy::Cs2UdpPorts)
        );
    }

    #[test]
    fn changed_pnp_identity_refuses_mutation() {
        let host = seeded_network_stack_host();
        let (binding, _) = capture_network_stack(&host, "P1:16".into()).unwrap();
        host.adapters.borrow_mut()[0].device.instance_id =
            "PCI\\VEN_1234&DEV_5678&SUBSYS_56781234&REV_01\\other".into();
        assert!(apply_network_stack(&host, &binding).is_err());
        assert!(host.writes.borrow().is_empty());
    }

    #[test]
    fn ambiguous_adapter_refuses_capture() {
        let host = seeded_network_stack_host();
        host.adapters.borrow_mut().push(test_adapter());
        assert!(capture_network_stack(&host, "P1:16".into()).is_err());
    }

    #[test]
    fn unknown_native_value_type_refuses_capture() {
        let host = seeded_network_stack_host();
        host.settings.borrow_mut().insert(
            NetworkStackSetting::RssEnabled,
            NetworkStackValue::Binary(vec![1]),
        );
        assert!(capture_network_stack(&host, "P1:16".into()).is_err());
    }

    #[test]
    fn missing_adapter_refuses_capture() {
        let host = seeded_network_stack_host();
        host.adapters.borrow_mut().clear();
        assert!(capture_network_stack(&host, "P1:16".into()).is_err());
    }

    #[test]
    fn absent_driver_key_is_inapplicable_and_not_synthesized_on_restore() {
        let host = seeded_network_stack_host();
        host.settings
            .borrow_mut()
            .remove(&NetworkStackSetting::UroEnabled);
        let (binding, entry) = capture_network_stack(&host, "P1:16".into()).unwrap();

        apply_network_stack(&host, &binding).unwrap();
        assert!(
            !host
                .writes
                .borrow()
                .contains(&NetworkStackSetting::UroEnabled)
        );
        restore_network_stack(&host, &entry).unwrap();
        assert!(
            !host
                .settings
                .borrow()
                .contains_key(&NetworkStackSetting::UroEnabled)
        );
    }

    #[test]
    fn stale_adapter_before_restore_refuses_write() {
        let host = seeded_network_stack_host();
        let (_, entry) = capture_network_stack(&host, "P1:16".into()).unwrap();
        host.adapters.borrow_mut()[0].interface_luid = 2;
        assert!(restore_network_stack(&host, &entry).is_err());
        assert!(host.writes.borrow().is_empty());
    }

    #[test]
    fn foreign_same_name_policy_refuses_capture_before_mutation() {
        let host = seeded_network_stack_host();
        host.policies
            .borrow_mut()
            .insert(NetworkStackPolicy::Cs2App, policy_snapshot());
        *host.foreign_policy.borrow_mut() = true;
        assert!(capture_network_stack(&host, "P1:16".into()).is_err());
        assert!(host.writes.borrow().is_empty());
    }

    #[test]
    fn foreign_policy_defaults_refuse_capture() {
        let host = seeded_network_stack_host();
        let mut foreign = policy_snapshot();
        foreign.template_match_condition = 1;
        host.policies
            .borrow_mut()
            .insert(NetworkStackPolicy::Cs2UdpPorts, foreign);
        *host.foreign_policy.borrow_mut() = true;
        assert!(capture_network_stack(&host, "P1:16".into()).is_err());
    }

    #[test]
    fn raw_nla_value_round_trips_and_missing_key_lifecycle_is_exact() {
        let host = seeded_network_stack_host();
        *host.nla.borrow_mut() = NetworkStackNlaBackup {
            key_existed: false,
            value_existed: false,
            original_value: None,
        };
        let (binding, entry) = capture_network_stack(&host, "P1:16".into()).unwrap();
        apply_network_stack(&host, &binding).unwrap();
        verify_network_stack(&host, &binding).unwrap();
        restore_network_stack(&host, &entry).unwrap();
        assert_eq!(
            *host.nla.borrow(),
            NetworkStackNlaBackup {
                key_existed: false,
                value_existed: false,
                original_value: None,
            }
        );

        *host.nla.borrow_mut() = NetworkStackNlaBackup {
            key_existed: true,
            value_existed: true,
            original_value: Some(NetworkStackRawRegistryValue {
                value_type: 3,
                bytes: vec![1, 2, 3, 4],
            }),
        };
        let (binding, entry) = capture_network_stack(&host, "P1:16".into()).unwrap();
        apply_network_stack(&host, &binding).unwrap();
        restore_network_stack(&host, &entry).unwrap();
        assert_eq!(
            host.nla
                .borrow()
                .original_value
                .as_ref()
                .unwrap()
                .value_type,
            3
        );
        assert_eq!(
            host.nla.borrow().original_value.as_ref().unwrap().bytes,
            vec![1, 2, 3, 4]
        );
    }

    #[test]
    fn qualifier_decoder_rejects_schema_drift() {
        let good = ["1", "2", "4"].map(String::from);
        let labels = ["DOMAIN", "Public", "private"].map(String::from);
        assert_eq!(
            super::super::decode_network_profile_qualifiers(&good, &labels),
            Ok(7)
        );
        assert!(super::super::decode_network_profile_qualifiers(&good[..2], &labels).is_err());
        let duplicate = ["Domain", "domain", "Private"].map(String::from);
        assert!(super::super::decode_network_profile_qualifiers(&good, &duplicate).is_err());
    }
}
