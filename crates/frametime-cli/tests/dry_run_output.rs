use std::{path::Path, process::Command};

use serde_json::Value;

const ASSIGNMENT: &str = "window.FRAMETIME_REGISTER = ";

#[test]
fn dry_run_all_cli_output_matches_the_published_register() {
    let output = Command::new(env!("CARGO_BIN_EXE_frametime"))
        .args(["dry-run", "all"])
        .output()
        .expect("run frametime dry-run all");
    assert!(
        output.status.success(),
        "CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = std::fs::read_to_string(root.join("site/register.js"))
        .expect("read published site register");
    let (_, encoded) = source
        .split_once(ASSIGNMENT)
        .expect("site register fixed assignment");
    let register: Value = serde_json::from_str(
        encoded
            .trim()
            .strip_suffix(';')
            .expect("site register assignment terminator"),
    )
    .expect("parse site register");
    assert_eq!(register["command"], "frametime dry-run all");

    let steps = register["steps"].as_array().expect("register steps");
    let mut expected = Vec::new();
    for branch in register["branches"].as_array().expect("register branches") {
        let results = branch["results"].as_object().expect("branch results");
        for step in steps {
            let id = step["id"].as_str().expect("step ID");
            let line = results[id]["line"].as_str().expect("dry-run line");
            expected.push(format!("[DRY-RUN] {line}"));
        }
    }
    let stdout = String::from_utf8(output.stdout).expect("CLI output is UTF-8");
    let actual = stdout
        .lines()
        .filter(|line| line.starts_with("[DRY-RUN] "))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}
