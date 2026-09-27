use crate::*;

const SUITE_POWER_PLAN_NAME: &str = "frametime.cfg";

pub(super) fn restore_power_plan(
    step: &str,
    original_guid: &str,
    suite_owned_guids: &[String],
    unknown: &BTreeMap<String, Value>,
) -> Result<(), String> {
    let mut host = NativePowerPlanHost;
    restore_power_plan_with_host(&mut host, step, original_guid, suite_owned_guids, unknown)
}

trait PowerPlanHost {
    fn powercfg(&mut self, arguments: &[&str]) -> Result<String, String>;
}

struct NativePowerPlanHost;

impl PowerPlanHost for NativePowerPlanHost {
    #[cfg(windows)]
    fn powercfg(&mut self, arguments: &[&str]) -> Result<String, String> {
        CommandVector::new(CommandName::Powercfg, arguments)?.run()
    }

    #[cfg(not(windows))]
    fn powercfg(&mut self, _: &[&str]) -> Result<String, String> {
        Err("the live backend is supported only on Windows".into())
    }
}

fn restore_power_plan_with_host(
    host: &mut impl PowerPlanHost,
    step: &str,
    original_guid: &str,
    suite_owned_guids: &[String],
    unknown: &BTreeMap<String, Value>,
) -> Result<(), String> {
    validate_power_plan_restore_binding(step, original_guid, suite_owned_guids, unknown)?;
    host.powercfg(&["/setactive", original_guid])?;
    verify_original_power_plan(host, original_guid)?;
    for suite_guid in suite_owned_guids {
        restore_suite_power_plan(host, suite_guid)?;
    }
    Ok(())
}

fn validate_power_plan_restore_binding(
    step: &str,
    original_guid: &str,
    suite_owned_guids: &[String],
    unknown: &BTreeMap<String, Value>,
) -> Result<(), String> {
    if step != "P1:6" || !unknown.is_empty() || validate_power_plan_guid(original_guid).is_err() {
        return Err("power-plan restore binding is not exact".into());
    }
    if suite_owned_guids.is_empty()
        || suite_owned_guids.iter().any(|guid| {
            validate_power_plan_guid(guid).is_err() || guid.eq_ignore_ascii_case(original_guid)
        })
        || has_duplicate_power_plan_guids(suite_owned_guids)
    {
        return Err("power-plan recovery identities are not exact".into());
    }
    Ok(())
}

fn has_duplicate_power_plan_guids(guids: &[String]) -> bool {
    guids.iter().enumerate().any(|(index, guid)| {
        guids[..index]
            .iter()
            .any(|previous| previous.eq_ignore_ascii_case(guid))
    })
}

fn verify_original_power_plan(
    host: &mut impl PowerPlanHost,
    original_guid: &str,
) -> Result<(), String> {
    if parse_active_power_plan(&host.powercfg(&["/getactivescheme"])?)?.guid != original_guid {
        Err("original power-plan readback did not match".into())
    } else {
        Ok(())
    }
}

fn restore_suite_power_plan(host: &mut impl PowerPlanHost, suite_guid: &str) -> Result<(), String> {
    let known = host
        .powercfg(&["/list"])?
        .lines()
        .filter_map(parse_power_plan_line)
        .find(|plan| plan.guid == suite_guid);
    let Some(plan) = known else {
        return Ok(());
    };
    if plan.name != SUITE_POWER_PLAN_NAME {
        return Err("suite power-plan provenance is no longer exact".into());
    }
    host.powercfg(&["/delete", suite_guid])?;
    if host
        .powercfg(&["/list"])?
        .lines()
        .filter_map(find_power_guid)
        .any(|guid| guid == suite_guid)
    {
        Err("suite power-plan remains after deletion".into())
    } else {
        Ok(())
    }
}

pub(crate) fn validate_power_plan_guid(value: &str) -> Result<(), String> {
    if value.len() == 36
        && value.chars().enumerate().all(|(index, character)| {
            matches!(index, 8 | 13 | 18 | 23) && character == '-' || character.is_ascii_hexdigit()
        })
    {
        Ok(())
    } else {
        Err("power-plan GUID is not allowlisted".into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ActivePowerPlan {
    guid: String,
    name: String,
}

fn parse_power_plan_line(line: &str) -> Option<ActivePowerPlan> {
    let raw_guid = line
        .split(|character: char| !character.is_ascii_hexdigit() && character != '-')
        .find(|candidate| validate_power_plan_guid(candidate).is_ok())?;
    let guid = raw_guid.to_ascii_lowercase();
    let tail = line
        .get(line.find(raw_guid)? + raw_guid.len()..)?
        .trim()
        .trim_end_matches('*')
        .trim();
    let name = tail.strip_prefix('(')?.strip_suffix(')')?.trim();
    (!name.is_empty()).then(|| ActivePowerPlan {
        guid,
        name: name.into(),
    })
}

fn find_power_guid(text: &str) -> Option<String> {
    text.split(|character: char| !character.is_ascii_hexdigit() && character != '-')
        .find(|candidate| validate_power_plan_guid(candidate).is_ok())
        .map(|value| value.to_ascii_lowercase())
}

fn parse_active_power_plan(text: &str) -> Result<ActivePowerPlan, String> {
    let plans = text
        .lines()
        .filter_map(parse_power_plan_line)
        .collect::<Vec<_>>();
    match plans.as_slice() {
        [plan] => Ok(plan.clone()),
        [] => Err("powercfg active-plan output has no exact GUID/name pair".into()),
        _ => Err("powercfg active-plan output has ambiguous GUID/name pairs".into()),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;

    const ORIGINAL_GUID: &str = "11111111-1111-1111-1111-111111111111";
    const SUITE_GUID: &str = "22222222-2222-2222-2222-222222222222";

    struct FakePowerPlanHost {
        responses: VecDeque<(Vec<String>, Result<String, String>)>,
    }

    impl FakePowerPlanHost {
        fn new<'a>(
            responses: impl IntoIterator<Item = (Vec<&'a str>, Result<&'a str, &'a str>)>,
        ) -> Self {
            Self {
                responses: responses
                    .into_iter()
                    .map(|(arguments, result)| {
                        (
                            arguments.into_iter().map(str::to_owned).collect(),
                            result.map(str::to_owned).map_err(str::to_owned),
                        )
                    })
                    .collect(),
            }
        }

        fn assert_complete(&self) {
            assert!(self.responses.is_empty(), "unused powercfg responses");
        }
    }

    impl PowerPlanHost for FakePowerPlanHost {
        fn powercfg(&mut self, arguments: &[&str]) -> Result<String, String> {
            let (expected, response) = self.responses.pop_front().expect("unexpected powercfg");
            assert_eq!(
                expected,
                arguments
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            );
            response
        }
    }

    fn no_unknown_fields() -> BTreeMap<String, Value> {
        BTreeMap::new()
    }

    fn active_plan(guid: &str) -> String {
        format!("Power Scheme GUID: {guid}  (Balanced) *")
    }

    fn listed_plan(guid: &str, name: &str) -> String {
        format!("Power Scheme GUID: {guid}  ({name})")
    }

    fn restore(host: &mut FakePowerPlanHost, suite_guids: &[String]) -> Result<(), String> {
        restore_power_plan_with_host(
            host,
            "P1:6",
            ORIGINAL_GUID,
            suite_guids,
            &no_unknown_fields(),
        )
    }

    #[test]
    fn rejects_active_output_without_an_exact_guid_name_pair() {
        let mut host = FakePowerPlanHost::new([
            (vec!["/setactive", ORIGINAL_GUID], Ok("")),
            (
                vec!["/getactivescheme"],
                Ok("Power Scheme GUID: unavailable"),
            ),
        ]);

        assert_eq!(
            restore(&mut host, &[SUITE_GUID.into()]),
            Err("powercfg active-plan output has no exact GUID/name pair".into())
        );
        host.assert_complete();
    }

    #[test]
    fn rejects_ambiguous_active_output() {
        let mut host = FakePowerPlanHost::new([
            (vec!["/setactive", ORIGINAL_GUID], Ok("")),
            (
                vec!["/getactivescheme"],
                Ok(
                    "Power Scheme GUID: 11111111-1111-1111-1111-111111111111  (Balanced)\nPower Scheme GUID: 22222222-2222-2222-2222-222222222222  (High performance)",
                ),
            ),
        ]);

        assert_eq!(
            restore(&mut host, &[SUITE_GUID.into()]),
            Err("powercfg active-plan output has ambiguous GUID/name pairs".into())
        );
        host.assert_complete();
    }

    #[test]
    fn refuses_to_delete_a_non_suite_plan() {
        let mut host = FakePowerPlanHost::new([
            (vec!["/setactive", ORIGINAL_GUID], Ok("")),
            (
                vec!["/getactivescheme"],
                Ok(active_plan(ORIGINAL_GUID).as_str()),
            ),
            (
                vec!["/list"],
                Ok(listed_plan(SUITE_GUID, "OEM custom").as_str()),
            ),
        ]);

        assert_eq!(
            restore(&mut host, &[SUITE_GUID.into()]),
            Err("suite power-plan provenance is no longer exact".into())
        );
        host.assert_complete();
    }

    #[test]
    fn rejects_duplicate_suite_guids_before_running_commands() {
        let mut host =
            FakePowerPlanHost::new(std::iter::empty::<(Vec<&str>, Result<&str, &str>)>());

        assert_eq!(
            restore(
                &mut host,
                &[SUITE_GUID.into(), SUITE_GUID.to_ascii_uppercase()],
            ),
            Err("power-plan recovery identities are not exact".into())
        );
        host.assert_complete();
    }

    #[test]
    fn restores_the_original_plan_before_deleting_the_suite_plan() {
        let mut host = FakePowerPlanHost::new([
            (vec!["/setactive", ORIGINAL_GUID], Ok("")),
            (
                vec!["/getactivescheme"],
                Ok(active_plan(ORIGINAL_GUID).as_str()),
            ),
            (
                vec!["/list"],
                Ok(listed_plan(SUITE_GUID, SUITE_POWER_PLAN_NAME).as_str()),
            ),
            (vec!["/delete", SUITE_GUID], Ok("")),
            (
                vec!["/list"],
                Ok(listed_plan(ORIGINAL_GUID, "Balanced").as_str()),
            ),
        ]);

        assert_eq!(restore(&mut host, &[SUITE_GUID.into()]), Ok(()));
        host.assert_complete();
    }

    #[test]
    fn fails_when_the_suite_plan_remains_after_deletion() {
        let mut host = FakePowerPlanHost::new([
            (vec!["/setactive", ORIGINAL_GUID], Ok("")),
            (
                vec!["/getactivescheme"],
                Ok(active_plan(ORIGINAL_GUID).as_str()),
            ),
            (
                vec!["/list"],
                Ok(listed_plan(SUITE_GUID, SUITE_POWER_PLAN_NAME).as_str()),
            ),
            (vec!["/delete", SUITE_GUID], Ok("")),
            (
                vec!["/list"],
                Ok(listed_plan(SUITE_GUID, SUITE_POWER_PLAN_NAME).as_str()),
            ),
        ]);

        assert_eq!(
            restore(&mut host, &[SUITE_GUID.into()]),
            Err("suite power-plan remains after deletion".into())
        );
        host.assert_complete();
    }
}
