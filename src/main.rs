//! Windows Reactor launcher entry point.

#![windows_subsystem = "windows"]

mod credential;
mod data_store;
mod icon_data;
mod launcher;
mod model;
mod picker;
mod search;
mod title_fetcher;
mod tray;
mod ui;
mod windows_shortcut;

const APP_ICON_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "\\assets\\app-icon.ico");

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

#[cfg(test)]
mod tests {
    use std::path::Path;

    #[test]
    fn tray_is_installed_before_app_run() {
        let source = include_str!("main.rs");

        assert!(source.contains("mod tray;"));
        assert!(source.contains("tray::install(\"Local Launcher\","));
    }

    #[test]
    fn tray_receives_initial_settings() {
        let source = include_str!("main.rs");

        assert!(source.contains("initial_config.settings.clone()"));
        assert!(source.contains("tray::install(\"Local Launcher\","));
    }

    #[test]
    fn app_icon_assets_are_wired() {
        assert!(Path::new("assets/app-icon.ico").exists());
        assert!(Path::new("assets/app.rc").exists());
    }

    #[test]
    fn winui_app_receives_icon_path() {
        let source = include_str!("main.rs");
        let icon_path_call = concat!(".icon", "_path(APP_ICON_PATH)");

        assert!(source.contains("APP_ICON_PATH"));
        assert!(source.contains(icon_path_call));
    }
}
