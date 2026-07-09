# Settings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a lightweight in-app settings view for startup behavior, tray behavior, desktop shortcut creation, global hotkey, data location, and about information.

**Architecture:** Keep settings inside the existing `items.json` config and reuse the current Windows Reactor state pattern. Add one focused Windows integration module for `.lnk` files and route tray/global-hotkey behavior through the existing hidden tray window.

**Tech Stack:** Rust 2024, `windows-reactor`, local `windows` bindings, serde JSON, native Win32 Shell/COM/user32 APIs.

---

## File Structure

- Modify `Cargo.toml`: add the `Win32_UI_Input_KeyboardAndMouse` feature required by `RegisterHotKey`.
- Modify `src/model.rs`: add `LauncherSettings`, defaults, and `settings` on `LauncherConfig`.
- Modify `src/data_store.rs`: keep existing JSON compatible and add tests for missing settings.
- Create `src/windows_shortcut.rs`: create/check `.lnk` files in Desktop and Startup folders with Shell COM APIs.
- Modify `src/tray.rs`: accept settings, honor close-to-tray, hide on startup when configured, and register/unregister one global hotkey.
- Modify `src/main.rs`: load settings before installing tray and register the new module.
- Modify `src/ui.rs`: add Settings mode, header gear button, settings view, immediate save helpers, and desktop/data buttons.
- Modify `items.example.json`: include the new `settings` object.

## Scope Notes

- Do not add a new dependency.
- Do not implement password storage, themes, backup, multiple hotkeys, or registry startup.
- Use native APIs only. No PowerShell, no `cmd`, no external helper process.

---

### Task 1: Persist Settings In The Existing Config

**Files:**
- Modify: `src/model.rs`
- Modify: `src/data_store.rs`
- Modify: `items.example.json`

- [ ] **Step 1: Add failing tests for default settings and old JSON compatibility**

Add these tests to `src/model.rs` under the existing test module:

```rust
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
```

Add this test to `src/data_store.rs`:

```rust
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
```

Also update the test imports:

```rust
use crate::model::{ItemKind, LauncherItem, LauncherSettings};
```

- [ ] **Step 2: Run tests and verify they fail**

Run:

```powershell
cargo test model::tests::config_missing_settings_uses_defaults model::tests::default_settings_match_first_version_behavior data_store::tests::imported_config_preserves_current_settings
```

Expected: FAIL because `LauncherSettings` and `LauncherConfig.settings` do not exist.

- [ ] **Step 3: Add the settings model**

In `src/model.rs`, replace `LauncherConfig` with:

```rust
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

fn default_true() -> bool {
    true
}

fn default_hotkey() -> String {
    "Ctrl + Alt + Space".to_string()
}
```

Update `LauncherConfig::empty()`:

```rust
Self {
    version: 1,
    settings: LauncherSettings::default(),
    items: Vec::new(),
}
```

Update every test `LauncherConfig { ... }` literal in `src/data_store.rs` to include:

```rust
settings: LauncherSettings::default(),
```

- [ ] **Step 4: Keep imports and validation clean**

In `src/data_store.rs`, update the top import:

```rust
use crate::model::{LauncherConfig, LauncherItem};
```

The production import does not need `LauncherSettings`; only the test module does.

Keep `merge_config` as item-only merge so imports do not overwrite local settings:

```rust
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
```

- [ ] **Step 5: Update `items.example.json`**

Add this object after `version`:

```json
"settings": {
    "launch_at_login": false,
    "show_window_on_startup": true,
    "close_to_tray": true,
    "global_hotkey_enabled": true,
    "global_hotkey": "Ctrl + Alt + Space"
},
```

- [ ] **Step 6: Run tests and commit**

Run:

```powershell
cargo test model data_store
```

Expected: PASS.

Commit:

```powershell
git add src/model.rs src/data_store.rs items.example.json
git commit -m "feat(settings): 保存启动和快捷键配置"
```

---

### Task 2: Add Native Shortcut And Startup-Link Operations

**Files:**
- Modify: `Cargo.toml`
- Create: `src/windows_shortcut.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write tests that prevent command-shell regressions and validate paths**

Create `src/windows_shortcut.rs` with this initial test module and file comment:

```rust
//! Native Windows shortcut helpers for desktop and startup links.

#[cfg(test)]
mod tests {
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
```

- [ ] **Step 2: Run test and verify it fails**

Run:

```powershell
cargo test windows_shortcut
```

Expected: FAIL because `shortcut_file_name` is missing and the module is not wired.

- [ ] **Step 3: Wire module and Windows feature**

In `src/main.rs`, add:

```rust
mod windows_shortcut;
```

In `Cargo.toml`, add this feature under `[dependencies.windows].features`:

```toml
"Win32_UI_Input_KeyboardAndMouse",
```

- [ ] **Step 4: Implement shortcut helpers**

Replace `src/windows_shortcut.rs` content with:

```rust
//! Native Windows shortcut helpers for desktop and startup links.

use crate::APP_ICON_PATH;
use std::fs;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize, IPersistFile,
};
use windows::Win32::UI::Shell::{
    FOLDERID_Desktop, FOLDERID_Startup, IShellLinkW, SHCoCreateInstance, SHGetKnownFolderPath,
    ShellLink,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{Interface, PCWSTR};

/// Returns true when the desktop shortcut already exists.
pub fn desktop_shortcut_exists() -> bool {
    shortcut_path(KnownShortcutFolder::Desktop).is_ok_and(|path| path.exists())
}

/// Creates or replaces the desktop shortcut for the current executable.
pub fn create_desktop_shortcut() -> Result<(), String> {
    create_shortcut(KnownShortcutFolder::Desktop)
}

/// Enables or disables startup through the current user's Startup folder.
pub fn set_launch_at_login(enabled: bool) -> Result<(), String> {
    if enabled {
        create_shortcut(KnownShortcutFolder::Startup)
    } else {
        remove_shortcut(KnownShortcutFolder::Startup)
    }
}

/// Returns true when the current user's Startup shortcut exists.
pub fn launch_at_login_enabled() -> bool {
    shortcut_path(KnownShortcutFolder::Startup).is_ok_and(|path| path.exists())
}

pub(crate) fn shortcut_file_name() -> &'static str {
    "Local Launcher.lnk"
}

#[derive(Clone, Copy)]
enum KnownShortcutFolder {
    Desktop,
    Startup,
}

struct ComApartment;

impl ComApartment {
    fn init() -> Result<Self, String> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED)
                .ok()
                .map_err(|error| format!("初始化快捷方式 COM 失败: {}", error.message()))?;
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

fn create_shortcut(folder: KnownShortcutFolder) -> Result<(), String> {
    let _apartment = ComApartment::init()?;
    let target = std::env::current_exe().map_err(|e| format!("读取程序路径失败: {e}"))?;
    let link_path = shortcut_path(folder)?;
    let working_dir = target
        .parent()
        .ok_or_else(|| "程序路径没有所在目录".to_string())?;

    unsafe {
        let link: IShellLinkW = SHCoCreateInstance(
            PCWSTR::null(),
            Some(&ShellLink),
            None::<windows::core::IUnknown>,
        )
        .map_err(|e| format!("创建 ShellLink 失败: {}", e.message()))?;

        let target_wide = wide_null(&target);
        let working_wide = wide_null(working_dir);
        let icon_wide = wide_null(APP_ICON_PATH);
        link.SetPath(PCWSTR(target_wide.as_ptr()))
            .map_err(|e| format!("设置快捷方式目标失败: {}", e.message()))?;
        link.SetWorkingDirectory(PCWSTR(working_wide.as_ptr()))
            .map_err(|e| format!("设置快捷方式工作目录失败: {}", e.message()))?;
        link.SetIconLocation(PCWSTR(icon_wide.as_ptr()), 0)
            .map_err(|e| format!("设置快捷方式图标失败: {}", e.message()))?;
        link.SetShowCmd(SW_SHOWNORMAL)
            .map_err(|e| format!("设置快捷方式窗口状态失败: {}", e.message()))?;

        let persist: IPersistFile = link
            .cast()
            .map_err(|e| format!("转换快捷方式保存接口失败: {}", e.message()))?;
        let link_wide = wide_null(&link_path);
        persist
            .Save(PCWSTR(link_wide.as_ptr()), true)
            .map_err(|e| format!("保存快捷方式失败: {}", e.message()))?;
    }

    Ok(())
}

fn remove_shortcut(folder: KnownShortcutFolder) -> Result<(), String> {
    let path = shortcut_path(folder)?;
    if !path.exists() {
        return Ok(());
    }
    fs::remove_file(&path).map_err(|e| format!("删除快捷方式失败: {e}"))
}

fn shortcut_path(folder: KnownShortcutFolder) -> Result<PathBuf, String> {
    let folder_path = known_folder_path(folder)?;
    Ok(folder_path.join(shortcut_file_name()))
}

fn known_folder_path(folder: KnownShortcutFolder) -> Result<PathBuf, String> {
    let id = match folder {
        KnownShortcutFolder::Desktop => &FOLDERID_Desktop,
        KnownShortcutFolder::Startup => &FOLDERID_Startup,
    };
    let path = unsafe { SHGetKnownFolderPath(id, Default::default(), None) }
        .map_err(|e| format!("读取 Windows 文件夹失败: {}", e.message()))?;
    let value = path.display().to_string();
    unsafe {
        windows::Win32::System::Com::CoTaskMemFree(Some(path.0 as _));
    }
    Ok(PathBuf::from(value))
}

fn wide_null(path: impl AsRef<Path>) -> Vec<u16> {
    path.as_ref()
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
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
```

- [ ] **Step 5: Run tests and build**

Run:

```powershell
cargo test windows_shortcut
cargo build
```

Expected: PASS and build succeeds.

- [ ] **Step 6: Commit**

```powershell
git add Cargo.toml Cargo.lock src/main.rs src/windows_shortcut.rs
git commit -m "feat(settings): 添加原生快捷方式操作"
```

---

### Task 3: Make Tray Settings Dynamic And Add One Global Hotkey

**Files:**
- Modify: `src/tray.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Add source-level regression tests**

Extend `src/tray.rs` tests:

```rust
#[test]
fn tray_exposes_settings_configuration() {
    let source = include_str!("tray.rs");
    let register_hotkey = concat!("Register", "HotKey");
    let unregister_hotkey = concat!("Unregister", "HotKey");

    assert!(source.contains("pub fn configure("));
    assert!(source.contains(register_hotkey));
    assert!(source.contains(unregister_hotkey));
    assert!(source.contains("WM_HOTKEY"));
    assert!(source.contains("close_to_tray"));
}
```

Extend `src/main.rs` tests:

```rust
#[test]
fn tray_receives_initial_settings() {
    let source = include_str!("main.rs");

    assert!(source.contains("initial_config.settings.clone()"));
    assert!(source.contains("tray::install(\"Local Launcher\","));
}
```

- [ ] **Step 2: Run tests and verify they fail**

Run:

```powershell
cargo test tray::tests::tray_exposes_settings_configuration tests::tray_receives_initial_settings
```

Expected: FAIL because tray settings are not wired.

- [ ] **Step 3: Add tray settings storage and public configure API**

In `src/tray.rs`, add imports:

```rust
use crate::model::LauncherSettings;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey,
    VK_SPACE,
};
```

Add constants and statics:

```rust
const TRAY_CONFIGURE: u32 = WM_APP + 2;
const HOTKEY_ID: i32 = 1;

static CLOSE_TO_TRAY: AtomicBool = AtomicBool::new(true);
static HOTKEY_REGISTERED: AtomicBool = AtomicBool::new(false);
static SHOW_ON_STARTUP: AtomicBool = AtomicBool::new(true);
static TRAY_HWND: AtomicIsize = AtomicIsize::new(0);
static SETTINGS: std::sync::Mutex<Option<LauncherSettings>> = std::sync::Mutex::new(None);
```

Change install signature and add `configure`:

```rust
/// Starts the tray icon integration for the launcher window.
pub fn install(window_title: &str, settings: LauncherSettings) {
    configure(settings.clone());
    let title = window_title.to_string();
    let _ = thread::Builder::new()
        .name("launcher-tray".to_string())
        .spawn(move || {
            if let Err(error) = run_tray(title, settings) {
                eprintln!("tray icon failed: {error}");
            }
        });
}

/// Applies runtime settings used by the tray window and global hotkey.
pub fn configure(settings: LauncherSettings) {
    CLOSE_TO_TRAY.store(settings.close_to_tray, Ordering::SeqCst);
    SHOW_ON_STARTUP.store(settings.show_window_on_startup, Ordering::SeqCst);
    if let Ok(mut current) = SETTINGS.lock() {
        *current = Some(settings);
    }
    let hwnd = TRAY_HWND.load(Ordering::SeqCst);
    if hwnd != 0 {
        unsafe {
            let _ = PostMessageW(Some(HWND(hwnd as _)), TRAY_CONFIGURE, WPARAM(0), LPARAM(0));
        }
    }
}
```

- [ ] **Step 4: Register hotkey in the tray message window**

Change `run_tray` signature:

```rust
fn run_tray(window_title: String, settings: LauncherSettings) -> Result<(), String> {
    let class_name = wide_null("LocalLauncherTrayWindow");
    register_tray_window_class(&class_name)?;
    let hwnd = create_tray_window(&class_name)?;
    TRAY_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
    add_tray_icon(hwnd)?;
    apply_hotkey_settings(hwnd, &settings)?;
    spawn_main_window_hook(window_title);
    run_message_loop();
    Ok(())
}
```

Add helper functions:

```rust
fn apply_pending_settings(hwnd: HWND) {
    let settings = SETTINGS.lock().ok().and_then(|settings| settings.clone());
    if let Some(settings) = settings {
        if let Err(error) = apply_hotkey_settings(hwnd, &settings) {
            eprintln!("hotkey settings failed: {error}");
        }
    }
}

fn apply_hotkey_settings(hwnd: HWND, settings: &LauncherSettings) -> Result<(), String> {
    unregister_hotkey(hwnd);
    if !settings.global_hotkey_enabled {
        return Ok(());
    }
    let (modifiers, key) = parse_hotkey(&settings.global_hotkey)?;
    unsafe {
        RegisterHotKey(Some(hwnd), HOTKEY_ID, modifiers | MOD_NOREPEAT, key)
            .map_err(|e| format!("注册全局快捷键失败: {}", e.message()))?;
    }
    HOTKEY_REGISTERED.store(true, Ordering::SeqCst);
    Ok(())
}

fn unregister_hotkey(hwnd: HWND) {
    if HOTKEY_REGISTERED.swap(false, Ordering::SeqCst) {
        unsafe {
            let _ = UnregisterHotKey(Some(hwnd), HOTKEY_ID);
        }
    }
}

fn parse_hotkey(value: &str) -> Result<(HOT_KEY_MODIFIERS, u32), String> {
    if value.trim().eq_ignore_ascii_case("Ctrl + Alt + Space") {
        return Ok((MOD_CONTROL | MOD_ALT, VK_SPACE.0 as u32));
    }
    Err(format!("暂只支持快捷键 Ctrl + Alt + Space: {value}"))
}
```

Update `tray_wnd_proc` match arms:

```rust
TRAY_CONFIGURE => {
    apply_pending_settings(hwnd);
    LRESULT(0)
}
WM_HOTKEY => {
    show_or_hide_main_window();
    LRESULT(0)
}
WM_DESTROY => {
    unregister_hotkey(hwnd);
    remove_tray_icon(hwnd);
    unsafe {
        PostQuitMessage(0);
    }
    LRESULT(0)
}
```

Add show/hide toggle:

```rust
fn show_or_hide_main_window() {
    if let Some(hwnd) = main_window() {
        unsafe {
            if IsWindowVisible(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_HIDE);
            } else {
                let _ = ShowWindow(hwnd, SW_RESTORE);
                let _ = SetForegroundWindow(hwnd);
            }
        }
    }
}
```

- [ ] **Step 5: Honor close-to-tray and startup visibility**

Update `main_wnd_proc` close handling:

```rust
if message == WM_CLOSE
    && CLOSE_TO_TRAY.load(Ordering::SeqCst)
    && !EXITING.load(Ordering::SeqCst)
{
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
    return LRESULT(0);
}
```

In `hook_main_window`, after `install_close_hook(hwnd);`, add:

```rust
if !SHOW_ON_STARTUP.load(Ordering::SeqCst) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
}
```

- [ ] **Step 6: Load initial settings before tray install**

In `src/main.rs`, change `main`:

```rust
fn main() -> windows_reactor::Result<()> {
    let initial_config = data_store::load_or_empty(std::path::Path::new("items.json"))
        .unwrap_or_else(|_| model::LauncherConfig::empty());
    tray::install("Local Launcher", initial_config.settings.clone());
    windows_reactor::App::new()
        .title("Local Launcher")
        .icon_path(APP_ICON_PATH)
        .inner_size(980.0, 680.0)
        .render(ui::app)
}
```

- [ ] **Step 7: Run tests and commit**

Run:

```powershell
cargo test tray main
cargo build
```

Expected: PASS and build succeeds.

Commit:

```powershell
git add src/tray.rs src/main.rs Cargo.toml Cargo.lock
git commit -m "feat(settings): 接入托盘和全局快捷键设置"
```

---

### Task 4: Add The In-App Settings View

**Files:**
- Modify: `src/ui.rs`

- [ ] **Step 1: Add UI regression tests**

Add to `src/ui.rs` tests:

```rust
#[test]
fn settings_mode_and_header_button_exist() {
    let source = include_str!("ui.rs");

    assert!(source.contains("Settings { section: SettingsSection }"));
    assert!(source.contains("button(\"⚙\")"));
    assert!(source.contains("fn settings_view("));
}

#[test]
fn settings_view_has_expected_sections() {
    let source = include_str!("ui.rs");

    assert!(source.contains("常规"));
    assert!(source.contains("快捷键"));
    assert!(source.contains("数据"));
    assert!(source.contains("关于"));
    assert!(source.contains("创建快捷方式"));
    assert!(source.contains("打开位置"));
}
```

- [ ] **Step 2: Run tests and verify they fail**

Run:

```powershell
cargo test ui::tests::settings_mode_and_header_button_exist ui::tests::settings_view_has_expected_sections
```

Expected: FAIL because settings UI does not exist.

- [ ] **Step 3: Add settings UI state types**

In `src/ui.rs`, update imports:

```rust
use crate::windows_shortcut;
```

Update model import:

```rust
use crate::model::{ItemKind, LauncherConfig, LauncherItem, LauncherSettings};
```

Extend `UiMode`:

```rust
enum UiMode {
    Browse,
    Edit { is_new: bool },
    Settings { section: SettingsSection },
}
```

Add:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SettingsSection {
    General,
    Hotkey,
    Data,
    About,
}
```

Extend `AsyncUiResult`:

```rust
SettingsUpdated {
    config: LauncherConfig,
    message: String,
},
```

Handle it in the existing mutation effect with the same body as `ConfigUpdated`.

- [ ] **Step 4: Put the gear button in the header**

Change `app_header` signature:

```rust
fn app_header(
    query: String,
    set_query: SetState<String>,
    set_draft: SetState<DraftItem>,
    set_mode: SetState<UiMode>,
) -> Element {
```

Add a fourth column button after `新增`:

```rust
button("⚙")
    .on_click(move || {
        set_mode.call(UiMode::Settings {
            section: SettingsSection::General,
        });
    })
    .grid_column(3),
```

Update columns:

```rust
.columns([
    GridLength::Pixel(260.0),
    GridLength::STAR,
    GridLength::Auto,
    GridLength::Auto,
])
```

- [ ] **Step 5: Route settings mode in `app`**

Add this match arm:

```rust
UiMode::Settings { section } => settings_view(
    config.clone(),
    config_path.clone(),
    set_message.clone(),
    async_state.clone(),
    async_trigger.clone(),
    set_mode.clone(),
    set_mode.clone(),
    section,
),
```

Clone `set_mode` before this match arm if the compiler reports a moved value.

- [ ] **Step 6: Add save helper functions**

Add near `update_draft`:

```rust
fn save_settings(
    mut config: LauncherConfig,
    config_path: PathBuf,
    next_settings: LauncherSettings,
) -> Result<AsyncUiResult, String> {
    config.settings = next_settings.clone();
    data_store::save(&config_path, &config)?;
    crate::tray::configure(next_settings);
    Ok(AsyncUiResult::SettingsUpdated {
        config,
        message: "设置已保存".to_string(),
    })
}

fn update_settings(
    config: LauncherConfig,
    config_path: PathBuf,
    async_trigger: MutationTrigger<AsyncUiResult>,
    update: impl FnOnce(&mut LauncherSettings) + Send + 'static,
) {
    async_trigger.fire(move || {
        let mut settings = config.settings.clone();
        update(&mut settings);
        save_settings(config, config_path, settings)
    });
}
```

- [ ] **Step 7: Implement settings view with four sections**

Add a `settings_view` function:

```rust
fn settings_view(
    config: LauncherConfig,
    config_path: PathBuf,
    set_message: SetState<String>,
    async_state: MutationState<AsyncUiResult>,
    async_trigger: MutationTrigger<AsyncUiResult>,
    set_mode: SetState<UiMode>,
    set_section_mode: SetState<UiMode>,
    section: SettingsSection,
) -> Element {
    let content = match section {
        SettingsSection::General => general_settings(
            config.clone(),
            config_path.clone(),
            set_message.clone(),
            async_trigger.clone(),
        ),
        SettingsSection::Hotkey => hotkey_settings(
            config.clone(),
            config_path.clone(),
            async_trigger.clone(),
        ),
        SettingsSection::Data => data_settings(config_path.clone(), set_message.clone()),
        SettingsSection::About => about_settings(),
    };

    border(
        grid((
            vstack((
                settings_nav_button("常规", SettingsSection::General, section, set_section_mode.clone()),
                settings_nav_button("快捷键", SettingsSection::Hotkey, section, set_section_mode.clone()),
                settings_nav_button("数据", SettingsSection::Data, section, set_section_mode.clone()),
                settings_nav_button("关于", SettingsSection::About, section, set_section_mode),
            ))
            .spacing(8.0)
            .grid_column(0),
            vstack((
                hstack((
                    text_block("设置").font_size(24.0).bold().width(620.0),
                    button("关闭").on_click(move || set_mode.call(UiMode::Browse)),
                ))
                .spacing(8.0),
                content,
            ))
            .spacing(16.0)
            .grid_column(1),
        ))
        .columns([GridLength::Pixel(130.0), GridLength::STAR])
        .column_spacing(18.0),
    )
    .background(ThemeRef::CardBackground)
    .border_brush(Color::rgb(205, 210, 216))
    .border_thickness(Thickness::uniform(1.0))
    .corner_radius(8.0)
    .padding(Thickness::uniform(16.0))
    .into()
}
```

- [ ] **Step 8: Add section helpers**

Add:

```rust
fn settings_nav_button(
    label: &str,
    target: SettingsSection,
    current: SettingsSection,
    set_mode: SetState<UiMode>,
) -> Element {
    let text = if current == target {
        format!("● {label}")
    } else {
        label.to_string()
    };
    button(text)
        .on_click(move || set_mode.call(UiMode::Settings { section: target }))
        .into()
}

fn general_settings(
    config: LauncherConfig,
    config_path: PathBuf,
    set_message: SetState<String>,
    async_trigger: MutationTrigger<AsyncUiResult>,
) -> Element {
    let settings = config.settings.clone();
    vstack((
        check_box(settings.launch_at_login)
            .content("开机自启")
            .on_checked({
                let config = config.clone();
                let config_path = config_path.clone();
                let async_trigger = async_trigger.clone();
                move |value| {
                    let config = config.clone();
                    let config_path = config_path.clone();
                    update_settings(config, config_path, async_trigger.clone(), move |settings| {
                        settings.launch_at_login = value;
                        let _ = windows_shortcut::set_launch_at_login(value);
                    });
                }
            }),
        check_box(settings.show_window_on_startup)
            .content("启动时显示主窗口")
            .on_checked({
                let config = config.clone();
                let config_path = config_path.clone();
                let async_trigger = async_trigger.clone();
                move |value| {
                    let config = config.clone();
                    let config_path = config_path.clone();
                    update_settings(config, config_path, async_trigger.clone(), move |settings| {
                        settings.show_window_on_startup = value;
                    });
                }
            }),
        check_box(settings.close_to_tray)
            .content("关闭窗口时最小化到托盘")
            .on_checked(move |value| {
                let config = config.clone();
                let config_path = config_path.clone();
                update_settings(config, config_path, async_trigger.clone(), move |settings| {
                    settings.close_to_tray = value;
                });
            }),
        button(if windows_shortcut::desktop_shortcut_exists() {
            "已存在"
        } else {
            "创建快捷方式"
        })
        .enabled(!windows_shortcut::desktop_shortcut_exists())
        .on_click(move || {
            match windows_shortcut::create_desktop_shortcut() {
                Ok(()) => set_message.call("已创建桌面快捷方式".to_string()),
                Err(error) => set_message.call(error),
            }
        }),
    ))
    .spacing(12.0)
    .into()
}
```

Add hotkey/data/about helpers:

```rust
fn hotkey_settings(
    config: LauncherConfig,
    config_path: PathBuf,
    async_trigger: MutationTrigger<AsyncUiResult>,
) -> Element {
    let settings = config.settings.clone();
    vstack((
        check_box(settings.global_hotkey_enabled)
            .content("启用全局唤起")
            .on_checked({
                let config = config.clone();
                let config_path = config_path.clone();
                let async_trigger = async_trigger.clone();
                move |value| {
                    let config = config.clone();
                    let config_path = config_path.clone();
                    update_settings(config, config_path, async_trigger.clone(), move |settings| {
                        settings.global_hotkey_enabled = value;
                    });
                }
            }),
        text_box(settings.global_hotkey)
            .header("快捷键")
            .on_text_changed(move |value| {
                let config = config.clone();
                let config_path = config_path.clone();
                update_settings(config, config_path, async_trigger.clone(), move |settings| {
                    settings.global_hotkey = value;
                });
            }),
        text_block("第一版支持 Ctrl + Alt + Space。"),
    ))
    .spacing(12.0)
    .into()
}

fn data_settings(config_path: PathBuf, set_message: SetState<String>) -> Element {
    hstack((
        text_block(config_path.display().to_string()).width(620.0),
        button("打开位置").on_click(move || {
            let item = LauncherItem {
                id: "config-folder".to_string(),
                name: "配置目录".to_string(),
                kind: ItemKind::Folder,
                target: config_path
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new("."))
                    .display()
                    .to_string(),
                arguments: String::new(),
                category: String::new(),
                tags: Vec::new(),
                username: String::new(),
                favorite: false,
                notes: String::new(),
                icon_format: None,
                icon_data: None,
            };
            if let Err(error) = launcher::open_item(&item) {
                set_message.call(error);
            }
        }),
    ))
    .spacing(8.0)
    .into()
}

fn about_settings() -> Element {
    vstack((
        text_block("Local Launcher").font_size(18.0).bold(),
        text_block(format!("版本 {}", env!("CARGO_PKG_VERSION"))),
        text_block("账号密码第一版不保存密码，只保存账号和备注。"),
    ))
    .spacing(8.0)
    .into()
}
```

- [ ] **Step 9: Run UI tests and commit**

Run:

```powershell
cargo test ui
cargo build
```

Expected: PASS and build succeeds.

Commit:

```powershell
git add src/ui.rs
git commit -m "feat(settings): 添加应用内设置页"
```

---

### Task 5: Final Verification And Manual Run

**Files:**
- No planned code changes unless verification finds a bug.

- [ ] **Step 1: Full automated verification**

Run:

```powershell
cargo test
cargo build
```

Expected: all tests PASS and build succeeds.

- [ ] **Step 2: Start the app**

Run:

```powershell
$existing = Get-Process -Name windows-reactor-launcher -ErrorAction SilentlyContinue
if ($existing) { $existing | Stop-Process }
Start-Process -FilePath ".\target\debug\windows-reactor-launcher.exe" -WorkingDirectory "."
```

Expected: no console window from the app itself; main window opens unless settings disable it.

- [ ] **Step 3: Manual smoke check**

Verify:

- Header has a gear button next to `新增`.
- Gear opens the settings view in the same window.
- `开机自启` toggles without a black command window.
- `创建快捷方式` creates `Local Launcher.lnk` on Desktop and then shows disabled/existing state after refresh.
- `关闭窗口时最小化到托盘` controls whether close hides or exits.
- `Ctrl + Alt + Space` shows/hides the window when enabled.
- Data `打开位置` opens the config folder.

- [ ] **Step 4: Commit any verification fixes**

If smoke testing finds a bug, make the smallest fix, rerun:

```powershell
cargo test
cargo build
```

Then commit:

```powershell
git add src/ui.rs src/tray.rs src/main.rs src/windows_shortcut.rs src/model.rs src/data_store.rs Cargo.toml Cargo.lock items.example.json
git commit -m "fix(settings): 修复设置页验证问题"
```

If no fixes are needed, do not create an empty commit.

---

## Self-Review

- Spec coverage: model defaults, header gear, in-app settings view, startup toggle, close-to-tray toggle, desktop shortcut button, global hotkey, data location, about text, native APIs, and tests are covered.
- Scope check: external themes, password storage, backups, registry startup, and multiple hotkeys are intentionally excluded.
- Placeholder scan: no task depends on an undefined future feature.
- Type consistency: `LauncherSettings`, `SettingsSection`, `UiMode::Settings`, and `AsyncUiResult::SettingsUpdated` names are used consistently.
