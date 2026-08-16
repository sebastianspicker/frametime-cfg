fn registry_batch(changes: Vec<RegistryChange>) -> Action {
    Action::RegistryBatch(changes)
}

fn registry_change(
    hive: Hive,
    key: &'static str,
    name: &'static str,
    value: RegValue,
) -> RegistryChange {
    RegistryChange {
        hive,
        key,
        name,
        value,
    }
}
