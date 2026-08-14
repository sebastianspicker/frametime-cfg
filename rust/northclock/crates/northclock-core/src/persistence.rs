use crate::{Measurement, NorthclockError, Result};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const SETTINGS_SCHEMA_VERSION: u32 = 1;
static NEXT_STORAGE_FILE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppSettings {
    pub schema_version: u32,
    pub selected_profile: Option<String>,
    pub measurement_interval_ms: u64,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            schema_version: SETTINGS_SCHEMA_VERSION,
            selected_profile: None,
            measurement_interval_ms: 1_000,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Profile {
    pub schema_version: u32,
    pub name: String,
    pub values: std::collections::BTreeMap<String, i64>,
    pub imported_from_ini: bool,
}

#[derive(Clone, Debug)]
pub struct Storage {
    root: PathBuf,
}

impl Storage {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn load_settings(&self) -> Result<AppSettings> {
        let path = self.root.join("settings.toml");
        if !path.exists() {
            return Ok(AppSettings::default());
        }
        let source = fs::read_to_string(&path).map_err(io_error)?;
        let settings: AppSettings = toml::from_str(&source).map_err(|error| {
            NorthclockError::Internal(format!("invalid settings TOML: {error}"))
        })?;
        if settings.schema_version != SETTINGS_SCHEMA_VERSION {
            return Err(NorthclockError::Internal(format!(
                "unsupported settings schema {}",
                settings.schema_version
            )));
        }
        Ok(settings)
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<()> {
        if settings.schema_version != SETTINGS_SCHEMA_VERSION {
            return Err(NorthclockError::InvalidUsage(
                "settings schema does not match this Northclock build".into(),
            ));
        }
        let data = toml::to_string_pretty(settings)
            .map_err(|error| NorthclockError::Internal(error.to_string()))?;
        self.replace_file("settings.toml", data.as_bytes())
    }

    pub fn import_ini_once(&self, ini_path: &Path) -> Result<Option<Profile>> {
        let marker = self.root.join("imports").join("legacy-ini.complete");
        if marker.exists() || !ini_path.exists() {
            return Ok(None);
        }
        let source = fs::read_to_string(ini_path).map_err(io_error)?;
        let mut values = std::collections::BTreeMap::new();
        for line in source.lines() {
            let line = line.trim();
            if line.is_empty()
                || line.starts_with(';')
                || line.starts_with('#')
                || line.starts_with('[')
            {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                if let Ok(value) = value.trim().parse::<i64>() {
                    values.insert(key.trim().to_ascii_lowercase(), value);
                }
            }
        }
        let profile = Profile {
            schema_version: SETTINGS_SCHEMA_VERSION,
            name: normalize_legacy_profile_name(
                ini_path
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .unwrap_or("legacy-profile"),
            ),
            values,
            imported_from_ini: true,
        };
        let data = toml::to_string_pretty(&profile)
            .map_err(|error| NorthclockError::Internal(error.to_string()))?;
        self.replace_file("profiles/imported-legacy.toml", data.as_bytes())?;
        self.replace_file(
            "imports/legacy-ini.complete",
            ini_path.as_os_str().as_encoded_bytes(),
        )?;
        Ok(Some(profile))
    }

    pub fn list_profiles(&self) -> Result<Vec<Profile>> {
        let directory = self.root.join("profiles");
        if !directory.exists() {
            return Ok(Vec::new());
        }
        let mut paths = fs::read_dir(directory)
            .map_err(io_error)?
            .filter_map(std::result::Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "toml")
            })
            .collect::<Vec<_>>();
        paths.sort();
        paths
            .into_iter()
            .map(|path| self.load_profile_path(&path))
            .collect()
    }

    pub fn save_profile(&self, profile: &Profile) -> Result<()> {
        validate_profile(profile)?;
        let file_name = profile_file_name(&profile.name)?;
        let data = toml::to_string_pretty(profile)
            .map_err(|error| NorthclockError::Internal(error.to_string()))?;
        self.replace_file(&format!("profiles/{file_name}.toml"), data.as_bytes())
    }

    pub fn append_history<T: Serialize + ?Sized>(&self, record: &T) -> Result<()> {
        self.append_line(
            "history.jsonl",
            serde_json::to_string(record).map_err(json_error)?,
        )
    }

    pub fn append_measurements_csv(&self, measurements: &[Measurement<f64>]) -> Result<()> {
        if measurements.is_empty() {
            return Err(NorthclockError::InvalidUsage(
                "refusing to create a measurement file without backend data".into(),
            ));
        }
        fs::create_dir_all(&self.root).map_err(io_error)?;
        let path = self.root.join("measurements.csv");
        let new_file = !path.exists();
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(io_error)?;
        if new_file {
            writeln!(file, "timestamp_unix_ms,device_id,value,unit,source").map_err(io_error)?;
        }
        for measurement in measurements {
            writeln!(
                file,
                "{},{},{},{},{}",
                measurement.timestamp_unix_ms,
                csv_escape(&measurement.device.stable_id),
                measurement.value,
                csv_escape(&measurement.unit),
                csv_escape(&measurement.source)
            )
            .map_err(io_error)?;
        }
        file.flush().map_err(io_error)?;
        file.sync_data().map_err(io_error)
    }

    fn replace_file(&self, relative_path: &str, data: &[u8]) -> Result<()> {
        let path = self.root.join(relative_path);
        let parent = path.parent().ok_or_else(|| {
            NorthclockError::Internal("storage path does not have a parent".into())
        })?;
        fs::create_dir_all(parent).map_err(io_error)?;
        let unique = unique_file_suffix();
        let temporary = path.with_extension(format!("tmp-{unique}"));
        let backup = path.with_extension(format!("bak-{unique}"));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(io_error)?;
        file.write_all(data).map_err(io_error)?;
        file.sync_all().map_err(io_error)?;
        drop(file);
        if !path.exists() {
            return fs::rename(&temporary, &path).map_err(io_error);
        }
        fs::rename(&path, &backup).map_err(io_error)?;
        if let Err(error) = fs::rename(&temporary, &path) {
            let restore = fs::rename(&backup, &path);
            return match restore {
                Ok(()) => Err(io_error(error)),
                Err(restore_error) => Err(NorthclockError::Internal(format!(
                    "settings replacement failed ({error}) and backup restoration failed ({restore_error})"
                ))),
            };
        }
        fs::remove_file(&backup).map_err(io_error)
    }

    fn append_line(&self, relative_path: &str, line: String) -> Result<()> {
        fs::create_dir_all(&self.root).map_err(io_error)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.root.join(relative_path))
            .map_err(io_error)?;
        writeln!(file, "{line}").map_err(io_error)?;
        file.flush().map_err(io_error)?;
        file.sync_data().map_err(io_error)
    }

    fn load_profile_path(&self, path: &Path) -> Result<Profile> {
        let source = fs::read_to_string(path).map_err(io_error)?;
        let profile: Profile = toml::from_str(&source).map_err(|error| {
            NorthclockError::Internal(format!(
                "invalid profile TOML in {}: {error}",
                path.display()
            ))
        })?;
        validate_profile(&profile)?;
        Ok(profile)
    }
}

fn validate_profile(profile: &Profile) -> Result<()> {
    if profile.schema_version != SETTINGS_SCHEMA_VERSION {
        return Err(NorthclockError::InvalidUsage(format!(
            "unsupported profile schema {}",
            profile.schema_version
        )));
    }
    profile_file_name(&profile.name).map(|_| ())
}

fn profile_file_name(name: &str) -> Result<String> {
    let valid = !name.is_empty()
        && name.len() <= 80
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    if !valid {
        return Err(NorthclockError::InvalidUsage(
            "profile names may contain only ASCII letters, numbers, '-' and '_'".into(),
        ));
    }
    Ok(name.to_ascii_lowercase())
}

fn unique_file_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let sequence = NEXT_STORAGE_FILE.fetch_add(1, Ordering::Relaxed);
    format!("{}-{nanos}-{sequence}", std::process::id())
}

fn normalize_legacy_profile_name(name: &str) -> String {
    let normalized = name
        .chars()
        .take(80)
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    if normalized.is_empty() {
        "legacy-profile".into()
    } else {
        normalized
    }
}

fn io_error(error: std::io::Error) -> NorthclockError {
    NorthclockError::Internal(error.to_string())
}

fn json_error(error: serde_json::Error) -> NorthclockError {
    NorthclockError::Internal(error.to_string())
}

fn csv_escape(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_dir(label: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        std::env::temp_dir().join(format!("northclock-{label}-{}-{stamp}", std::process::id()))
    }

    #[test]
    fn settings_round_trip_with_schema() {
        let root = test_dir("settings");
        let storage = Storage::new(&root);
        let settings = AppSettings {
            selected_profile: Some("quiet".into()),
            ..AppSettings::default()
        };
        storage
            .save_settings(&settings)
            .unwrap_or_else(|error| panic!("save failed: {error}"));
        storage
            .save_settings(&settings)
            .unwrap_or_else(|error| panic!("replacement save failed: {error}"));
        assert_eq!(
            storage
                .load_settings()
                .unwrap_or_else(|error| panic!("load failed: {error}")),
            settings
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ini_import_is_non_destructive_and_one_shot() {
        let root = test_dir("ini");
        fs::create_dir_all(&root).unwrap_or_else(|error| panic!("mkdir failed: {error}"));
        let ini = root.join("old.ini");
        fs::write(&ini, "[CPU]\nCurveOptimizer=-12\n")
            .unwrap_or_else(|error| panic!("fixture failed: {error}"));
        let storage = Storage::new(root.join("app"));
        let imported = storage
            .import_ini_once(&ini)
            .unwrap_or_else(|error| panic!("import failed: {error}"));
        assert_eq!(
            imported.and_then(|profile| profile.values.get("curveoptimizer").copied()),
            Some(-12)
        );
        assert!(storage
            .import_ini_once(&ini)
            .unwrap_or_else(|error| panic!("second import failed: {error}"))
            .is_none());
        assert!(ini.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn measurement_file_requires_real_input() {
        let root = test_dir("measurements");
        let storage = Storage::new(&root);
        let error = storage
            .append_measurements_csv(&[])
            .err()
            .unwrap_or_else(|| panic!("empty measurement set unexpectedly persisted"));
        assert_eq!(error.exit_code(), 2);
        assert!(!root.join("measurements.csv").exists());
    }

    #[test]
    fn profiles_are_versioned_listed_and_path_safe() {
        let root = test_dir("profiles");
        let storage = Storage::new(&root);
        let profile = Profile {
            schema_version: SETTINGS_SCHEMA_VERSION,
            name: "quiet_mode".into(),
            values: std::collections::BTreeMap::from([("curve_optimizer".into(), -8)]),
            imported_from_ini: false,
        };
        storage
            .save_profile(&profile)
            .unwrap_or_else(|error| panic!("profile save failed: {error}"));
        assert_eq!(
            storage
                .list_profiles()
                .unwrap_or_else(|error| panic!("profile list failed: {error}")),
            vec![profile]
        );
        let invalid = Profile {
            schema_version: SETTINGS_SCHEMA_VERSION,
            name: "../escape".into(),
            values: Default::default(),
            imported_from_ini: false,
        };
        assert_eq!(
            storage
                .save_profile(&invalid)
                .err()
                .map(|error| error.exit_code()),
            Some(2)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn measurement_csv_writes_one_header_and_escapes_fields() {
        let root = test_dir("measurement-csv");
        let storage = Storage::new(&root);
        let measurement = Measurement::at(
            42.0,
            "MHz, effective",
            7,
            crate::DeviceIdentity::new("cpu", "cpu,0", "CPU", None),
            "test\"backend",
        );
        storage
            .append_measurements_csv(std::slice::from_ref(&measurement))
            .unwrap_or_else(|error| panic!("first CSV append failed: {error}"));
        storage
            .append_measurements_csv(&[measurement])
            .unwrap_or_else(|error| panic!("second CSV append failed: {error}"));
        let source = fs::read_to_string(root.join("measurements.csv"))
            .unwrap_or_else(|error| panic!("CSV read failed: {error}"));
        assert_eq!(source.matches("timestamp_unix_ms").count(), 1);
        assert!(source.contains("\"cpu,0\""));
        assert!(source.contains("\"test\"\"backend\""));
        let _ = fs::remove_dir_all(root);
    }
}
