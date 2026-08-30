use crate::*;
pub(crate) fn registry_batch(changes: Vec<RegistryChange>) -> Action {
    Action::RegistryBatch(changes)
}

pub(crate) fn registry_change(
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
