//! Settings types

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Runtime settings type
pub type RuntimeSettings = HashMap<String, serde_json::Value>;

/// Setting value variants
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SettingValue {
    /// String value
    String(String),
    /// Boolean value
    Bool(bool),
    /// Number value
    Number(f64),
    /// Null value
    Null,
    /// Complex value
    Object(serde_json::Value),
}

impl From<String> for SettingValue {
    fn from(s: String) -> Self {
        SettingValue::String(s)
    }
}

impl From<&str> for SettingValue {
    fn from(s: &str) -> Self {
        SettingValue::String(s.to_string())
    }
}

impl From<bool> for SettingValue {
    fn from(b: bool) -> Self {
        SettingValue::Bool(b)
    }
}

impl From<f64> for SettingValue {
    fn from(n: f64) -> Self {
        SettingValue::Number(n)
    }
}

impl From<serde_json::Value> for SettingValue {
    fn from(v: serde_json::Value) -> Self {
        match v {
            serde_json::Value::String(s) => SettingValue::String(s),
            serde_json::Value::Bool(b) => SettingValue::Bool(b),
            serde_json::Value::Number(n) => SettingValue::Number(n.as_f64().unwrap_or(0.0)),
            serde_json::Value::Null => SettingValue::Null,
            other => SettingValue::Object(other),
        }
    }
}

impl From<SettingValue> for serde_json::Value {
    fn from(val: SettingValue) -> Self {
        match val {
            SettingValue::String(s) => serde_json::Value::String(s),
            SettingValue::Bool(b) => serde_json::Value::Bool(b),
            SettingValue::Number(n) => serde_json::Value::Number(
                serde_json::Number::from_f64(n).unwrap_or(serde_json::Number::from(0)),
            ),
            SettingValue::Null => serde_json::Value::Null,
            SettingValue::Object(o) => o,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setting_value_from_string() {
        let val: SettingValue = "test".into();
        match val {
            SettingValue::String(s) => assert_eq!(s, "test"),
            _ => panic!("Expected String variant"),
        }
    }

    #[test]
    fn test_setting_value_from_bool() {
        let val: SettingValue = true.into();
        match val {
            SettingValue::Bool(b) => assert!(b),
            _ => panic!("Expected Bool variant"),
        }
    }

    #[test]
    fn test_setting_value_to_json() {
        let val = SettingValue::String("test".to_string());
        let json: serde_json::Value = val.into();
        assert_eq!(json, serde_json::Value::String("test".to_string()));
    }
}
