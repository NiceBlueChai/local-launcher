//! Opens launcher targets through Windows shell and process APIs.

use crate::model::{ItemKind, LauncherItem};
use std::path::Path;
use windows::core::PCWSTR;
use windows::Win32::shellapi::ShellExecuteW;
use windows::Win32::shlobj_core::{ILCreateFromPathW, ILFree, SHOpenFolderAndSelectItems};
use windows::Win32::winuser::SW_SHOWNORMAL;

/// Opens a launcher item with the appropriate Windows mechanism.
pub fn open_item(item: &LauncherItem) -> Result<(), String> {
    match item.kind {
        ItemKind::Folder => open_folder(&item.target),
        ItemKind::Program => open_program(&item.target, &item.arguments),
        ItemKind::Url | ItemKind::IntranetUrl => open_url(&item.target),
    }
}

/// Opens the containing folder for a program launcher item.
pub fn open_containing_folder(item: &LauncherItem) -> Result<(), String> {
    if item.kind != ItemKind::Program {
        return Err("只有程序条目支持打开所在文件夹".to_string());
    }
    validate_existing_path(&item.target)?;
    if select_in_folder(&item.target).is_ok() {
        return Ok(());
    }

    let folder = containing_folder(&item.target).ok_or_else(|| "程序没有所在文件夹".to_string())?;
    validate_existing_path(&folder)?;
    shell_open(&folder, None, None, "打开所在文件夹失败")
}

/// Validates a launcher target before showing or opening it.
pub fn validate_target(item: &LauncherItem) -> Result<(), String> {
    match item.kind {
        ItemKind::Folder | ItemKind::Program => {
            if Path::new(&item.target).exists() {
                Ok(())
            } else {
                Err(format!("目标不存在: {}", item.target))
            }
        }
        ItemKind::Url | ItemKind::IntranetUrl => {
            if item.target.starts_with("http://") || item.target.starts_with("https://") {
                Ok(())
            } else {
                Err(format!(
                    "URL 必须以 http:// 或 https:// 开头: {}",
                    item.target
                ))
            }
        }
    }
}

fn open_folder(target: &str) -> Result<(), String> {
    validate_existing_path(target)?;
    shell_open(target, None, None, "打开目录失败")
}

fn open_program(target: &str, arguments: &str) -> Result<(), String> {
    validate_existing_path(target)?;
    let directory = program_working_directory(target);
    shell_open(
        target,
        Some(arguments),
        directory.as_deref(),
        "启动程序失败",
    )
}

fn open_url(target: &str) -> Result<(), String> {
    if !(target.starts_with("http://") || target.starts_with("https://")) {
        return Err(format!("URL 必须以 http:// 或 https:// 开头: {target}"));
    }

    shell_open(target, None, None, "打开网址失败")
}

fn validate_existing_path(target: &str) -> Result<(), String> {
    if Path::new(target).exists() {
        Ok(())
    } else {
        Err(format!("目标不存在: {target}"))
    }
}

fn containing_folder(target: &str) -> Option<String> {
    Path::new(target)
        .parent()
        .map(|parent| parent.display().to_string())
        .filter(|parent| !parent.trim().is_empty())
}

fn program_working_directory(target: &str) -> Option<String> {
    containing_folder(target)
}

fn select_in_folder(target: &str) -> Result<(), String> {
    let target = wide_null(target);
    let pidl = unsafe { ILCreateFromPathW(PCWSTR(target.as_ptr())) };
    if pidl.is_null() {
        return Err("创建 Shell 路径失败".to_string());
    }

    let result = unsafe {
        let result = SHOpenFolderAndSelectItems(pidl, None, 0);
        ILFree(Some(pidl));
        result
    };
    result.ok().map_err(|error| format!("选中文件失败: {error:?}"))
}

fn shell_open(
    target: &str,
    parameters: Option<&str>,
    directory: Option<&str>,
    error_prefix: &str,
) -> Result<(), String> {
    let operation = wide_null("open");
    let file = wide_null(target);
    let parameters = parameters
        .filter(|value| !value.trim().is_empty())
        .map(wide_null);
    let parameter_ptr = parameters
        .as_ref()
        .map_or(PCWSTR::null(), |value| PCWSTR(value.as_ptr()));
    let directory = directory
        .filter(|value| !value.trim().is_empty())
        .map(wide_null);
    let directory_ptr = directory
        .as_ref()
        .map_or(PCWSTR::null(), |value| PCWSTR(value.as_ptr()));
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(operation.as_ptr()),
            PCWSTR(file.as_ptr()),
            parameter_ptr,
            directory_ptr,
            SW_SHOWNORMAL,
        )
    };
    let outcome = result as isize;
    if outcome > 32 {
        Ok(())
    } else {
        Err(format!("{error_prefix}: ShellExecuteW 错误 {outcome}"))
    }
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_bad_url() {
        let item = LauncherItem {
            id: "bad".to_string(),
            name: "Bad".to_string(),
            kind: ItemKind::Url,
            target: "example.com".to_string(),
            arguments: String::new(),
            category: String::new(),
            tags: Vec::new(),
            username: String::new(),
            favorite: false,
            notes: String::new(),
            icon_format: None,
            icon_data: None,
        };

        assert!(validate_target(&item).is_err());
    }

    #[test]
    fn launcher_does_not_spawn_commands_for_opening() {
        let source = include_str!("launcher.rs");
        let command_new = concat!("Command", "::new");

        assert!(!source.contains(command_new));
    }

    #[test]
    fn program_location_selection_uses_shell_api() {
        let source = include_str!("launcher.rs");
        let shell_selection_api = concat!("SHOpenFolder", "AndSelectItems");
        let explorer_select = concat!("explorer", " /select");

        assert!(source.contains(shell_selection_api));
        assert!(!source.contains(explorer_select));
    }

    #[test]
    fn finds_program_parent_folder() {
        assert_eq!(
            containing_folder(r"C:\Tools\App\tool.exe").as_deref(),
            Some(r"C:\Tools\App")
        );
    }

    #[test]
    fn uses_program_parent_as_working_directory() {
        assert_eq!(
            program_working_directory(r"C:\Tools\App\tool.exe").as_deref(),
            Some(r"C:\Tools\App")
        );
    }
}
