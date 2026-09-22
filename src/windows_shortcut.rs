//! Native Windows shortcut helpers for desktop and startup links.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::PathBuf;

use crate::APP_ICON_PATH;
use windows::core::{Interface, PCWSTR};
use windows::Win32::combaseapi::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
};
use windows::Win32::knownfolders::{FOLDERID_Desktop, FOLDERID_Startup};
use windows::Win32::objbase::COINIT_APARTMENTTHREADED;
use windows::Win32::objidl::IPersistFile;
use windows::Win32::shlobj_core::{KF_FLAG_DEFAULT, SHGetKnownFolderPath};
use windows::Win32::shobjidl_core::{IShellLinkW, ShellLink};
use windows::Win32::winuser::SW_SHOWNORMAL;
use windows::Win32::wtypesbase::CLSCTX_INPROC_SERVER;

#[cfg(windows)]
use std::os::windows::ffi::{OsStrExt, OsStringExt};

enum KnownShortcutFolder {
    Desktop,
    Startup,
}

struct ComApartment;

impl ComApartment {
    fn initialize() -> Result<Self, String> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED as u32)
                .ok()
                .map_err(|error| format!("initialize COM apartment: {error}"))?;
        }

        Ok(Self)
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

/// Returns whether the desktop shortcut exists.
#[allow(dead_code)]
pub fn desktop_shortcut_exists() -> bool {
    shortcut_path(KnownShortcutFolder::Desktop).is_ok_and(|path| path.exists())
}

/// Creates or replaces the desktop shortcut for the current executable.
#[allow(dead_code)]
pub fn create_desktop_shortcut() -> Result<(), String> {
    create_shortcut(KnownShortcutFolder::Desktop)
}

/// Enables or disables launch at login through the user's Startup shortcut folder.
#[allow(dead_code)]
pub fn set_launch_at_login(enabled: bool) -> Result<(), String> {
    if enabled {
        create_shortcut(KnownShortcutFolder::Startup)
    } else {
        remove_shortcut(KnownShortcutFolder::Startup)
    }
}

/// Returns whether launch at login is enabled through the user's Startup shortcut folder.
#[allow(dead_code)]
pub fn launch_at_login_enabled() -> bool {
    shortcut_path(KnownShortcutFolder::Startup).is_ok_and(|path| path.exists())
}

pub(crate) fn shortcut_file_name() -> &'static str {
    "Local Launcher.lnk"
}

fn create_shortcut(folder: KnownShortcutFolder) -> Result<(), String> {
    let _apartment = ComApartment::initialize()?;
    let executable =
        std::env::current_exe().map_err(|error| format!("resolve current exe: {error}"))?;
    let working_directory = executable
        .parent()
        .ok_or_else(|| "resolve current exe directory".to_string())?;
    let shortcut = shortcut_path(folder)?;
    let executable = wide_null(executable.as_os_str());
    let working_directory = wide_null(working_directory.as_os_str());
    let icon = wide_null(OsStr::new(APP_ICON_PATH));
    let shortcut = wide_null(shortcut.as_os_str());

    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
            .map_err(|error| format!("create ShellLink instance: {error}"))?;

        link.SetPath(PCWSTR(executable.as_ptr()))
            .ok()
            .map_err(|error| format!("set shortcut target: {error}"))?;
        link.SetWorkingDirectory(PCWSTR(working_directory.as_ptr()))
            .ok()
            .map_err(|error| format!("set shortcut working directory: {error}"))?;
        link.SetIconLocation(PCWSTR(icon.as_ptr()), 0)
            .ok()
            .map_err(|error| format!("set shortcut icon: {error}"))?;
        link.SetShowCmd(SW_SHOWNORMAL)
            .ok()
            .map_err(|error| format!("set shortcut show command: {error}"))?;

        let file: IPersistFile = link
            .cast()
            .map_err(|error| format!("open shortcut persistence: {error}"))?;
        file.Save(shortcut.as_ptr(), true)
            .ok()
            .map_err(|error| format!("save shortcut: {error}"))?;
    }

    Ok(())
}

fn remove_shortcut(folder: KnownShortcutFolder) -> Result<(), String> {
    let path = shortcut_path(folder)?;

    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("remove shortcut {}: {error}", path.display())),
    }
}

fn shortcut_path(folder: KnownShortcutFolder) -> Result<PathBuf, String> {
    Ok(known_folder_path(folder)?.join(shortcut_file_name()))
}

fn known_folder_path(folder: KnownShortcutFolder) -> Result<PathBuf, String> {
    let folder_id = match folder {
        KnownShortcutFolder::Desktop => &FOLDERID_Desktop,
        KnownShortcutFolder::Startup => &FOLDERID_Startup,
    };

    unsafe {
        let path = SHGetKnownFolderPath(folder_id, KF_FLAG_DEFAULT, None)
            .map_err(|error| format!("resolve known folder: {error}"))?;
        let result = PathBuf::from(OsString::from_wide(path.as_wide()));
        CoTaskMemFree(path.as_ptr().cast());
        Ok(result)
    }
}

#[cfg(windows)]
fn wide_null(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_module_does_not_spawn_shells() {
        let source = include_str!("windows_shortcut.rs");
        let command_new = concat!("Command", "::new");
        let powershell = concat!("powershell", ".exe");
        let cmd = concat!("cmd", ".exe");

        assert!(!source.contains(command_new));
        assert!(!source.contains(powershell));
        assert!(!source.contains(cmd));
    }

    #[test]
    fn shortcut_file_name_is_stable() {
        assert_eq!(shortcut_file_name(), "Local Launcher.lnk");
    }
}
