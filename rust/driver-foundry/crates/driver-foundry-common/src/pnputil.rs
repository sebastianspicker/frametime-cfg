//! Shared parsing for `pnputil /enum-drivers` output.

/// One OEM driver package reported by PnPUtil.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OemDriverPackage {
    pub published_name: String,
    pub original_name: String,
    pub provider: String,
    pub class_name: String,
}

/// Parse English and common German PnPUtil field labels.
pub fn parse_pnputil_enum_drivers(text: &str) -> Vec<OemDriverPackage> {
    let mut packages = Vec::new();
    let mut current = OemDriverPackage::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            flush(&mut packages, &mut current);
            continue;
        }
        let lower = line.to_ascii_lowercase();
        let Some((_, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        if lower.contains("published name") || lower.contains("veröffentlichter name") {
            current.published_name = value.to_owned();
        } else if lower.contains("original name") || lower.contains("ursprünglicher name") {
            current.original_name = value.to_owned();
        } else if lower.contains("provider name")
            || lower.starts_with("provider")
            || lower.contains("anbietername")
        {
            current.provider = value.to_owned();
        } else if lower.contains("class name") || lower.contains("klassenname") {
            current.class_name = value.to_owned();
        }
    }
    flush(&mut packages, &mut current);
    let mut seen = std::collections::BTreeSet::new();
    packages.retain(|package| seen.insert(package.published_name.to_ascii_lowercase()));
    packages
}

fn flush(packages: &mut Vec<OemDriverPackage>, current: &mut OemDriverPackage) {
    if !current.published_name.is_empty() {
        packages.push(std::mem::take(current));
    } else {
        *current = OemDriverPackage::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_english_and_german_rows() {
        let rows = parse_pnputil_enum_drivers(
            "Published Name: oem12.inf\nOriginal Name: nv_disp.inf\nProvider Name: NVIDIA\nClass Name: Display\n\nVeröffentlichter Name: oem13.inf\nUrsprünglicher Name: iigd.inf\nAnbietername: Intel\nKlassenname: Display\n",
        );
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].published_name, "oem12.inf");
        assert_eq!(rows[1].provider, "Intel");
    }

    #[test]
    fn deduplicates_published_names_case_insensitively() {
        let rows = parse_pnputil_enum_drivers(
            "Published Name: oem12.inf\nProvider Name: NVIDIA\n\nPublished Name: OEM12.INF\nProvider Name: NVIDIA\n",
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].published_name, "oem12.inf");
    }
}
