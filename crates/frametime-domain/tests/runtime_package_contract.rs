use std::{collections::BTreeMap, fs, path::Path};

use frametime_domain::runtime::RUNTIME_PAYLOAD_PATHS;

#[test]
fn runtime_payloads_are_source_backed_and_listed_once_in_the_package_layout() {
    let package_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace package root");
    let package_layout =
        fs::read_to_string(package_root.join("package-layout.txt")).expect("read package layout");
    let mut layout_entries = BTreeMap::new();

    for entry in package_layout
        .lines()
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
    {
        *layout_entries.entry(entry).or_insert(0_usize) += 1;
    }

    for payload in RUNTIME_PAYLOAD_PATHS {
        assert!(
            !payload.contains('\\'),
            "runtime payload paths must use portable separators: {payload}"
        );
        assert_eq!(
            layout_entries.get(payload),
            Some(&1),
            "runtime payload must appear exactly once in package-layout.txt: {payload}"
        );

        if payload != "frametime.exe" {
            assert!(
                package_root.join(payload).is_file(),
                "runtime payload must exist in the source package: {payload}"
            );
        }
    }
}
