use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::BTreeMap;

pub(super) struct StateFields {
    values: BTreeMap<String, Value>,
}

impl StateFields {
    pub(super) const fn new(values: BTreeMap<String, Value>) -> Self {
        Self { values }
    }

    pub(super) fn string(&mut self, key: &str) -> Option<String> {
        self.values
            .remove(key)
            .as_ref()
            .and_then(Value::as_str)
            .map(str::to_owned)
    }

    pub(super) fn u32(&mut self, key: &str) -> Option<u32> {
        self.values
            .remove(key)
            .as_ref()
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
    }

    pub(super) fn u64(&mut self, key: &str) -> Option<u64> {
        self.values.remove(key).as_ref().and_then(Value::as_u64)
    }

    pub(super) fn finite_f64(&mut self, key: &str) -> Option<f64> {
        self.values
            .remove(key)
            .as_ref()
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite())
    }

    pub(super) fn nonnegative_f64(&mut self, key: &str) -> Option<f64> {
        self.finite_f64(key).filter(|value| *value >= 0.0)
    }

    pub(super) fn is_true(&mut self, key: &str) -> bool {
        self.values
            .remove(key)
            .as_ref()
            .is_some_and(|value| value == &Value::Bool(true))
    }

    pub(super) fn typed_preserving_malformed<T>(&mut self, key: &str) -> Option<T>
    where
        T: DeserializeOwned,
    {
        let value = self.values.remove(key)?;
        match serde_json::from_value(value.clone()) {
            Ok(parsed) => Some(parsed),
            Err(_) => {
                self.values.insert(key.to_owned(), value);
                None
            }
        }
    }

    pub(super) fn into_unknown(self) -> BTreeMap<String, Value> {
        self.values
    }
}
