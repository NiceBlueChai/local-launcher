//! Provides the Windows notification-area icon for the launcher.

use crate::model::LauncherSettings;
use std::mem::size_of;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::thread;
use std::time::Duration;
use windows::core::{BOOL, PCWSTR};
use windows::Win32::combaseapi::{CoCreateInstance, CoInitializeEx};
use windows::Win32::libloaderapi::GetModuleHandleW;
use windows::Win32::minwindef::{LPARAM, WPARAM};
use windows::Win32::oaidl::VARIANT;
use windows::Win32::objbase::COINIT_APARTMENTTHREADED;
use windows::Win32::shellapi::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::uiautomationclient::{CUIAutomation, IUIAutomation, TreeScope_Descendants};
use windows::Win32::windef::{HICON, HWND, POINT};
use windows::Win32::winnt::HANDLE;
use windows::Win32::winuser::{
    AppendMenuW, CallWindowProcW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
    DestroyWindow, DispatchMessageW, EnumWindows, GCLP_HICON, GCLP_HICONSM, GWLP_WNDPROC,
    GetCursorPos, GetMessageW, GetWindowThreadProcessId, ICON_BIG, ICON_SMALL, IDI_APPLICATION,
    IMAGE_ICON, IsWindowVisible, LR_SHARED, LoadIconW, LoadImageW, MF_SEPARATOR, MF_STRING, MSG,
    MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, PostMessageW, PostQuitMessage, RegisterClassW,
    RegisterHotKey, SendMessageW, SetClassLongPtrW, SetForegroundWindow, SetWindowLongPtrW,
    ShowWindow, SW_HIDE, SW_RESTORE, TPM_RIGHTBUTTON, TrackPopupMenu, TranslateMessage,
    UnregisterHotKey, VK_SPACE, WM_APP, WM_CLOSE, WM_COMMAND, WM_DESTROY, WM_HOTKEY,
    WM_LBUTTONDBLCLK, WM_RBUTTONUP, WM_SETICON, WNDCLASSW,
};
use windows::Win32::wtypes::VT_I4;
use windows::Win32::wtypesbase::CLSCTX_INPROC_SERVER;

/// windows 0.100 no longer generates the UI Automation property and control-type identifiers.
const UIA_CONTROL_TYPE_PROPERTY_ID: i32 = 30003;
const UIA_EDIT_CONTROL_TYPE_ID: i32 = 50004;

/// `IDI_APPLICATION` is published as `PCSTR`, but resource identifiers are plain ordinals.
const IDI_APPLICATION_W: PCWSTR = PCWSTR::from_raw(IDI_APPLICATION.0 as *const u16);

const TRAY_ID: u32 = 1;
const APP_ICON_RESOURCE_ID: usize = 1;
const TRAY_CALLBACK: i32 = WM_APP + 1;
const TRAY_CONFIGURE: i32 = WM_APP + 2;
const HOTKEY_ID: i32 = 1;
const MENU_SHOW: usize = 1001;
const MENU_EXIT: usize = 1002;
const HOOK_ATTEMPTS: usize = 120;
const HOOK_DELAY: Duration = Duration::from_millis(100);

static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);
static ORIGINAL_WNDPROC: AtomicIsize = AtomicIsize::new(0);
static EXITING: AtomicBool = AtomicBool::new(false);
static CLOSE_TO_TRAY: AtomicBool = AtomicBool::new(true);
static HOTKEY_REGISTERED: AtomicBool = AtomicBool::new(false);
static SHOW_ON_STARTUP: AtomicBool = AtomicBool::new(true);
static TRAY_HWND: AtomicIsize = AtomicIsize::new(0);
static SETTINGS: std::sync::Mutex<Option<LauncherSettings>> = std::sync::Mutex::new(None);
static CONFIGURE_ERROR: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Starts the tray icon integration for the launcher window.
pub fn install(window_title: &str, settings: LauncherSettings) {
    if let Err(error) = configure(settings.clone()) {
        eprintln!("tray settings failed: {error}");
    }
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
pub fn configure(settings: LauncherSettings) -> Result<(), String> {
    let previous = current_settings();
    let hwnd = TRAY_HWND.load(Ordering::SeqCst);
    store_settings(settings);
    if hwnd != 0 {
        let hwnd = hwnd as HWND;
        clear_configure_error();
        unsafe {
            let _ = SendMessageW(hwnd, TRAY_CONFIGURE as u32, 0, 0);
        }
        if let Some(error) = take_configure_error() {
            if let Some(previous) = previous {
                store_settings(previous);
                clear_configure_error();
                unsafe {
                    let _ = SendMessageW(hwnd, TRAY_CONFIGURE as u32, 0, 0);
                }
            }
            return Err(error);
        }
    }

    Ok(())
}

fn current_settings() -> Option<LauncherSettings> {
    SETTINGS.lock().ok().and_then(|settings| settings.clone())
}

fn store_settings(settings: LauncherSettings) {
    if let Ok(mut current) = SETTINGS.lock() {
        *current = Some(settings.clone());
    }
    CLOSE_TO_TRAY.store(settings.close_to_tray, Ordering::SeqCst);
    SHOW_ON_STARTUP.store(settings.show_window_on_startup, Ordering::SeqCst);
}

fn clear_configure_error() {
    if let Ok(mut error) = CONFIGURE_ERROR.lock() {
        *error = None;
    }
}

fn set_configure_error(value: String) {
    if let Ok(mut error) = CONFIGURE_ERROR.lock() {
        *error = Some(value);
    }
}

fn take_configure_error() -> Option<String> {
    CONFIGURE_ERROR
        .lock()
        .ok()
        .and_then(|mut error| error.take())
}

fn run_tray(window_title: String, settings: LauncherSettings) -> Result<(), String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED as u32);
    }
    let class_name = wide_null("LocalLauncherTrayWindow");
    register_tray_window_class(&class_name)?;
    let hwnd = create_tray_window(&class_name)?;
    TRAY_HWND.store(hwnd as isize, Ordering::SeqCst);
    add_tray_icon(hwnd)?;
    if let Err(error) = apply_hotkey_settings(hwnd, &settings) {
        eprintln!("hotkey settings failed: {error}");
    }
    spawn_main_window_hook(window_title);
    run_message_loop();
    Ok(())
}

fn register_tray_window_class(class_name: &[u16]) -> Result<(), String> {
    let class = WNDCLASSW {
        lpfnWndProc: Some(tray_wnd_proc),
        lpszClassName: PCWSTR(class_name.as_ptr()),
        ..Default::default()
    };
    let atom = unsafe { RegisterClassW(&raw const class) };
    if atom == 0 {
        Err("注册托盘消息窗口失败".to_string())
    } else {
        Ok(())
    }
}

fn create_tray_window(class_name: &[u16]) -> Result<HWND, String> {
    let title = wide_null("Local Launcher Tray");
    let hwnd = unsafe {
        CreateWindowExW(
            0,
            PCWSTR(class_name.as_ptr()),
            PCWSTR(title.as_ptr()),
            0,
            0,
            0,
            0,
            0,
            None,
            None,
            None,
            None,
        )
    };

    if hwnd.is_null() {
        Err("创建托盘消息窗口失败".to_string())
    } else {
        Ok(hwnd)
    }
}

fn add_tray_icon(hwnd: HWND) -> Result<(), String> {
    let icon = load_app_icon()?;
    let mut data = notify_icon_data(hwnd);
    data.uFlags = (NIF_MESSAGE | NIF_ICON | NIF_TIP) as u32;
    data.uCallbackMessage = TRAY_CALLBACK as u32;
    data.hIcon = icon;
    copy_wide(&mut data.szTip, "Local Launcher");

    if unsafe { Shell_NotifyIconW(NIM_ADD as u32, &raw mut data).as_bool() } {
        Ok(())
    } else {
        Err("添加托盘图标失败".to_string())
    }
}

fn load_app_icon() -> Result<HICON, String> {
    let resource = PCWSTR(APP_ICON_RESOURCE_ID as _);
    let module = unsafe { GetModuleHandleW(PCWSTR::null()) };
    if !module.is_null() {
        let icon = unsafe { LoadIconW(Some(module.cast()), resource) };
        if !icon.is_null() {
            return Ok(icon);
        }
    }

    let fallback = unsafe { LoadIconW(None, IDI_APPLICATION_W) };
    if fallback.is_null() {
        return Err("加载托盘图标失败".to_string());
    }

    Ok(fallback)
}

fn remove_tray_icon(hwnd: HWND) {
    let mut data = notify_icon_data(hwnd);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE as u32, &raw mut data);
    }
}

fn notify_icon_data(hwnd: HWND) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ID,
        ..Default::default()
    }
}

fn spawn_main_window_hook(_window_title: String) {
    let _ = thread::Builder::new()
        .name("launcher-window-hook".to_string())
        .spawn(move || {
            hook_main_window();
        });
}

fn hook_main_window() {
    for _ in 0..HOOK_ATTEMPTS {
        if let Some(hwnd) = find_main_window() {
            MAIN_HWND.store(hwnd as isize, Ordering::SeqCst);
            set_main_window_icons(hwnd);
            install_close_hook(hwnd);
            if !SHOW_ON_STARTUP.load(Ordering::SeqCst) {
                unsafe {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
            }
            return;
        }
        thread::sleep(HOOK_DELAY);
    }
}

fn find_main_window() -> Option<HWND> {
    let mut state = FindMainWindowState {
        process_id: std::process::id(),
        hwnd: HWND::default(),
    };
    let state_ptr = &mut state as *mut FindMainWindowState;
    let _ = unsafe { EnumWindows(Some(enum_main_window), state_ptr as LPARAM) };

    if state.hwnd.is_null() {
        None
    } else {
        Some(state.hwnd)
    }
}

struct FindMainWindowState {
    process_id: u32,
    hwnd: HWND,
}

unsafe extern "system" fn enum_main_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let state = unsafe { &mut *(lparam as *mut FindMainWindowState) };
    let mut process_id = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&raw mut process_id));
    }
    if process_id == state.process_id && unsafe { IsWindowVisible(hwnd).as_bool() } {
        state.hwnd = hwnd;
        return BOOL(0);
    }

    BOOL(1)
}

fn install_close_hook(hwnd: HWND) {
    let wnd_proc = main_wnd_proc as *const () as isize;
    let previous = unsafe { SetWindowLongPtrW(hwnd, GWLP_WNDPROC, wnd_proc) };
    if previous != 0 {
        ORIGINAL_WNDPROC.store(previous, Ordering::SeqCst);
    }
}

fn set_main_window_icons(hwnd: HWND) {
    if let Some(icon) = load_sized_app_icon(32, 32) {
        unsafe {
            let _ = SendMessageW(hwnd, WM_SETICON as u32, ICON_BIG as WPARAM, icon as LPARAM);
            let _ = SetClassLongPtrW(hwnd, GCLP_HICON, icon as isize);
        }
    }
    if let Some(icon) = load_sized_app_icon(16, 16) {
        unsafe {
            let _ = SendMessageW(hwnd, WM_SETICON as u32, ICON_SMALL as WPARAM, icon as LPARAM);
            let _ = SetClassLongPtrW(hwnd, GCLP_HICONSM, icon as isize);
        }
    }
}

fn load_sized_app_icon(width: i32, height: i32) -> Option<HICON> {
    let module = unsafe { GetModuleHandleW(PCWSTR::null()) };
    if module.is_null() {
        return None;
    }
    let resource = PCWSTR(APP_ICON_RESOURCE_ID as _);
    let handle: HANDLE = unsafe {
        LoadImageW(
            Some(module.cast()),
            resource,
            IMAGE_ICON as u32,
            width,
            height,
            LR_SHARED as u32,
        )
    };

    if handle.is_null() {
        None
    } else {
        Some(handle as HICON)
    }
}

fn run_message_loop() {
    let mut message = MSG::default();
    while unsafe { GetMessageW(&raw mut message, None, 0, 0).as_bool() } {
        unsafe {
            let _ = TranslateMessage(&raw const message);
            let _ = DispatchMessageW(&raw const message);
        }
    }
}

unsafe extern "system" fn tray_wnd_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> isize {
    // windows 0.100 publishes the message identifiers as `i32` while window procedures receive `u32`.
    match message {
        value if value == TRAY_CALLBACK as u32 => {
            handle_tray_event(hwnd, lparam);
            0
        }
        value if value == WM_COMMAND as u32 => {
            handle_menu_command(hwnd, wparam & 0xffff);
            0
        }
        value if value == TRAY_CONFIGURE as u32 => {
            apply_pending_settings(hwnd);
            0
        }
        value if value == WM_HOTKEY as u32 => {
            show_or_hide_main_window();
            0
        }
        value if value == WM_DESTROY as u32 => {
            unregister_hotkey(hwnd);
            remove_tray_icon(hwnd);
            unsafe {
                PostQuitMessage(0);
            }
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

unsafe extern "system" fn main_wnd_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> isize {
    if message == WM_CLOSE as u32
        && CLOSE_TO_TRAY.load(Ordering::SeqCst)
        && !EXITING.load(Ordering::SeqCst)
    {
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
        return 0;
    }

    call_original_window_proc(hwnd, message, wparam, lparam)
}

fn handle_tray_event(hwnd: HWND, event: LPARAM) {
    match event as i32 {
        WM_LBUTTONDBLCLK => show_main_window(),
        WM_RBUTTONUP => show_tray_menu(hwnd),
        _ => {}
    }
}

fn handle_menu_command(hwnd: HWND, command: usize) {
    match command {
        MENU_SHOW => show_main_window(),
        MENU_EXIT => exit_app(hwnd),
        _ => {}
    }
}

fn show_tray_menu(hwnd: HWND) {
    let menu = unsafe { CreatePopupMenu() };
    if menu.is_null() {
        return;
    }
    let show = wide_null("显示窗口");
    let exit = wide_null("退出");
    let mut point = POINT::default();
    unsafe {
        let _ = AppendMenuW(menu, MF_STRING as u32, MENU_SHOW, PCWSTR(show.as_ptr()));
        let _ = AppendMenuW(menu, MF_SEPARATOR as u32, 0, PCWSTR::null());
        let _ = AppendMenuW(menu, MF_STRING as u32, MENU_EXIT, PCWSTR(exit.as_ptr()));
        if GetCursorPos(&raw mut point).as_bool() {
            let _ = SetForegroundWindow(hwnd);
            let _ = TrackPopupMenu(
                menu,
                TPM_RIGHTBUTTON as u32,
                point.x,
                point.y,
                None,
                hwnd,
                None,
            );
        }
        let _ = DestroyMenu(menu);
    }
}

fn show_main_window() {
    if let Some(hwnd) = main_window() {
        unsafe {
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(hwnd);
        }
        focus_search_box(hwnd);
    }
}

fn show_or_hide_main_window() {
    if let Some(hwnd) = main_window() {
        unsafe {
            if IsWindowVisible(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_HIDE);
            } else {
                let _ = ShowWindow(hwnd, SW_RESTORE);
                let _ = SetForegroundWindow(hwnd);
                focus_search_box(hwnd);
            }
        }
    }
}

fn focus_search_box(hwnd: HWND) {
    if let Err(error) = focus_first_edit_control(hwnd) {
        eprintln!("focus search box failed: {error}");
    }
}

fn focus_first_edit_control(hwnd: HWND) -> Result<(), String> {
    let automation: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }.map_err(describe)?;
    let root = unsafe { automation.ElementFromHandle(hwnd.cast()) }.map_err(describe)?;
    let edit_type = edit_type_variant();
    let edit_condition = unsafe {
        automation.CreatePropertyCondition(UIA_CONTROL_TYPE_PROPERTY_ID, &edit_type)
    }
    .map_err(describe)?;
    let search_box =
        unsafe { root.FindFirst(TreeScope_Descendants, &edit_condition) }.map_err(describe)?;

    unsafe { search_box.SetFocus() }.ok().map_err(describe)
}

fn edit_type_variant() -> VARIANT {
    let mut value = VARIANT::default();
    unsafe {
        let typed = &mut *value.Anonymous.Anonymous;
        typed.vt = VT_I4 as u16;
        typed.Anonymous.lVal = UIA_EDIT_CONTROL_TYPE_ID;
    }

    value
}

fn apply_pending_settings(hwnd: HWND) {
    let settings = current_settings();
    if let Some(settings) = settings {
        if let Err(error) = apply_hotkey_settings(hwnd, &settings) {
            set_configure_error(error.clone());
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
        RegisterHotKey(Some(hwnd), HOTKEY_ID, modifiers | MOD_NOREPEAT as u32, key)
            .ok()
            .map_err(|error| format!("注册全局快捷键失败: {}", error.message()))?;
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

fn parse_hotkey(value: &str) -> Result<(u32, u32), String> {
    if value.trim().eq_ignore_ascii_case("Ctrl + Alt + Space") {
        return Ok(((MOD_CONTROL | MOD_ALT) as u32, virtual_key(VK_SPACE)));
    }
    Err(format!("暂只支持快捷键 Ctrl + Alt + Space: {value}"))
}

const fn virtual_key(value: i32) -> u32 {
    value as u32
}

fn exit_app(tray_hwnd: HWND) {
    EXITING.store(true, Ordering::SeqCst);
    if let Some(hwnd) = main_window() {
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_CLOSE as u32, 0, 0);
        }
    } else {
        std::process::exit(0);
    }
    unsafe {
        let _ = DestroyWindow(tray_hwnd);
    }
}

fn main_window() -> Option<HWND> {
    let value = MAIN_HWND.load(Ordering::SeqCst);
    if value == 0 {
        None
    } else {
        Some(value as HWND)
    }
}

fn call_original_window_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> isize {
    let previous = ORIGINAL_WNDPROC.load(Ordering::SeqCst);
    if previous == 0 {
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }

    unsafe {
        let proc: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> isize =
            std::mem::transmute(previous);
        CallWindowProcW(Some(proc), hwnd, message, wparam, lparam)
    }
}

fn describe(error: impl core::fmt::Debug) -> String {
    format!("{error:?}")
}

fn copy_wide(target: &mut [u16], value: &str) {
    for (slot, code_unit) in target.iter_mut().zip(value.encode_utf16()) {
        *slot = code_unit;
    }
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_uses_native_notify_icon_api() {
        let source = include_str!("tray.rs");
        let notify_icon = concat!("Shell_", "NotifyIconW");
        let powershell = concat!("powershell", ".exe");
        let find_window = concat!("Find", "WindowW");
        let set_icon_message = concat!("WM_", "SETICON");

        assert!(source.contains(notify_icon));
        assert!(source.contains("APP_ICON_RESOURCE_ID"));
        assert!(source.contains("GetModuleHandleW"));
        assert!(source.contains("EnumWindows"));
        assert!(source.contains(set_icon_message));
        assert!(!source.contains(find_window));
        assert!(source.contains("WM_CLOSE"));
        assert!(source.contains("SW_HIDE"));
        assert!(!source.contains(powershell));
    }

    #[test]
    fn tray_exposes_settings_configuration() {
        let source = include_str!("tray.rs");
        let register_hotkey = concat!("Register", "HotKey");
        let unregister_hotkey = concat!("Unregister", "HotKey");

        assert!(
            source.contains("pub fn configure(settings: LauncherSettings) -> Result<(), String>")
        );
        assert!(source.contains(register_hotkey));
        assert!(source.contains(unregister_hotkey));
        assert!(source.contains("WM_HOTKEY"));
        assert!(source.contains("close_to_tray"));
    }

    #[test]
    fn configure_reports_runtime_hotkey_errors() {
        let source = include_str!("tray.rs");
        let configure = source
            .split("pub fn configure(settings: LauncherSettings) -> Result<(), String>")
            .nth(1)
            .and_then(|tail| tail.split("fn current_settings()").next())
            .unwrap_or_default();

        assert!(configure.contains("SendMessageW(hwnd, TRAY_CONFIGURE"));
        assert!(!configure.contains("apply_hotkey_settings(hwnd, &settings)"));
        assert!(source.contains("take_configure_error()"));
        assert!(source.contains("return Err(error);"));
    }

    #[test]
    fn showing_main_window_focuses_search_box() {
        let source = include_str!("tray.rs");
        let show_main = source
            .split("fn show_main_window()")
            .nth(1)
            .and_then(|tail| tail.split("fn show_or_hide_main_window()").next())
            .unwrap_or_default();
        let show_or_hide = source
            .split("fn show_or_hide_main_window()")
            .nth(1)
            .and_then(|tail| tail.split("fn apply_pending_settings(").next())
            .unwrap_or_default();

        assert!(show_main.contains("focus_search_box(hwnd)"));
        assert!(show_or_hide.contains("focus_search_box(hwnd)"));
        assert!(source.contains("CUIAutomation"));
        assert!(source.contains("UIA_EDIT_CONTROL_TYPE_ID"));
    }

    #[test]
    fn parse_hotkey_accepts_ctrl_alt_space_only() {
        let (modifiers, key) = parse_hotkey("  cTrL + aLt + sPaCe  ").unwrap();

        assert_eq!(modifiers, (MOD_CONTROL | MOD_ALT) as u32);
        assert_eq!(key, virtual_key(VK_SPACE));
        assert!(parse_hotkey("Ctrl + Shift + Space").is_err());
    }
}
