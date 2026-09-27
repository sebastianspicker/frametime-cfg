use std::collections::BTreeSet;

use frametime_app::Branch;
use frametime_domain::catalog::{Step, step_catalog};
use serde_json::{Map, Value};

const ASSIGNMENT: &str = "window.FRAMETIME_REGISTER = ";

#[test]
fn published_register_matches_the_catalog_and_every_dry_run_branch() {
    let root = repo_checks::repository_root();
    let source = repo_checks::read_utf8(&root.join("site/register.js"));
    let (_, encoded) = source
        .split_once(ASSIGNMENT)
        .expect("site register must use the fixed window assignment");
    let register: Value = serde_json::from_str(
        encoded
            .trim()
            .strip_suffix(';')
            .expect("site register assignment must end with a semicolon"),
    )
    .expect("site register assignment must contain JSON");

    assert_eq!(register["command"], "frametime dry-run all");
    assert_steps(register["steps"].as_array().expect("steps array"));

    let branches = register["branches"].as_array().expect("branches array");
    let expected = [
        ("1", "NVIDIA RTX 5000", Branch::NvidiaRtx5000),
        ("2", "Other NVIDIA", Branch::Nvidia),
        ("3", "AMD Radeon", Branch::Amd),
        ("4", "Intel Arc", Branch::IntelArc),
    ];
    assert_eq!(branches.len(), expected.len());
    for (key, label, branch) in expected {
        let published = branches
            .iter()
            .find(|value| value["key"] == key)
            .unwrap_or_else(|| panic!("missing site register branch {key}"));
        assert_eq!(published["label"], label);
        assert_branch(
            branch,
            published["results"].as_object().expect("branch results"),
        );
    }
}

fn assert_steps(published: &[Value]) {
    assert_eq!(published.len(), step_catalog().len());
    let mut ids = BTreeSet::new();
    for (actual, expected) in published.iter().zip(step_catalog()) {
        let id = expected.id.progress_key();
        assert!(ids.insert(id.clone()), "duplicate step {id}");
        assert_eq!(actual["id"], id, "step ID");
        assert_eq!(actual["category"], expected.category, "{id} category");
        assert_eq!(actual["title"], expected.title, "{id} title");
        assert_eq!(actual["tier"], expected.tier, "{id} tier");
        assert_eq!(actual["risk"], format!("{:?}", expected.risk), "{id} risk");
        assert_eq!(actual["check"], expected.check_only, "{id} check-only");
        assert_eq!(actual["reboot"], expected.reboot, "{id} reboot");
    }
}

fn assert_branch(branch: Branch, published: &Map<String, Value>) {
    let preview = frametime_app::run_dry(branch).expect("dry-run branch");
    let lines = preview
        .lines
        .iter()
        .filter_map(|line| line.strip_prefix("[DRY-RUN] "))
        .collect::<Vec<_>>();
    assert_eq!(lines.len(), step_catalog().len());
    assert_eq!(published.len(), step_catalog().len());
    for (step, line) in step_catalog().iter().zip(lines) {
        let id = step.id.progress_key();
        let result = published
            .get(&id)
            .unwrap_or_else(|| panic!("missing site result for {id}"));
        assert_eq!(result["line"], line, "{id} preview line");
        assert_eq!(result["v"], verdict(step, line), "{id} verdict");
    }
}

fn verdict(step: &Step, line: &str) -> &'static str {
    if line.starts_with("Would skip inapplicable") {
        "skip"
    } else if line.contains(" is advisory and unverified: ") {
        "keep"
    } else if line.starts_with("Would require a complete VProf capture") {
        "evidence"
    } else if step.check_only {
        "inspect"
    } else {
        "change"
    }
}
