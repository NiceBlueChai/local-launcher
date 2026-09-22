//! Windows Reactor UI for the local launcher.

use crate::APP_ICON_PATH;
use crate::credential;
use crate::data_store;
use crate::icon_data;
use crate::launcher;
use crate::model::{ItemKind, LauncherConfig, LauncherItem, LauncherSettings};
use crate::picker;
use crate::search::{self, CategoryFilter};
use crate::title_fetcher;
use crate::windows_shortcut;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use windows_reactor::*;

pub const WINDOW_TITLE: &str = "Local Launcher";
pub const CONFIG_FILE: &str = "items.json";
const WINDOW_WIDTH: f64 = 980.0;
const WINDOW_HEIGHT: f64 = 680.0;

#[derive(Clone, Debug, Eq, PartialEq)]
enum UiMode {
    Browse,
    Edit { is_new: bool },
    Settings { section: SettingsSection },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsSection {
    General,
    Hotkey,
    Data,
    About,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AsyncUiResult {
    DraftUpdated {
        draft: DraftItem,
        message: String,
    },
    ConfigUpdated {
        config: LauncherConfig,
        message: String,
    },
    SettingsUpdated {
        config: LauncherConfig,
        message: String,
    },
    Message(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingKind {
    LaunchAtLogin,
    ShowWindowOnStartup,
    CloseToTray,
    GlobalHotkey,
}

impl SettingKind {
    fn label(self) -> &'static str {
        match self {
            Self::LaunchAtLogin => "开机自启",
            Self::ShowWindowOnStartup => "启动时显示主窗口",
            Self::CloseToTray => "关闭窗口时最小化到托盘",
            Self::GlobalHotkey => "启用全局唤起",
        }
    }

    fn message(self) -> &'static str {
        match self {
            Self::LaunchAtLogin => "已更新开机自启",
            Self::ShowWindowOnStartup => "已更新启动显示设置",
            Self::CloseToTray => "已更新关闭行为",
            Self::GlobalHotkey => "已更新全局唤起设置",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftField {
    Name,
    Target,
    Category,
    Tags,
    Username,
    Notes,
    Arguments,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Msg {
    Query(String),
    Filter(CategoryFilter),
    NewEntry,
    EditItem(LauncherItem),
    OpenItem(LauncherItem),
    OpenItemFolder(LauncherItem),
    Browse,
    OpenSettings(SettingsSection),
    Draft(DraftField, String),
    DraftKind(Option<usize>),
    DraftFavorite(bool),
    SaveDraft,
    DeleteDraft,
    PickFolder,
    PickProgram,
    FetchTitle,
    ToggleSetting(SettingKind, bool),
    CreateShortcut,
    OpenConfigFolder,
    Import,
    Export,
    Completed(AsyncUiResult),
    Failed(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftItem {
    id: String,
    name: String,
    kind: ItemKind,
    target: String,
    arguments: String,
    category: String,
    tags: String,
    username: String,
    favorite: bool,
    notes: String,
    icon_format: Option<String>,
    icon_data: Option<String>,
}

impl DraftItem {
    fn sample() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            kind: ItemKind::Url,
            target: String::new(),
            arguments: String::new(),
            category: "未分类".to_string(),
            tags: String::new(),
            username: String::new(),
            favorite: false,
            notes: String::new(),
            icon_format: None,
            icon_data: None,
        }
    }

    fn into_item(self) -> LauncherItem {
        LauncherItem {
            id: if self.id.is_empty() {
                new_id()
            } else {
                self.id
            },
            name: self.name,
            kind: self.kind,
            target: self.target,
            arguments: self.arguments,
            category: self.category,
            tags: self
                .tags
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect(),
            username: self.username,
            favorite: self.favorite,
            notes: self.notes,
            icon_format: self.icon_format,
            icon_data: self.icon_data,
        }
    }
}

impl From<LauncherItem> for DraftItem {
    fn from(item: LauncherItem) -> Self {
        Self {
            id: item.id,
            name: item.name,
            kind: item.kind,
            target: item.target,
            arguments: item.arguments,
            category: item.category,
            tags: item.tags.join(", "),
            username: item.username,
            favorite: item.favorite,
            notes: item.notes,
            icon_format: item.icon_format,
            icon_data: item.icon_data,
        }
    }
}

/// Launcher root component: owns configuration, filters, drafts, and async work.
pub struct Launcher {
    config: LauncherConfig,
    config_path: PathBuf,
    query: String,
    filter: CategoryFilter,
    message: String,
    draft: DraftItem,
    mode: UiMode,
    busy: bool,
}

impl Component for Launcher {
    type Input = LauncherConfig;
    type Message = Msg;

    fn create(input: &Self::Input, _context: &ComponentContext<Self>) -> Self {
        Self {
            config: input.clone(),
            config_path: PathBuf::from(CONFIG_FILE),
            query: String::new(),
            filter: CategoryFilter::All,
            message: String::new(),
            draft: DraftItem::sample(),
            mode: UiMode::Browse,
            busy: false,
        }
    }

    fn update(&mut self, message: Msg, context: &ComponentContext<Self>) {
        match message {
            Msg::Query(value) => self.query = value,
            Msg::Filter(value) => self.filter = value,
            Msg::NewEntry => {
                self.draft = DraftItem::sample();
                self.mode = UiMode::Edit { is_new: true };
            }
            Msg::EditItem(item) => {
                self.draft = DraftItem::from(item);
                self.mode = UiMode::Edit { is_new: false };
            }
            Msg::OpenItem(item) => {
                if let Err(error) = launcher::open_item(&item) {
                    self.message = error;
                }
            }
            Msg::OpenItemFolder(item) => {
                if let Err(error) = launcher::open_containing_folder(&item) {
                    self.message = error;
                }
            }
            Msg::Browse => self.mode = UiMode::Browse,
            Msg::OpenSettings(section) => self.mode = UiMode::Settings { section },
            Msg::Draft(field, value) => apply_draft_field(&mut self.draft, field, value),
            Msg::DraftKind(index) => {
                self.draft.kind = kind_from_index(index);
                self.draft.icon_format = None;
                self.draft.icon_data = None;
            }
            Msg::DraftFavorite(value) => self.draft.favorite = value,
            Msg::SaveDraft => self.save_draft(),
            Msg::DeleteDraft => self.delete_draft(),
            Msg::PickFolder => {
                self.busy = true;
                let current = self.draft.clone();
                _ = context.spawn_background(move |_cancel| match picker::pick_folder() {
                    Ok(Some(path)) => {
                        let mut next = current;
                        next.target = path;
                        next.icon_format = None;
                        next.icon_data = None;
                        Msg::Completed(AsyncUiResult::DraftUpdated {
                            draft: next,
                            message: "已选择目录".to_string(),
                        })
                    }
                    Ok(None) => Msg::Completed(AsyncUiResult::Message("已取消选择目录".to_string())),
                    Err(error) => Msg::Failed(error),
                });
            }
            Msg::PickProgram => {
                self.busy = true;
                let current = self.draft.clone();
                _ = context.spawn_background(move |_cancel| match picker::pick_program() {
                    Ok(Some(path)) => {
                        let mut next = current;
                        next.target = path.clone();
                        if let Ok(icon) = icon_data::extract_program_icon(&path) {
                            next.icon_format = Some(icon.format);
                            next.icon_data = Some(icon.data);
                        } else {
                            next.icon_format = None;
                            next.icon_data = None;
                        }
                        Msg::Completed(AsyncUiResult::DraftUpdated {
                            draft: next,
                            message: "已选择程序".to_string(),
                        })
                    }
                    Ok(None) => Msg::Completed(AsyncUiResult::Message("已取消选择程序".to_string())),
                    Err(error) => Msg::Failed(error),
                });
            }
            Msg::FetchTitle => {
                self.busy = true;
                let current = self.draft.clone();
                _ = context.spawn_background(move |_cancel| {
                    match title_fetcher::fetch_title(&current.target) {
                        Ok(title) => {
                            let mut next = current;
                            next.name = title;
                            if let Ok(icon) = icon_data::fetch_url_icon(&next.target) {
                                next.icon_format = Some(icon.format);
                                next.icon_data = Some(icon.data);
                            }
                            Msg::Completed(AsyncUiResult::DraftUpdated {
                                draft: next,
                                message: "已获取网页标题".to_string(),
                            })
                        }
                        Err(error) => Msg::Failed(error),
                    }
                });
            }
            Msg::ToggleSetting(kind, value) => {
                self.busy = true;
                let config = self.config.clone();
                let config_path = self.config_path.clone();
                _ = context.spawn_background(move |_cancel| {
                    match update_settings(config, &config_path, value, kind) {
                        Ok(result) => Msg::Completed(result),
                        Err(error) => Msg::Failed(error),
                    }
                });
            }
            Msg::CreateShortcut => {
                self.message = match windows_shortcut::create_desktop_shortcut() {
                    Ok(()) => "已创建桌面快捷方式".to_string(),
                    Err(error) => error,
                };
            }
            Msg::OpenConfigFolder => self.open_config_folder(),
            Msg::Import => {
                self.busy = true;
                let current = self.config.clone();
                let config_path = self.config_path.clone();
                _ = context.spawn_background(move |_cancel| {
                    let import = || -> std::result::Result<AsyncUiResult, String> {
                        match picker::pick_json_file()? {
                            Some(path) => {
                                let imported = data_store::import_from(&PathBuf::from(&path))?;
                                let merged = data_store::merge_config(current, imported)?;
                                data_store::save(&config_path, &merged)?;
                                Ok(AsyncUiResult::ConfigUpdated {
                                    config: merged,
                                    message: format!("已合并导入 {path}"),
                                })
                            }
                            None => Ok(AsyncUiResult::Message("已取消导入".to_string())),
                        }
                    };

                    match import() {
                        Ok(result) => Msg::Completed(result),
                        Err(error) => Msg::Failed(error),
                    }
                });
            }
            Msg::Export => {
                self.message = match data_store::export_to(
                    &PathBuf::from("launcher-export.json"),
                    &self.config,
                ) {
                    Ok(()) => "已导出 launcher-export.json".to_string(),
                    Err(error) => error,
                };
            }
            Msg::Completed(result) => {
                self.busy = false;
                match result {
                    AsyncUiResult::DraftUpdated { draft, message } => {
                        self.draft = draft;
                        self.message = message;
                    }
                    AsyncUiResult::ConfigUpdated { config, message }
                    | AsyncUiResult::SettingsUpdated { config, message } => {
                        self.config = config;
                        self.message = message;
                    }
                    AsyncUiResult::Message(message) => self.message = message,
                }
            }
            Msg::Failed(error) => {
                self.busy = false;
                self.message = error;
            }
        }
    }

    fn view(&self, _input: &Self::Input, context: &mut ViewContext<Self>) -> View {
        context.window_title(WINDOW_TITLE);
        context.window_visuals(
            WindowVisuals::new()
                .client_size(WINDOW_WIDTH, WINDOW_HEIGHT)
                .icon(APP_ICON_PATH),
        );

        if let UiMode::Settings { section } = self.mode.clone() {
            return Border::new()
                .background(Color::rgb(255, 255, 255))
                .padding(Thickness::uniform(18.0))
                .content(self.settings_view(section, context));
        }

        let content = match self.mode.clone() {
            UiMode::Browse => self.browse_view(context),
            UiMode::Edit { is_new } => {
                ScrollViewer::new().content(self.editor_panel(is_new, context))
            }
            UiMode::Settings { .. } => unreachable!("settings mode returns before main shell"),
        };

        let header = self.app_header(context);
        let footer = self.footer_bar(context);
        Grid::new()
            .rows([GridLength::Auto, GridLength::STAR, GridLength::Auto])
            .background(Color::rgb(238, 242, 246))
            .children((
                header,
                Border::new()
                    .background(Color::rgb(255, 255, 255))
                    .padding(Thickness::uniform(18.0))
                    .grid_row(1)
                    .content(content),
                footer,
            ))
    }
}

impl Launcher {
    fn save_draft(&mut self) {
        let item = self.draft.clone().into_item();
        let mut next = self.config.clone();
        next.items.retain(|existing| existing.id != item.id);
        next.items.push(item);
        match data_store::save(&self.config_path, &next) {
            Ok(()) => {
                self.config = next;
                self.message = "已保存".to_string();
                self.mode = UiMode::Browse;
            }
            Err(error) => self.message = error,
        }
    }

    fn delete_draft(&mut self) {
        let id = self.draft.id.clone();
        let mut next = self.config.clone();
        next.items.retain(|existing| existing.id != id);
        match data_store::save(&self.config_path, &next) {
            Ok(()) => {
                self.config = next;
                self.message = "已删除".to_string();
                self.mode = UiMode::Browse;
            }
            Err(error) => self.message = error,
        }
    }

    fn open_config_folder(&mut self) {
        let folder = self
            .config_path
            .parent()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| ".".to_string());
        let item = LauncherItem {
            id: "settings-folder".to_string(),
            name: "设置目录".to_string(),
            kind: ItemKind::Folder,
            target: folder,
            arguments: String::new(),
            category: String::new(),
            tags: Vec::new(),
            username: String::new(),
            favorite: false,
            notes: String::new(),
            icon_format: None,
            icon_data: None,
        };

        self.message = match launcher::open_item(&item) {
            Ok(()) => "已打开配置位置".to_string(),
            Err(error) => error,
        };
    }

    fn app_header(&self, context: &mut ViewContext<Self>) -> View {
        let query = self.query.clone();
        let settings_button = Button::new()
            .on_click(context.message(Msg::OpenSettings(SettingsSection::General)))
            .grid_column(3)
            .content("⚙");
        let grid = Grid::new()
            .columns([
                GridLength::Pixel(260.0),
                GridLength::STAR,
                GridLength::Auto,
                GridLength::Auto,
            ])
            .column_spacing(14.0)
            .children((
                TextBlock::new()
                    .text(WINDOW_TITLE)
                    .font_size(28.0)
                    .font_weight(FontWeight::BOLD)
                    .grid_column(0),
                TextBox::new()
                    .text(query)
                    .placeholder_text("搜索目录、程序、网址、内网 IP、标签或备注")
                    .on_text_changed(context.callback(move |value| Msg::Query(value)))
                    .grid_column(1),
                Button::new()
                    .style(ButtonStyle::Accent)
                    .on_click(context.message(Msg::NewEntry))
                    .grid_column(2)
                    .content("新增"),
                settings_button,
            ));

        Border::new()
            .background(Color::rgb(229, 233, 239))
            .border_brush(Color::rgb(199, 208, 220))
            .border_thickness(Thickness::uniform(1.0))
            .padding(Thickness::xy(18.0, 12.0))
            .grid_row(0)
            .content(grid)
    }

    fn browse_view(&self, context: &mut ViewContext<Self>) -> View {
        let toolbar = self.filter_toolbar(context);
        let results = self.result_list(context);
        Grid::new()
            .rows([GridLength::Auto, GridLength::STAR])
            .children((toolbar, results))
    }

    fn filter_toolbar(&self, context: &mut ViewContext<Self>) -> View {
        Border::new()
            .background(Color::rgb(237, 241, 245))
            .border_brush(Color::rgb(199, 208, 220))
            .border_thickness(Thickness::uniform(1.0))
            .padding(Thickness::xy(12.0, 10.0))
            .grid_row(0)
            .content(self.filter_bar(context))
    }

    fn filter_bar(&self, context: &mut ViewContext<Self>) -> View {
        let mut buttons: Vec<(String, View)> = vec![
            self.filter_button("全部", "all", CategoryFilter::All, context),
            self.filter_button("收藏", "favorites", CategoryFilter::Favorites, context),
            self.filter_button(
                "目录",
                "kind-folder",
                CategoryFilter::Kind(ItemKind::Folder),
                context,
            ),
            self.filter_button(
                "程序",
                "kind-program",
                CategoryFilter::Kind(ItemKind::Program),
                context,
            ),
            self.filter_button(
                "外网",
                "kind-url",
                CategoryFilter::Kind(ItemKind::Url),
                context,
            ),
            self.filter_button(
                "内网",
                "kind-intranet",
                CategoryFilter::Kind(ItemKind::IntranetUrl),
                context,
            ),
        ];

        for category in categories(&self.config.items) {
            let label = format!("分类：{category}");
            let key = format!("category-{category}");
            buttons.push(self.filter_button(
                label,
                &key,
                CategoryFilter::Category(category),
                context,
            ));
        }

        StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(8.0)
            .keyed_children(buttons)
    }

    fn filter_button(
        &self,
        label: impl Into<String>,
        key: &str,
        target: CategoryFilter,
        context: &mut ViewContext<Self>,
    ) -> (String, View) {
        let label = label.into();
        let text = if self.filter == target {
            format!("● {label}")
        } else {
            label
        };

        (
            key.to_string(),
            Button::new()
                .on_click(context.message(Msg::Filter(target)))
                .content(text),
        )
    }

    fn result_list(&self, context: &mut ViewContext<Self>) -> View {
        let items = search::filter_items(&self.config.items, &self.query, &self.filter);
        if items.is_empty() {
            let empty = Border::new()
                .background(ThemeBrush::CardBackground)
                .border_brush(Color::rgb(220, 220, 220))
                .border_thickness(Thickness::uniform(1.0))
                .corner_radius(8.0)
                .padding(Thickness::uniform(18.0))
                .content(
                    StackPanel::new()
                        .spacing(6.0)
                        .children((
                            TextBlock::new()
                                .text("没有匹配入口")
                                .font_size(18.0)
                                .font_weight(FontWeight::BOLD),
                            TextBlock::new().text("点右上角“新增”添加目录、程序或网址。"),
                        )),
                );
            return ScrollViewer::new().grid_row(1).content(empty);
        }

        let cards: Vec<(String, View)> = items
            .into_iter()
            .map(|item| {
                let key = item.id.clone();
                (key, self.result_card(item, context))
            })
            .collect();

        ScrollViewer::new().grid_row(1).content(StackPanel::new().spacing(8.0).keyed_children(cards))
    }

    fn result_card(&self, item: LauncherItem, context: &mut ViewContext<Self>) -> View {
        let title = if item.favorite {
            format!("★ {}", item.name)
        } else {
            item.name.clone()
        };
        let status = launcher::validate_target(&item)
            .err()
            .unwrap_or_else(|| kind_label(&item.kind).to_string());
        let meta = item_meta(&item);
        let edit_label = item.name.clone();
        let folder_button: View = if item.kind == ItemKind::Program {
            let target = format!("{} 位置", edit_label);
            Button::new()
                .on_click(context.message(Msg::OpenItemFolder(item.clone())))
                .content(target)
        } else {
            View::empty()
        };

        let text = StackPanel::new()
            .spacing(3.0)
            .width(650.0)
            .children((
                TextBlock::new()
                    .text(title)
                    .font_weight(FontWeight::BOLD)
                    .font_size(16.0),
                TextBlock::new().text(item.target.clone()),
                self.argument_line(&item),
                TextBlock::new()
                    .text(format!("{status} · {meta}"))
                    .font_size(12.0),
            ));

        Border::new()
            .background(ThemeBrush::CardBackground)
            .border_brush(Color::rgb(205, 210, 216))
            .border_thickness(Thickness::uniform(1.0))
            .corner_radius(8.0)
            .padding(Thickness::uniform(10.0))
            .content(
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(10.0)
                    .children((
                        card_icon(&item),
                        text,
                        Button::new()
                            .on_click(context.message(Msg::EditItem(item.clone())))
                            .content("编辑"),
                        folder_button,
                        Button::new()
                            .style(ButtonStyle::Accent)
                            .on_click(context.message(Msg::OpenItem(item)))
                            .content("打开"),
                    )),
            )
    }

    fn argument_line(&self, item: &LauncherItem) -> View {
        match program_argument_summary(item) {
            Some(summary) => TextBlock::new().text(summary).font_size(12.0).into(),
            None => View::empty(),
        }
    }

    fn footer_bar(&self, context: &mut ViewContext<Self>) -> View {
        let status = if self.message.is_empty() {
            credential::STATUS.to_string()
        } else {
            self.message.clone()
        };
        let count = self.config.items.len();
        let import: View = if self.busy {
            ProgressRing::new()
                .is_indeterminate(true)
                .grid_column(1)
                .into()
        } else {
            Button::new()
                .on_click(context.message(Msg::Import))
                .grid_column(1)
                .content("导入")
        };

        Border::new()
            .background(Color::rgb(229, 233, 239))
            .border_brush(Color::rgb(199, 208, 220))
            .border_thickness(Thickness::uniform(1.0))
            .padding(Thickness::xy(18.0, 10.0))
            .grid_row(2)
            .content(
                Grid::new()
                    .columns([GridLength::STAR, GridLength::Auto, GridLength::Auto])
                    .column_spacing(8.0)
                    .children((
                        TextBlock::new()
                            .text(format!("{count} 个入口 · {status}"))
                            .text_wrapping(TextWrapping::NoWrap)
                            .text_trimming(TextTrimming::CharacterEllipsis)
                            .grid_column(0),
                        import,
                        Button::new()
                            .on_click(context.message(Msg::Export))
                            .grid_column(2)
                            .content("导出"),
                    )),
            )
    }

    fn settings_view(&self, section: SettingsSection, context: &mut ViewContext<Self>) -> View {
        let content: View = match section {
            SettingsSection::General => self.general_settings(context),
            SettingsSection::Hotkey => self.hotkey_settings(context),
            SettingsSection::Data => self.data_settings(context),
            SettingsSection::About => self.about_settings(),
        };
        let header = StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(12.0)
            .children((
                TextBlock::new()
                    .text("设置")
                    .font_size(24.0)
                    .font_weight(FontWeight::BOLD)
                    .width(760.0),
                Button::new()
                    .on_click(context.message(Msg::Browse))
                    .content("关闭"),
            ));
        let sidebar = self.settings_sidebar(section, context);
        let panel = self.settings_content_panel(content);
        let layout = Grid::new()
            .columns([GridLength::Pixel(260.0), GridLength::STAR])
            .column_spacing(14.0)
            .children((sidebar, panel));

        Border::new()
            .background(Color::rgb(238, 242, 246))
            .border_brush(Color::rgb(205, 214, 226))
            .border_thickness(Thickness::uniform(1.0))
            .corner_radius(8.0)
            .padding(Thickness::uniform(16.0))
            .content(StackPanel::new().spacing(14.0).children((header, layout)))
    }

    fn settings_sidebar(&self, section: SettingsSection, context: &mut ViewContext<Self>) -> View {
        Border::new()
            .background(Color::rgb(236, 248, 248))
            .padding(Thickness::xy(10.0, 12.0))
            .grid_column(0)
            .vertical_alignment(VerticalAlignment::Top)
            .content(
                StackPanel::new()
                    .spacing(4.0)
                    .children((
                        self.settings_nav_button("常规", Symbol::Setting, SettingsSection::General, section, context),
                        self.settings_nav_button("快捷键", Symbol::Keyboard, SettingsSection::Hotkey, section, context),
                        self.settings_nav_button("数据", Symbol::SaveLocal, SettingsSection::Data, section, context),
                        self.settings_nav_button("关于", Symbol::Help, SettingsSection::About, section, context),
                    )),
            )
    }

    fn settings_content_panel(&self, content: View) -> View {
        Border::new()
            .background(Color::rgb(248, 250, 252))
            .border_brush(Color::rgb(200, 210, 224))
            .border_thickness(Thickness::uniform(1.0))
            .corner_radius(8.0)
            .padding(Thickness::uniform(14.0))
            .grid_column(1)
            .content(ScrollViewer::new().content(content))
    }

    fn settings_nav_button(
        &self,
        label: &'static str,
        symbol: Symbol,
        target: SettingsSection,
        current: SettingsSection,
        context: &mut ViewContext<Self>,
    ) -> View {
        let is_current = current == target;
        let row_color = if is_current {
            Color::rgb(222, 237, 245)
        } else {
            Color::rgb(236, 248, 248)
        };
        let indicator_color = if is_current {
            Color::rgb(0, 120, 212)
        } else {
            Color::rgb(236, 248, 248)
        };

        Border::new()
            .background(row_color)
            .corner_radius(6.0)
            .height(40.0)
            .width(240.0)
            .on_pointer_pressed(context.callback(move |_| Msg::OpenSettings(target)))
            .content(
                Grid::new()
                    .columns([
                        GridLength::Pixel(4.0),
                        GridLength::Pixel(36.0),
                        GridLength::STAR,
                    ])
                    .column_spacing(8.0)
                    .children((
                        Border::new()
                            .background(indicator_color)
                            .corner_radius(2.0)
                            .width(4.0)
                            .height(24.0)
                            .vertical_alignment(VerticalAlignment::Center)
                            .grid_column(0)
                            .content(View::empty()),
                        View::from(
                            SymbolIcon::new()
                                .symbol(symbol)
                                .horizontal_alignment(HorizontalAlignment::Center)
                                .vertical_alignment(VerticalAlignment::Center)
                                .grid_column(1),
                        ),
                        TextBlock::new()
                            .text(label)
                            .font_size(14.0)
                            .vertical_alignment(VerticalAlignment::Center)
                            .grid_column(2),
                    )),
            )
    }

    fn setting_row(
        &self,
        title: &'static str,
        description: impl Into<String>,
        control: View,
    ) -> View {
        let description = description.into();
        Grid::new()
            .columns([GridLength::STAR, GridLength::Pixel(220.0)])
            .column_spacing(14.0)
            .children((
                StackPanel::new()
                    .spacing(3.0)
                    .grid_column(0)
                    .children((
                        TextBlock::new()
                            .text(title)
                            .font_size(14.0)
                            .font_weight(FontWeight::BOLD),
                        TextBlock::new().text(description).font_size(12.0),
                    )),
                Border::new().grid_column(1).content(control),
            ))
            .into()
    }

    fn toggle_setting(
        &self,
        checked: bool,
        kind: SettingKind,
        is_loading: bool,
        context: &mut ViewContext<Self>,
    ) -> View {
        CheckBox::new()
            .is_checked(checked)
            .is_enabled(!is_loading)
            .on_is_checked_changed(context.callback(move |value| Msg::ToggleSetting(kind, value)))
            .content(kind.label())
    }

    fn general_settings(&self, context: &mut ViewContext<Self>) -> View {
        let settings = self.config.settings.clone();
        let shortcut_exists = windows_shortcut::desktop_shortcut_exists();
        let is_loading = self.busy;
        let shortcut_label = if shortcut_exists {
            "已存在"
        } else {
            "创建快捷方式"
        };
        let shortcut_button = Button::new()
            .is_enabled(!shortcut_exists && !is_loading)
            .on_click(context.message(Msg::CreateShortcut))
            .content(shortcut_label);
        let login = self.toggle_setting(
            settings.launch_at_login,
            SettingKind::LaunchAtLogin,
            is_loading,
            context,
        );
        let startup = self.toggle_setting(
            settings.show_window_on_startup,
            SettingKind::ShowWindowOnStartup,
            is_loading,
            context,
        );
        let tray = self.toggle_setting(
            settings.close_to_tray,
            SettingKind::CloseToTray,
            is_loading,
            context,
        );
        let title = self.setting_row(
            "开机自启",
            "登录 Windows 后自动启动这个工具。",
            login,
        );
        let startup_row = self.setting_row(
            "启动窗口",
            "启动程序时是否直接显示主窗口。",
            startup,
        );
        let tray_row = self.setting_row(
            "关闭行为",
            "点关闭按钮时保留托盘后台入口。",
            tray,
        );
        let shortcut_row = self.setting_row(
            "桌面快捷方式",
            "在桌面创建一个启动入口。",
            shortcut_button,
        );

        StackPanel::new()
            .spacing(10.0)
            .children((
                TextBlock::new()
                    .text("常规")
                    .font_size(18.0)
                    .font_weight(FontWeight::BOLD),
                title,
                startup_row,
                tray_row,
                shortcut_row,
            ))
    }

    fn hotkey_settings(&self, context: &mut ViewContext<Self>) -> View {
        let settings = self.config.settings.clone();
        let is_loading = self.busy;
        let state: View = if is_loading {
            ProgressRing::new().is_indeterminate(true).into()
        } else {
            View::empty()
        };
        let hotkey = self.toggle_setting(
            settings.global_hotkey_enabled,
            SettingKind::GlobalHotkey,
            is_loading,
            context,
        );
        let hotkey_row = self.setting_row(
            "全局唤起",
            "在后台运行时使用快捷键打开窗口。",
            hotkey,
        );
        let key_row = self.setting_row(
            "快捷键",
            hotkey_status_message(&self.message)
                .unwrap_or_else(|| "第一版固定支持 Ctrl + Alt + Space。".to_string()),
            TextBox::new()
                .text(settings.global_hotkey.clone())
                .header("快捷键")
                .placeholder_text("Ctrl + Alt + Space")
                .is_enabled(false)
                .into(),
        );

        StackPanel::new()
            .spacing(10.0)
            .children((
                TextBlock::new()
                    .text("快捷键")
                    .font_size(18.0)
                    .font_weight(FontWeight::BOLD),
                hotkey_row,
                key_row,
                state,
            ))
    }

    fn data_settings(&self, context: &mut ViewContext<Self>) -> View {
        let display_path = self.config_path.display().to_string();
        let open_button = Button::new()
            .on_click(context.message(Msg::OpenConfigFolder))
            .content("打开位置");
        let path_row = self.setting_row("配置文件", display_path, View::empty());
        let open_row = self.setting_row(
            "配置目录",
            "用资源管理器打开配置文件所在目录。",
            open_button,
        );

        StackPanel::new()
            .spacing(10.0)
            .children((
                TextBlock::new()
                    .text("数据")
                    .font_size(18.0)
                    .font_weight(FontWeight::BOLD),
                path_row,
                open_row,
            ))
    }

    fn about_settings(&self) -> View {
        StackPanel::new()
            .spacing(10.0)
            .children((
                TextBlock::new()
                    .text("关于")
                    .font_size(18.0)
                    .font_weight(FontWeight::BOLD),
                Border::new()
                    .background(Color::rgb(255, 255, 255))
                    .border_brush(Color::rgb(217, 225, 235))
                    .border_thickness(Thickness::uniform(1.0))
                    .corner_radius(8.0)
                    .padding(Thickness::uniform(12.0))
                    .content(
                        StackPanel::new().spacing(4.0).children((
                            TextBlock::new()
                                .text(format!("Local Launcher · 版本 {}", env!("CARGO_PKG_VERSION"))),
                            TextBlock::new().text("本地快捷入口和导航工具。"),
                        )),
                    ),
                Border::new()
                    .background(Color::rgb(255, 255, 255))
                    .border_brush(Color::rgb(217, 225, 235))
                    .border_thickness(Thickness::uniform(1.0))
                    .corner_radius(8.0)
                    .padding(Thickness::uniform(12.0))
                    .content(TextBlock::new().text("凭据策略：第一版只保存账号名和备注，不保存密码。")),
            ))
    }

    fn editor_panel(&self, is_new: bool, context: &mut ViewContext<Self>) -> View {
        let draft = &self.draft;
        let title = if is_new { "新增入口" } else { "编辑入口" };
        let header = StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(12.0)
            .children((
                TextBlock::new()
                    .text(title)
                    .font_size(24.0)
                    .font_weight(FontWeight::BOLD)
                    .width(760.0),
                Button::new()
                    .on_click(context.message(Msg::Browse))
                    .content("取消"),
            ));
        let name = self.draft_text_field(DraftField::Name, draft.name.clone(), "名称", context);
        let kind = ComboBox::new()
            .header("类型")
            .items_source(["目录", "程序", "外网", "内网"])
            .selected_index(kind_index(draft.kind))
            .on_selection_changed(context.callback(|index| Msg::DraftKind(index)));
        let target =
            self.draft_text_field(DraftField::Target, draft.target.clone(), "目标", context);
        let picker_row = self.target_picker(context);
        let arguments = self.program_arguments_editor(context);
        let category =
            self.draft_text_field(DraftField::Category, draft.category.clone(), "分类", context);
        let tags = self.draft_text_field(
            DraftField::Tags,
            draft.tags.clone(),
            "标签，逗号分隔",
            context,
        );
        let account = StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(12.0)
            .children((
                self.draft_text_field(DraftField::Username, draft.username.clone(), "账号名", context)
                    .width(300.0),
                CheckBox::new()
                    .is_checked(draft.favorite)
                    .on_is_checked_changed(context.callback(|value| Msg::DraftFavorite(value)))
                    .content("收藏"),
            ));
        let notes = self.draft_text_field(DraftField::Notes, draft.notes.clone(), "备注", context);
        let actions = StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(8.0)
            .children((
                Button::new()
                    .style(ButtonStyle::Accent)
                    .on_click(context.message(Msg::SaveDraft))
                    .content("保存"),
                Button::new()
                    .is_enabled(!is_new)
                    .on_click(context.message(Msg::DeleteDraft))
                    .content("删除"),
                Button::new()
                    .on_click(context.message(Msg::Browse))
                    .content("取消"),
            ));

        Border::new()
            .background(ThemeBrush::CardBackground)
            .border_brush(Color::rgb(205, 210, 216))
            .border_thickness(Thickness::uniform(1.0))
            .corner_radius(8.0)
            .padding(Thickness::uniform(16.0))
            .content(
                StackPanel::new()
                    .spacing(10.0)
                    .children((
                        header,
                        name,
                        View::from(kind),
                        target,
                        picker_row,
                        arguments,
                        category,
                        tags,
                        account,
                        notes,
                        actions,
                    )),
            )
    }

    fn draft_text_field(
        &self,
        field: DraftField,
        value: String,
        header: &'static str,
        context: &mut ViewContext<Self>,
    ) -> TextBox {
        TextBox::new()
            .text(value)
            .header(header)
            .on_text_changed(context.callback(move |value| Msg::Draft(field, value)))
    }

    fn target_picker(&self, context: &mut ViewContext<Self>) -> View {
        if self.busy {
            return ProgressRing::new().is_indeterminate(true).into();
        }

        match self.draft.kind {
            ItemKind::Folder => Button::new()
                .on_click(context.message(Msg::PickFolder))
                .content("选择目录"),
            ItemKind::Program => Button::new()
                .on_click(context.message(Msg::PickProgram))
                .content("选择程序"),
            ItemKind::Url | ItemKind::IntranetUrl => StackPanel::new()
                .orientation(Orientation::Horizontal)
                .spacing(8.0)
                .children((
                    TextBlock::new()
                        .text("请输入 http:// 或 https:// 开头的网址")
                        .font_size(12.0)
                        .width(300.0),
                    Button::new()
                        .on_click(context.message(Msg::FetchTitle))
                        .content("获取标题"),
                ))
                .into(),
        }
    }

    fn program_arguments_editor(&self, context: &mut ViewContext<Self>) -> View {
        if self.draft.kind != ItemKind::Program {
            return View::empty();
        }

        self.draft_text_field(
            DraftField::Arguments,
            self.draft.arguments.clone(),
            "启动参数",
            context,
        )
        .into()
    }
}

fn hotkey_status_message(message: &str) -> Option<String> {
    message
        .contains("注册全局快捷键失败")
        .then(|| format!("快捷键冲突或注册失败：{message}"))
}

fn apply_draft_field(draft: &mut DraftItem, field: DraftField, value: String) {
    match field {
        DraftField::Name => draft.name = value,
        DraftField::Target => {
            if draft.target != value {
                draft.icon_format = None;
                draft.icon_data = None;
            }
            draft.target = value;
        }
        DraftField::Category => draft.category = value,
        DraftField::Tags => draft.tags = value,
        DraftField::Username => draft.username = value,
        DraftField::Notes => draft.notes = value,
        DraftField::Arguments => draft.arguments = value,
    }
}

fn update_settings(
    mut config: LauncherConfig,
    config_path: &Path,
    value: bool,
    kind: SettingKind,
) -> std::result::Result<AsyncUiResult, String> {
    let old_settings = config.settings.clone();
    let mut next_settings = config.settings.clone();
    let update_startup_shortcut = kind == SettingKind::LaunchAtLogin;
    if update_startup_shortcut {
        windows_shortcut::set_launch_at_login(value)?;
    }
    apply_setting(&mut next_settings, kind, value);
    config.settings = next_settings.clone();
    if let Err(error) = crate::tray::configure(next_settings) {
        if update_startup_shortcut {
            let _ = windows_shortcut::set_launch_at_login(!value);
        }
        return Err(error);
    }
    if let Err(error) = data_store::save(config_path, &config) {
        if update_startup_shortcut {
            let _ = windows_shortcut::set_launch_at_login(!value);
        }
        let _ = crate::tray::configure(old_settings);
        return Err(error);
    }

    Ok(AsyncUiResult::SettingsUpdated {
        config,
        message: kind.message().to_string(),
    })
}

fn apply_setting(settings: &mut LauncherSettings, kind: SettingKind, value: bool) {
    match kind {
        SettingKind::LaunchAtLogin => settings.launch_at_login = value,
        SettingKind::ShowWindowOnStartup => settings.show_window_on_startup = value,
        SettingKind::CloseToTray => settings.close_to_tray = value,
        SettingKind::GlobalHotkey => settings.global_hotkey_enabled = value,
    }
}

fn item_meta(item: &LauncherItem) -> String {
    let tags = if item.tags.is_empty() {
        "无标签".to_string()
    } else {
        item.tags.join(" / ")
    };
    let username = if item.username.trim().is_empty() {
        "无账号".to_string()
    } else {
        format!("账号：{}", item.username)
    };
    let intranet = if item.kind == ItemKind::IntranetUrl {
        " · INTRANET"
    } else {
        ""
    };

    format!("{} · {} · {}{}", item.category, tags, username, intranet)
}

fn program_argument_summary(item: &LauncherItem) -> Option<String> {
    let arguments = item.arguments.trim();
    (item.kind == ItemKind::Program && !arguments.is_empty()).then(|| format!("参数: {arguments}"))
}

fn categories(items: &[LauncherItem]) -> Vec<String> {
    items
        .iter()
        .filter_map(|item| {
            let category = item.category.trim();
            (!category.is_empty()).then(|| category.to_string())
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn kind_label(kind: &ItemKind) -> &'static str {
    match kind {
        ItemKind::Folder => "目录",
        ItemKind::Program => "程序",
        ItemKind::Url => "外网",
        ItemKind::IntranetUrl => "内网",
    }
}

fn kind_color(kind: &ItemKind) -> Color {
    match kind {
        ItemKind::Folder => Color::rgb(218, 236, 255),
        ItemKind::Program => Color::rgb(229, 240, 223),
        ItemKind::Url => Color::rgb(238, 232, 255),
        ItemKind::IntranetUrl => Color::rgb(255, 232, 222),
    }
}

fn card_icon(item: &LauncherItem) -> View {
    let icon: Option<Image> = icon_data::icon_uri(item)
        .and_then(|uri| Image::new().width(30.0).height(30.0).source(uri).ok());

    match icon {
        Some(image) => Border::new()
            .background(Color::rgb(248, 250, 252))
            .border_brush(Color::rgb(218, 223, 230))
            .border_thickness(Thickness::uniform(1.0))
            .corner_radius(6.0)
            .padding(Thickness::uniform(6.0))
            .width(74.0)
            .content(image),
        None => Border::new()
            .background(kind_color(&item.kind))
            .corner_radius(6.0)
            .padding(Thickness::xy(10.0, 6.0))
            .width(74.0)
            .content(
                TextBlock::new()
                    .text(kind_label(&item.kind))
                    .font_weight(FontWeight::BOLD)
                    .font_size(13.0),
            ),
    }
}

fn kind_index(kind: ItemKind) -> usize {
    match kind {
        ItemKind::Folder => 0,
        ItemKind::Program => 1,
        ItemKind::Url => 2,
        ItemKind::IntranetUrl => 3,
    }
}

fn kind_from_index(index: Option<usize>) -> ItemKind {
    match index {
        Some(0) => ItemKind::Folder,
        Some(1) => ItemKind::Program,
        Some(3) => ItemKind::IntranetUrl,
        _ => ItemKind::Url,
    }
}

fn new_id() -> String {
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    format!("item-{id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn production_source() -> &'static str {
        include_str!("ui.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default()
    }

    fn settings_nav_source(source: &'static str) -> &'static str {
        source
            .split("fn settings_nav_button(")
            .nth(1)
            .unwrap_or_default()
            .split("fn setting_row(")
            .next()
            .unwrap_or_default()
    }

    #[test]
    fn program_argument_summary_shows_only_non_empty_program_arguments() {
        let mut item = LauncherItem {
            id: "tool".to_string(),
            name: "工具".to_string(),
            kind: ItemKind::Program,
            target: "C:\\Tools\\tool.exe".to_string(),
            arguments: "--dev --port 8080".to_string(),
            category: String::new(),
            tags: Vec::new(),
            username: String::new(),
            favorite: false,
            notes: String::new(),
            icon_format: None,
            icon_data: None,
        };

        assert_eq!(
            program_argument_summary(&item),
            Some("参数: --dev --port 8080".to_string())
        );

        item.arguments.clear();
        assert_eq!(program_argument_summary(&item), None);

        item.kind = ItemKind::Url;
        item.arguments = "--ignored".to_string();
        assert_eq!(program_argument_summary(&item), None);
    }

    #[test]
    fn settings_mode_and_header_button_exist() {
        let source = production_source();

        assert!(source.contains("Settings { section: SettingsSection }"));
        assert!(source.contains(".content(\"⚙\")"));
        assert!(source.contains("fn settings_view("));
    }

    #[test]
    fn settings_view_has_expected_sections() {
        let source = production_source();

        assert!(source.contains("常规"));
        assert!(source.contains("快捷键"));
        assert!(source.contains("数据"));
        assert!(source.contains("关于"));
        assert!(source.contains("创建快捷方式"));
        assert!(source.contains("打开位置"));
    }

    #[test]
    fn settings_view_has_distinct_visual_layers() {
        let source = production_source();

        assert!(source.contains("fn settings_sidebar("));
        assert!(source.contains("fn settings_content_panel("));
        assert!(source.contains("fn setting_row("));
        assert!(!source.contains("fn settings_tabs("));
        assert!(source.contains("GridLength::Pixel(260.0)"));
        assert!(source.contains("Color::rgb(236, 248, 248)"));
        assert!(source.contains("Color::rgb(0, 120, 212)"));
        assert!(source.contains(".vertical_alignment(VerticalAlignment::Top)"));
        assert!(source.contains("Symbol::Keyboard"));
        assert!(settings_nav_source(source).contains("SymbolIcon::new()"));
        assert!(settings_nav_source(source).contains("GridLength::Pixel(36.0)"));
        assert!(settings_nav_source(source).contains(".height(40.0)"));
        assert!(settings_nav_source(source).contains(".on_pointer_pressed("));
        assert!(!settings_nav_source(source).contains(".style(ButtonStyle::Accent)"));
    }

    #[test]
    fn settings_mode_replaces_main_shell() {
        let source = production_source();
        let settings_gate = source
            .find("if let UiMode::Settings { section } = self.mode.clone()")
            .unwrap_or(usize::MAX);
        let main_header = source.find("self.app_header(context").unwrap_or_default();

        assert!(settings_gate < main_header);
        assert!(source.contains(".content(self.settings_view(section, context))"));
        assert!(source.contains("unreachable!(\"settings mode returns before main shell\")"));
    }

    #[test]
    fn settings_controls_are_disabled_while_loading() {
        let source = production_source();

        assert!(source.contains("let is_loading = self.busy;"));
        assert!(source.contains(".is_enabled(!is_loading)"));
        assert!(source.contains(".is_enabled(!shortcut_exists && !is_loading)"));
    }

    #[test]
    fn hotkey_settings_show_registration_failure_inline() {
        let source = production_source();

        assert!(source.contains("fn hotkey_status_message("));
        assert!(source.contains("注册全局快捷键失败"));
        assert!(source.contains("hotkey_status_message(&self.message)"));
    }

    #[test]
    fn hotkey_status_message_only_shows_registration_failures() {
        assert!(
            hotkey_status_message("注册全局快捷键失败: 已占用")
                .unwrap()
                .contains("快捷键冲突或注册失败")
        );
        assert_eq!(hotkey_status_message("已更新全局唤起设置"), None);
    }

    #[test]
    fn launch_at_login_rolls_back_shortcut_when_save_fails() {
        let source = production_source();

        assert!(source.contains("if let Err(error) = data_store::save(config_path, &config)"));
        assert!(source.contains("windows_shortcut::set_launch_at_login(!value)"));
    }

    #[test]
    fn settings_apply_tray_configuration_before_persisting() {
        let source = production_source();
        let update_settings = source
            .split("fn update_settings(")
            .nth(1)
            .unwrap_or_default();
        let configure = update_settings.find("crate::tray::configure");
        let save = update_settings.find("data_store::save");

        assert!(configure.is_some());
        assert!(save.is_some());
        assert!(configure < save);
        assert!(update_settings.contains("return Err(error);"));
    }

    #[test]
    fn setting_messages_are_distinct_per_kind() {
        assert_eq!(
            SettingKind::LaunchAtLogin.message(),
            "已更新开机自启"
        );
        assert_eq!(
            SettingKind::GlobalHotkey.message(),
            "已更新全局唤起设置"
        );
    }

    #[test]
    fn result_list_height_is_not_fixed() {
        let source = include_str!("ui.rs");
        let fixed_height = concat!(".height", "(430.0)");

        assert!(!source.contains(fixed_height));
    }

    #[test]
    fn editor_panel_is_scrollable() {
        let source = production_source();
        let edit_arm = source
            .split("UiMode::Edit { is_new } =>")
            .nth(1)
            .and_then(|tail| tail.split("UiMode::Settings").next())
            .unwrap_or_default();

        assert!(edit_arm.contains("ScrollViewer::new()"));
        assert!(edit_arm.contains("self.editor_panel("));
    }

    #[test]
    fn page_has_distinct_header_and_footer_sections() {
        let source = include_str!("ui.rs");

        assert!(source.contains("fn app_header("));
        assert!(source.contains("fn footer_bar("));
        assert!(source.contains("fn filter_toolbar("));
    }

    #[test]
    fn section_colors_are_visibly_distinct_from_white() {
        let source = include_str!("ui.rs");
        let page_background = concat!("Color::rgb", "(238, 242, 246)");
        let section_background = concat!("Color::rgb", "(229, 233, 239)");
        let toolbar_background = concat!("Color::rgb", "(237, 241, 245)");
        let divider = concat!("Color::rgb", "(199, 208, 220)");

        assert!(source.contains(page_background));
        assert!(source.contains(section_background));
        assert!(source.contains(toolbar_background));
        assert!(source.contains(divider));
    }

    #[test]
    fn window_visuals_carry_size_and_icon() {
        let source = production_source();

        assert!(source.contains("context.window_title(WINDOW_TITLE)"));
        assert!(source.contains("WindowVisuals::new()"));
        assert!(source.contains("WINDOW_WIDTH, WINDOW_HEIGHT"));
        assert!(source.contains(".icon(APP_ICON_PATH)"));
    }

    #[test]
    fn background_work_runs_on_component_context() {
        let source = production_source();

        assert!(source.contains("context.spawn_background("));
        assert!(!source.contains("use_mutation("));
        assert!(!source.contains("RenderCx"));
        assert!(!source.contains("Element"));
    }
}
