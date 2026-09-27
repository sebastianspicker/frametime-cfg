use super::*;
use crate::*;
impl LiveBackend<'_> {
    pub(crate) fn apply_pagefile_action(&mut self, key: &str) -> Result<(), String> {
        let binding = self
            .captured_pagefile_bindings
            .get(key)
            .ok_or("P1:8 mutation requires a captured CIM pagefile binding")?;
        match native_pagefile_begin(binding) {
            Ok(Some(created)) => {
                if let Err(error) =
                    persist_created_pagefile_token(&self._trusted_work_dir, &created)
                {
                    let rollback = native_pagefile_compensate(binding, Some(&created));
                    return Err(compound_pagefile_error(
                        "persist exact created pagefile token",
                        error,
                        rollback,
                    ));
                }
                self.created_pagefile_tokens.insert(key.into(), created);
                Ok(())
            }
            Ok(None) => Ok(()),
            Err(error) => Err(error),
        }
    }

    pub(crate) fn verify_pagefile_action(&mut self, key: &str) -> Result<(), String> {
        let binding = self
            .captured_pagefile_bindings
            .get(key)
            .ok_or("P1:8 verification requires a captured CIM pagefile binding")?;
        let created = self.created_pagefile_tokens.get(key);
        match native_pagefile_verify(binding, created) {
            Ok(()) => Ok(()),
            Err(error) => Err(compound_pagefile_error(
                "verify P1:8 postconditions",
                error,
                native_pagefile_compensate(binding, created),
            )),
        }
    }
}

#[cfg(any(test, windows))]
pub(crate) fn check_foreign_preservation(
    binding: &PagefileBinding,
    after: &PagefileInventory,
) -> Result<(), String> {
    validate_inventory(after)?;
    for original in &binding.before.settings {
        if binding
            .target
            .as_ref()
            .is_some_and(|target| target.object_path == original.object_path)
        {
            continue;
        }
        let found = after
            .settings
            .iter()
            .find(|setting| setting.object_path == original.object_path);
        if found != Some(original) {
            return Err("a non-target pagefile setting changed or disappeared".into());
        }
    }
    Ok(())
}

#[cfg(any(test, windows))]
pub(crate) fn begin_pagefile_mutation<S: PagefileStore>(
    store: &mut S,
    binding: &PagefileBinding,
) -> Result<Option<CreatedPagefileToken>, String> {
    store.set_automatic(
        &binding.before.computer_object_path,
        &binding.before.computer_relative_path,
        Some(binding.before.automatic_managed),
        false,
    )?;
    let disabled = store.inventory()?;
    if disabled.automatic_managed {
        return Err("AutomaticManagedPagefile did not read back false".into());
    }
    check_foreign_preservation(binding, &disabled)?;
    if let Some(target) = &binding.target {
        store.update(target, binding.initial_size, binding.maximum_size)?;
        Ok(None)
    } else {
        store
            .create(
                &binding.target_path,
                binding.initial_size,
                binding.maximum_size,
            )
            .map(Some)
    }
}

#[cfg(any(test, windows))]
pub(crate) fn verify_pagefile_mutation<S: PagefileStore>(
    store: &mut S,
    binding: &PagefileBinding,
    created: Option<&CreatedPagefileToken>,
) -> Result<(), String> {
    let after = store.inventory()?;
    if after.automatic_managed {
        return Err("AutomaticManagedPagefile changed during pagefile transaction".into());
    }
    check_foreign_preservation(binding, &after)?;
    let target = after
        .settings
        .iter()
        .find(|setting| setting.path.eq_ignore_ascii_case(&binding.target_path))
        .ok_or("target pagefile setting is absent after mutation")?;
    if target.initial_size != binding.initial_size || target.maximum_size != binding.maximum_size {
        return Err("target pagefile sizes did not read back exactly".into());
    }
    if let Some(created) = created
        && (target.object_path != created.object_path
            || target.relative_path != created.relative_path)
    {
        return Err("created pagefile identity did not read back exactly".into());
    }
    Ok(())
}

#[cfg(any(test, windows))]
pub(crate) fn compensate_pagefile_mutation<S: PagefileStore>(
    store: &mut S,
    binding: &PagefileBinding,
    created: Option<&CreatedPagefileToken>,
) -> Result<(), String> {
    let mut failures = Vec::new();
    if let Some(created) = created {
        if let Err(error) = store.delete(created) {
            failures.push(format!("delete exact created pagefile: {error}"));
        }
    } else if let Some(target) = &binding.target
        && let Err(error) = store.update(target, target.initial_size, target.maximum_size)
    {
        failures.push(format!("restore target pagefile: {error}"));
    }
    if let Err(error) = store.set_automatic(
        &binding.before.computer_object_path,
        &binding.before.computer_relative_path,
        None,
        binding.before.automatic_managed,
    ) {
        failures.push(format!("restore AutomaticManagedPagefile last: {error}"));
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}
