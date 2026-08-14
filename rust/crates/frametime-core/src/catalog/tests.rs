use super::*;

#[test]
fn catalog_has_exact_phase_counts_and_unique_keys() {
    assert_eq!(STEPS.iter().filter(|s| s.phase == Phase::One).count(), 38);
    assert_eq!(STEPS.iter().filter(|s| s.phase == Phase::Two).count(), 3);
    assert_eq!(STEPS.iter().filter(|s| s.phase == Phase::Three).count(), 13);
    let mut keys = STEPS
        .iter()
        .map(|s| (s.phase as u8, s.number))
        .collect::<Vec<_>>();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), 54);
}
