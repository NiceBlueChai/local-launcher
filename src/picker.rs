//! Native Windows file and folder pickers.

use windows::{
    Win32::{
        combaseapi::{
            CLSCTX_ALL, CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
        },
        objbase::COINIT_APARTMENTTHREADED,
        shobjidl_core::{
            FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS,
            FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH,
        },
        shtypes::COMDLG_FILTERSPEC,
        winerror::ERROR_CANCELLED,
    },
    core::{Error, WIN32_ERROR, w},
};

/// Shows a native folder picker and returns the selected path.
pub fn pick_folder() -> Result<Option<String>, String> {
    pick_path(PickerKind::Folder)
}

/// Shows a native executable picker and returns the selected path.
pub fn pick_program() -> Result<Option<String>, String> {
    pick_path(PickerKind::Program)
}

/// Shows a native JSON picker and returns the selected path.
pub fn pick_json_file() -> Result<Option<String>, String> {
    pick_path(PickerKind::JsonFile)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PickerKind {
    Folder,
    Program,
    JsonFile,
}

struct ComApartment;

impl ComApartment {
    fn init() -> Result<Self, String> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED as u32)
                .ok()
                .map_err(|e| format!("初始化选择对话框失败: {}", e.message()))?;
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

fn pick_path(kind: PickerKind) -> Result<Option<String>, String> {
    let _apartment = ComApartment::init()?;

    unsafe {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL)
            .map_err(|e| format!("创建选择对话框失败: {}", e.message()))?;

        configure_dialog(&dialog, kind)?;

        if let Err(error) = dialog.Show(None).ok() {
            return if is_cancelled(&error) {
                Ok(None)
            } else {
                Err(format!("打开选择对话框失败: {}", error.message()))
            };
        }

        let item = dialog
            .GetResult()
            .map_err(|e| format!("读取选择结果失败: {}", e.message()))?;
        let path = item
            .GetDisplayName(SIGDN_FILESYSPATH)
            .map_err(|e| format!("读取选择路径失败: {}", e.message()))?;
        let selected = path.to_string().map_err(|e| format!("读取选择路径失败: {e}"))?;
        CoTaskMemFree(path.0 as _);

        Ok((!selected.trim().is_empty()).then_some(selected))
    }
}

fn configure_dialog(dialog: &IFileOpenDialog, kind: PickerKind) -> Result<(), String> {
    let filters: &[COMDLG_FILTERSPEC] = match kind {
        PickerKind::Folder => &[],
        PickerKind::Program => &[
            COMDLG_FILTERSPEC {
                pszName: w!("可执行程序 (*.exe)"),
                pszSpec: w!("*.exe"),
            },
            COMDLG_FILTERSPEC {
                pszName: w!("所有文件 (*.*)"),
                pszSpec: w!("*.*"),
            },
        ],
        PickerKind::JsonFile => &[
            COMDLG_FILTERSPEC {
                pszName: w!("JSON 文件 (*.json)"),
                pszSpec: w!("*.json"),
            },
            COMDLG_FILTERSPEC {
                pszName: w!("所有文件 (*.*)"),
                pszSpec: w!("*.*"),
            },
        ],
    };

    unsafe {
        let mut options = dialog
            .GetOptions()
            .map_err(|e| format!("读取对话框选项失败: {}", e.message()))?
            | FOS_FORCEFILESYSTEM
            | FOS_PATHMUSTEXIST;

        match kind {
            PickerKind::Folder => {
                options |= FOS_PICKFOLDERS;
                dialog
                    .SetTitle(w!("选择目录"))
                    .ok()
                    .map_err(|e| format!("设置目录选择标题失败: {}", e.message()))?;
            }
            PickerKind::Program => {
                options |= FOS_FILEMUSTEXIST;
                dialog
                    .SetTitle(w!("选择程序"))
                    .ok()
                    .map_err(|e| format!("设置程序选择标题失败: {}", e.message()))?;
                dialog
                    .SetFileTypes(filters.len() as u32, filters.as_ptr())
                    .ok()
                    .map_err(|e| format!("设置程序文件过滤失败: {}", e.message()))?;
                dialog
                    .SetFileTypeIndex(1)
                    .ok()
                    .map_err(|e| format!("设置默认文件过滤失败: {}", e.message()))?;
            }
            PickerKind::JsonFile => {
                options |= FOS_FILEMUSTEXIST;
                dialog
                    .SetTitle(w!("选择导入文件"))
                    .ok()
                    .map_err(|e| format!("设置导入选择标题失败: {}", e.message()))?;
                dialog
                    .SetFileTypes(filters.len() as u32, filters.as_ptr())
                    .ok()
                    .map_err(|e| format!("设置导入文件过滤失败: {}", e.message()))?;
                dialog
                    .SetFileTypeIndex(1)
                    .ok()
                    .map_err(|e| format!("设置默认文件过滤失败: {}", e.message()))?;
            }
        }

        dialog
            .SetOptions(options)
            .ok()
            .map_err(|e| format!("设置选择对话框选项失败: {}", e.message()))
    }
}

fn is_cancelled(error: &Error) -> bool {
    WIN32_ERROR::from_error(error) == Some(WIN32_ERROR(ERROR_CANCELLED as u32))
}

#[cfg(test)]
mod tests {
    #[test]
    fn picker_does_not_spawn_powershell() {
        let source = include_str!("picker.rs");
        let powershell = concat!("powershell", ".exe");
        let winforms = concat!("System", ".Windows", ".Forms");
        let command_new = concat!("Command", "::new");

        assert!(!source.contains(powershell));
        assert!(!source.contains(winforms));
        assert!(!source.contains(command_new));
    }
}
