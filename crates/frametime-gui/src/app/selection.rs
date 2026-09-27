/// Converts a Win32 combo-box result into a closed-list value without giving the
/// GUI a path to invent an unlisted selection. HWND reads remain at call sites.
pub(super) fn selected_or_default<T: Copy>(index: isize, choices: &[T], fallback: T) -> T {
    usize::try_from(index)
        .ok()
        .and_then(|index| choices.get(index).copied())
        .unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use super::selected_or_default;

    #[test]
    fn invalid_combo_indices_use_the_closed_list_default() {
        assert_eq!(selected_or_default(1, &[10, 20], 10), 20);
        assert_eq!(selected_or_default(-1, &[10, 20], 10), 10);
        assert_eq!(selected_or_default(2, &[10, 20], 10), 10);
    }
}
