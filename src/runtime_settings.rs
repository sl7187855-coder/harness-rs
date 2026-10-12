use serde_json::Value;
use std::{env, path::Path};

pub const EXECUTION_MODES: &[&str] = &["plan", "ask", "auto-edit", "auto"];

pub fn validate_execution_mode(value: &str) -> bool {
    EXECUTION_MODES.contains(&value.trim())
}

/// Environment overrides are useful for service administrators. If the service
/// does not explicitly set the variable, the persisted HUD setting is used.
pub fn execution_mode(data_dir: &Path) -> String {
    if let Ok(value) = env::var("HARNESS_EXECUTION_MODE") {
        let value = value.trim();
        if validate_execution_mode(value) {
            return value.to_owned();
        }
    }
    read_configured_mode(data_dir).unwrap_or_else(|| "ask".to_owned())
}

pub fn read_configured_mode(data_dir: &Path) -> Option<String> {
    let contents = std::fs::read_to_string(data_dir.join("settings.json")).ok()?;
    let settings: Value = serde_json::from_str(&contents).ok()?;
    let mode = settings
        .get("execution_mode")
        .or_else(|| settings.get("executionMode"))?
        .as_str()?
        .trim();
    validate_execution_mode(mode).then(|| mode.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_four_documented_modes_are_accepted() {
        for mode in ["plan", "ask", "auto-edit", "auto"] {
            assert!(validate_execution_mode(mode));
        }
        for mode in ["", "yolo", "readonly", "auto-everything"] {
            assert!(!validate_execution_mode(mode));
        }
    }

    #[test]
    fn missing_or_invalid_settings_fall_back_to_ask() {
        let temp = tempfile::tempdir().unwrap();
        assert_eq!(read_configured_mode(temp.path()), None);
        std::fs::write(
            temp.path().join("settings.json"),
            r#"{"execution_mode":"invalid"}"#,
        )
        .unwrap();
        assert_eq!(read_configured_mode(temp.path()), None);
        std::fs::write(
            temp.path().join("settings.json"),
            r#"{"execution_mode":"auto-edit"}"#,
        )
        .unwrap();
        assert_eq!(
            read_configured_mode(temp.path()).as_deref(),
            Some("auto-edit")
        );
    }
}
