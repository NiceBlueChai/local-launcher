//! JSON persistence for launcher items.

use crate::model::{LauncherConfig, LauncherItem};
use std::fs;
use std::path::Path;

/// Loads a launcher config or returns an empty config when the file is absent.
pub fn load_or_empty(path: &Path) -> Result<LauncherConfig, String> {
    if !path.exists() {
        return Ok(LauncherConfig::empty());
    }

    let text = fs::read_to_string(path).map_err(|e| format!("读取配置失败: {e}"))?;
    let config: LauncherConfig =
        serde_json::from_str(&text).map_err(|e| format!("配置 JSON 无效: {e}"))?;
    validate_config(&config)?;
    Ok(config)
}

/// Saves a validated launcher config to disk.
pub fn save(path: &Path, config: &LauncherConfig) -> Result<(), String> {
    validate_config(config)?;
    let text = serde_json::to_string_pretty(config).map_err(|e| format!("序列化配置失败: {e}"))?;
    fs::write(path, text).map_err(|e| format!("保存配置失败: {e}"))
}

/// Loads and validates a launcher config from an import path.
pub fn import_from(path: &Path) -> Result<LauncherConfig, String> {
    load_or_empty(path)
}

/// Merges imported launcher items into the current config by item id.
pub fn merge_config(
    mut current: LauncherConfig,
    imported: LauncherConfig,
) -> Result<LauncherConfig, String> {
    validate_config(&current)?;
    validate_config(&imported)?;
    for item in imported.items {
        if let Some(existing) = current
            .items
            .iter_mut()
            .find(|existing| existing.id == item.id)
        {
            *existing = item;
        } else {
            current.items.push(item);
        }
    }
    Ok(current)
}

/// Exports the current launcher config to a target path.
pub fn export_to(path: &Path, config: &LauncherConfig) -> Result<(), String> {
    save(path, config)
}

/// Validates the config version and required item fields.
pub fn validate_config(config: &LauncherConfig) -> Result<(), String> {
    if config.version != 1 {
        return Err(format!("不支持的配置版本: {}", config.version));
    }

    for item in &config.items {
        validate_item(item)?;
    }

    Ok(())
}

fn validate_item(item: &LauncherItem) -> Result<(), String> {
    if item.id.trim().is_empty() {
        return Err("条目缺少 id".to_string());
    }
    if item.name.trim().is_empty() {
        return Err(format!("条目 {} 缺少名称", item.id));
    }
    if item.target.trim().is_empty() {
        return Err(format!("条目 {} 缺少目标", item.name));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ItemKind, LauncherItem, LauncherSettings};

    #[test]
    fn missing_file_returns_empty_config() {
        let path = std::env::temp_dir().join("launcher-missing-config.json");
        let _ = fs::remove_file(&path);

        let config = load_or_empty(&path).unwrap();

        assert_eq!(config, LauncherConfig::empty());
    }

    #[test]
    fn invalid_item_is_rejected() {
        let config = LauncherConfig {
            version: 1,
            settings: LauncherSettings::default(),
            items: vec![LauncherItem {
                id: "bad".to_string(),
                name: String::new(),
                kind: ItemKind::Folder,
                target: "C:\\".to_string(),
                arguments: String::new(),
                category: String::new(),
                tags: Vec::new(),
                username: String::new(),
                favorite: false,
                notes: String::new(),
                icon_format: None,
                icon_data: None,
            }],
        };

        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn import_merge_replaces_matching_ids_and_keeps_existing_items() {
        let current = LauncherConfig {
            version: 1,
            settings: LauncherSettings::default(),
            items: vec![item("keep", "保留"), item("replace", "旧名称")],
        };
        let imported = LauncherConfig {
            version: 1,
            settings: LauncherSettings::default(),
            items: vec![item("replace", "新名称"), item("add", "新增")],
        };

        let merged = merge_config(current, imported).unwrap();

        assert_eq!(
            merged
                .items
                .iter()
                .map(|item| (item.id.as_str(), item.name.as_str()))
                .collect::<Vec<_>>(),
            vec![("keep", "保留"), ("replace", "新名称"), ("add", "新增")]
        );
    }

    #[test]
    fn imported_config_preserves_current_settings() {
        let current = LauncherConfig {
            version: 1,
            settings: LauncherSettings {
                launch_at_login: true,
                show_window_on_startup: false,
                close_to_tray: true,
                global_hotkey_enabled: true,
                global_hotkey: "Ctrl + Alt + Space".to_string(),
            },
            items: vec![item("keep", "保留")],
        };
        let imported = LauncherConfig {
            version: 1,
            settings: LauncherSettings::default(),
            items: vec![item("add", "新增")],
        };

        let merged = merge_config(current, imported).unwrap();

        assert!(merged.settings.launch_at_login);
        assert!(!merged.settings.show_window_on_startup);
        assert_eq!(merged.items.len(), 2);
    }

    fn item(id: &str, name: &str) -> LauncherItem {
        LauncherItem {
            id: id.to_string(),
            name: name.to_string(),
            kind: ItemKind::Url,
            target: "https://example.com".to_string(),
            arguments: String::new(),
            category: String::new(),
            tags: Vec::new(),
            username: String::new(),
            favorite: false,
            notes: String::new(),
            icon_format: None,
            icon_data: None,
        }
    }
}
