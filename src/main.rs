//! Windows Reactor launcher entry point.

#![windows_subsystem = "windows"]

mod credential;
mod data_store;
mod http;
mod icon_data;
mod launcher;
mod model;
mod picker;
mod search;
mod title_fetcher;
mod tray;
mod ui;
mod windows_shortcut;

pub(crate) const APP_ICON_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "\\assets\\app-icon.ico");

fn main() -> windows::core::Result<()> {
    let initial_config = data_store::load_or_empty(std::path::Path::new(ui::CONFIG_FILE))
        .unwrap_or_else(|_| model::LauncherConfig::empty());
    tray::install(ui::WINDOW_TITLE, initial_config.settings.clone());
    windows_reactor::App::run_component::<ui::Launcher>(initial_config)
}

#[cfg(test)]
mod tests {
    #[test]
    fn tray_is_installed_before_app_run() {
        let source = include_str!("main.rs");

        assert!(source.contains("mod tray;"));
        assert!(source.contains("tray::install(ui::WINDOW_TITLE,"));
    }

    #[test]
    fn tray_receives_initial_settings() {
        let source = include_str!("main.rs");

        assert!(source.contains("initial_config.settings.clone()"));
        assert!(source.contains("tray::install(ui::WINDOW_TITLE,"));
    }

    #[test]
    fn app_icon_assets_are_wired() {
        assert!(std::path::Path::new("assets/app-icon.ico").exists());
        assert!(std::path::Path::new("assets/app.rc").exists());
    }

    #[test]
    fn launcher_component_receives_initial_config() {
        let source = include_str!("main.rs");

        assert!(source.contains("App::run_component::<ui::Launcher>(initial_config)"));
    }
}
