//! Shared launcher data types.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Folder,
    Program,
    Url,
    IntranetUrl,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LauncherItem {
    pub id: String,
    pub name: String,
    pub kind: ItemKind,
    pub target: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub arguments: String,
    pub category: String,
    pub tags: Vec<String>,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub notes: String,
    /// Embedded icon image format such as `png` or `ico`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_format: Option<String>,
    /// Embedded icon image bytes encoded as base64.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_data: Option<String>,
}

/// Stores global launcher behavior settings.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LauncherSettings {
    #[serde(default)]
    pub launch_at_login: bool,
    #[serde(default = "default_true")]
    pub show_window_on_startup: bool,
    #[serde(default = "default_true")]
    pub close_to_tray: bool,
    #[serde(default = "default_true")]
    pub global_hotkey_enabled: bool,
    #[serde(default = "default_hotkey")]
    pub global_hotkey: String,
}

impl Default for LauncherSettings {
    fn default() -> Self {
        Self {
            launch_at_login: false,
            show_window_on_startup: true,
            close_to_tray: true,
            global_hotkey_enabled: true,
            global_hotkey: default_hotkey(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LauncherConfig {
    pub version: u32,
    #[serde(default)]
    pub settings: LauncherSettings,
    pub items: Vec<LauncherItem>,
}

impl LauncherConfig {
    /// Returns an empty version-one launcher config.
    pub fn empty() -> Self {
        Self {
            version: 1,
            settings: LauncherSettings::default(),
            items: Vec::new(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_hotkey() -> String {
    "Ctrl + Alt + Space".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_snake_case_kind() {
        let item: LauncherItem = serde_json::from_str(
            r#"{
                "id": "a",
                "name": "内网",
                "kind": "intranet_url",
                "target": "http://10.0.0.1",
                "category": "测试",
                "tags": []
            }"#,
        )
        .unwrap();

        assert_eq!(item.kind, ItemKind::IntranetUrl);
        assert!(!item.favorite);
    }

    #[test]
    fn parses_embedded_icon_data() {
        let item: LauncherItem = serde_json::from_str(
            r#"{
                "id": "a",
                "name": "工具",
                "kind": "program",
                "target": "C:\\Tools\\app.exe",
                "category": "测试",
                "tags": [],
                "icon_format": "png",
                "icon_data": "aWNvbg=="
            }"#,
        )
        .unwrap();

        assert_eq!(item.icon_format.as_deref(), Some("png"));
        assert_eq!(item.icon_data.as_deref(), Some("aWNvbg=="));
    }

    #[test]
    fn parses_program_arguments() {
        let item: LauncherItem = serde_json::from_str(
            r#"{
                "id": "a",
                "name": "工具",
                "kind": "program",
                "target": "C:\\Tools\\app.exe",
                "arguments": "--dev --port 8080",
                "category": "测试",
                "tags": []
            }"#,
        )
        .unwrap();

        assert_eq!(item.arguments, "--dev --port 8080");
    }

    #[test]
    fn config_missing_settings_uses_defaults() {
        let config: LauncherConfig = serde_json::from_str(
            r#"{
                "version": 1,
                "items": []
            }"#,
        )
        .unwrap();

        assert_eq!(config.settings, LauncherSettings::default());
    }

    #[test]
    fn default_settings_match_first_version_behavior() {
        let settings = LauncherSettings::default();

        assert!(!settings.launch_at_login);
        assert!(settings.show_window_on_startup);
        assert!(settings.close_to_tray);
        assert!(settings.global_hotkey_enabled);
        assert_eq!(settings.global_hotkey, "Ctrl + Alt + Space");
    }
}
