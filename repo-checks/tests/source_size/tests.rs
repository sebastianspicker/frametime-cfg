use super::*;

const CLONE: &str = r#"
    let alpha = input + 1;
    let beta = alpha * 2;
    let gamma = beta - 3;
    let delta = gamma / 4;
    let epsilon = delta % 5;
    let zeta = epsilon == 6;
    let eta = zeta || input > 7;
    consume(alpha, beta, gamma, delta, epsilon, zeta, eta);
"#;

fn fixture(path: &str, functions: &[(&str, &str)]) -> Vec<FunctionBody> {
    let source = functions
        .iter()
        .map(|(name, body)| format!("fn {name}(input: i32) {{ {body} }}"))
        .collect::<Vec<_>>()
        .join("\n");
    parse_production_functions(Path::new(path), &source)
}

#[test]
fn detects_cross_file_clones_despite_comments_and_formatting() {
    let mut functions = fixture("crates/a/src/lib.rs", &[("left", CLONE)]);
    let formatted = CLONE.replace("let beta", "// commentary\nlet     beta");
    functions.extend(fixture("crates/b/src/lib.rs", &[("right", &formatted)]));

    let clones = find_exact_clones(&functions, MIN_CLONE_TOKENS, MIN_CLONE_LINES);

    assert_eq!(clones.len(), 1);
    assert_eq!(clones[0].left.symbol, "left");
    assert_eq!(clones[0].right.symbol, "right");
}

#[test]
fn detects_intra_file_clones() {
    let functions = fixture("crates/a/src/lib.rs", &[("left", CLONE), ("right", CLONE)]);

    let clones = find_exact_clones(&functions, MIN_CLONE_TOKENS, MIN_CLONE_LINES);

    assert_eq!(clones.len(), 1);
    assert_eq!(clones[0].left.file, clones[0].right.file);
}

#[test]
fn ignores_test_only_paths_and_cfg_test_items() {
    let mut functions = fixture("crates/a/tests/copied.rs", &[("integration", CLONE)]);
    functions.extend(parse_production_functions(
        Path::new("crates/a/src/lib.rs"),
        &format!("#[cfg(test)] fn copied(input: i32) {{ {CLONE} }}"),
    ));
    functions.extend(fixture("crates/a/src/live.rs", &[("production", CLONE)]));

    assert!(find_exact_clones(&functions, MIN_CLONE_TOKENS, MIN_CLONE_LINES).is_empty());
}

#[test]
fn requires_both_token_and_physical_line_thresholds() {
    let enough_lines = fixture("crates/a/src/lib.rs", &[("left", CLONE), ("right", CLONE)]);
    assert!(find_exact_clones(&enough_lines, usize::MAX, MIN_CLONE_LINES).is_empty());

    let one_line = CLONE.lines().collect::<Vec<_>>().join(" ");
    let too_few_lines = fixture(
        "crates/a/src/lib.rs",
        &[("left", &one_line), ("right", &one_line)],
    );
    assert!(find_exact_clones(&too_few_lines, MIN_CLONE_TOKENS, MIN_CLONE_LINES).is_empty());
}

#[test]
fn reports_maximal_non_overlapping_matches_deterministically() {
    let functions = fixture(
        "crates/a/src/lib.rs",
        &[("alpha", CLONE), ("beta", CLONE), ("gamma", CLONE)],
    );

    let first = find_exact_clones(&functions, MIN_CLONE_TOKENS, MIN_CLONE_LINES);
    let second = find_exact_clones(&functions, MIN_CLONE_TOKENS, MIN_CLONE_LINES);

    assert_eq!(first, second);
    assert_eq!(first.len(), 3);
    assert!(
        first
            .iter()
            .all(|clone| clone.token_count > MIN_CLONE_TOKENS)
    );
}
