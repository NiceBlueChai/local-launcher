//! Windows Reactor UI for the local launcher.

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
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use windows_reactor::*;

#[derive(Clone, Debug, Eq, PartialEq)]
enum UiMode {
    Browse,
    Edit { is_new: bool },
    Settings { section: SettingsSection },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SettingsSection {
    General,
    Hotkey,
    Data,
    About,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum AsyncUiResult {
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct DraftItem {
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

/// Renders the launcher main window.
pub fn app(cx: &mut RenderCx) -> Element {
    let config_path = PathBuf::from("items.json");
    let initial_config =
        data_store::load_or_empty(&config_path).unwrap_or_else(|_| LauncherConfig::empty());
    let (config, set_config) = cx.use_state(initial_config);
    let (query, set_query) = cx.use_state(String::new());
    let (filter, set_filter) = cx.use_state(CategoryFilter::All);
    let (message, set_message) = cx.use_state(String::new());
    let (draft, set_draft) = cx.use_state(DraftItem::sample());
    let (mode, set_mode) = cx.use_state(UiMode::Browse);
    let (async_state, async_trigger) = cx.use_mutation::<AsyncUiResult>();

    {
        let set_draft = set_draft.clone();
        let set_config = set_config.clone();
        let set_message = set_message.clone();
        let async_trigger = async_trigger.clone();
        let async_state_for_deps = async_state.clone();
        let async_state_for_match = async_state.clone();
        cx.use_effect(
            (async_state_for_deps,),
            move || match async_state_for_match {
                MutationState::Success(AsyncUiResult::DraftUpdated { draft, message }) => {
                    set_draft.call(draft);
                    set_message.call(message);
                    async_trigger.reset();
                }
                MutationState::Success(AsyncUiResult::ConfigUpdated { config, message }) => {
                    set_config.call(config);
                    set_message.call(message);
                    async_trigger.reset();
                }
                MutationState::Success(AsyncUiResult::SettingsUpdated { config, message }) => {
                    set_config.call(config);
                    set_message.call(message);
                    async_trigger.reset();
                }
                MutationState::Success(AsyncUiResult::Message(message)) => {
                    set_message.call(message);
                    async_trigger.reset();
                }
                MutationState::Error(error) => {
                    set_message.call(error);
                    async_trigger.reset();
                }
                MutationState::Idle | MutationState::Loading => {}
            },
        );
    }

    let items = search::filter_items(&config.items, &query, &filter);
    let categories = categories(&config.items);

    if let UiMode::Settings { section } = mode.clone() {
        return settings_view(
            config,
            config_path,
            message,
            set_message,
            async_state,
            async_trigger,
            set_mode,
            section,
        )
        .background(Color::rgb(255, 255, 255))
        .padding(Thickness::uniform(18.0));
    }

    let content = match mode.clone() {
        UiMode::Browse => browse_view(
            filter,
            items,
            categories,
            set_filter,
            set_message.clone(),
            set_draft.clone(),
            set_mode.clone(),
        ),
        UiMode::Edit { is_new } => scroll_view(editor_panel(
            config.clone(),
            set_config.clone(),
            draft,
            set_draft.clone(),
            config_path.clone(),
            set_message.clone(),
            async_state.clone(),
            async_trigger.clone(),
            set_mode.clone(),
            is_new,
        ))
        .into(),
        UiMode::Settings { .. } => unreachable!("settings mode returns before main shell"),
    };

    grid((
        app_header(query, set_query, set_draft, set_mode).grid_row(0),
        content
            .grid_row(1)
            .background(Color::rgb(255, 255, 255))
            .padding(Thickness::uniform(18.0)),
        footer_bar(
            config,
            config_path,
            message,
            set_message,
            async_state,
            async_trigger,
        )
        .grid_row(2),
    ))
    .rows([GridLength::Auto, GridLength::STAR, GridLength::Auto])
    .background(Color::rgb(238, 242, 246))
    .into()
}

fn app_header(
    query: String,
    set_query: SetState<String>,
    set_draft: SetState<DraftItem>,
    set_mode: SetState<UiMode>,
) -> Element {
    let set_edit_mode = set_mode.clone();

    border(
        grid((
            text_block("Local Launcher")
                .font_size(28.0)
                .bold()
                .grid_column(0),
            text_box(query)
                .placeholder_text("搜索目录、程序、网址、内网 IP、标签或备注")
                .on_text_changed(move |value| set_query.call(value))
                .grid_column(1),
            button("新增")
                .accent()
                .on_click(move || {
                    set_draft.call(DraftItem::sample());
                    set_edit_mode.call(UiMode::Edit { is_new: true });
                })
                .grid_column(2),
            button("⚙")
                .on_click(move || {
                    set_mode.call(UiMode::Settings {
                        section: SettingsSection::General,
                    });
                })
                .grid_column(3),
        ))
        .columns([
            GridLength::Pixel(260.0),
            GridLength::STAR,
            GridLength::Auto,
            GridLength::Auto,
        ])
        .column_spacing(14.0),
    )
    .background(Color::rgb(229, 233, 239))
    .border_brush(Color::rgb(199, 208, 220))
    .border_thickness(Thickness::uniform(1.0))
    .padding(Thickness::xy(18.0, 12.0))
    .into()
}

fn browse_view(
    filter: CategoryFilter,
    items: Vec<LauncherItem>,
    categories: Vec<String>,
    set_filter: SetState<CategoryFilter>,
    set_message: SetState<String>,
    set_draft: SetState<DraftItem>,
    set_mode: SetState<UiMode>,
) -> Element {
    grid((
        filter_toolbar(filter, categories, set_filter).grid_row(0),
        result_list(items, set_message, set_draft, set_mode).grid_row(1),
    ))
    .rows([GridLength::Auto, GridLength::STAR])
    .into()
}

fn filter_toolbar(
    filter: CategoryFilter,
    categories: Vec<String>,
    set_filter: SetState<CategoryFilter>,
) -> Element {
    border(filter_bar(filter, categories, set_filter))
        .background(Color::rgb(237, 241, 245))
        .border_brush(Color::rgb(199, 208, 220))
        .border_thickness(Thickness::uniform(1.0))
        .padding(Thickness::xy(12.0, 10.0))
        .into()
}

fn filter_bar(
    filter: CategoryFilter,
    categories: Vec<String>,
    set_filter: SetState<CategoryFilter>,
) -> Element {
    let mut buttons: Vec<Element> = vec![
        filter_button("全部", CategoryFilter::All, &filter, set_filter.clone()),
        filter_button(
            "收藏",
            CategoryFilter::Favorites,
            &filter,
            set_filter.clone(),
        ),
        filter_button(
            "目录",
            CategoryFilter::Kind(ItemKind::Folder),
            &filter,
            set_filter.clone(),
        ),
        filter_button(
            "程序",
            CategoryFilter::Kind(ItemKind::Program),
            &filter,
            set_filter.clone(),
        ),
        filter_button(
            "外网",
            CategoryFilter::Kind(ItemKind::Url),
            &filter,
            set_filter.clone(),
        ),
        filter_button(
            "内网",
            CategoryFilter::Kind(ItemKind::IntranetUrl),
            &filter,
            set_filter.clone(),
        ),
    ];

    buttons.extend(categories.into_iter().map(|category| {
        let target = CategoryFilter::Category(category.clone());
        filter_button(
            format!("分类：{category}"),
            target,
            &filter,
            set_filter.clone(),
        )
    }));

    hstack(buttons).spacing(8.0).into()
}

fn filter_button(
    label: impl Into<String>,
    target: CategoryFilter,
    current: &CategoryFilter,
    set_filter: SetState<CategoryFilter>,
) -> Element {
    let label = label.into();
    let text = if *current == target {
        format!("● {label}")
    } else {
        label
    };

    button(text)
        .on_click(move || set_filter.call(target.clone()))
        .into()
}

fn result_list(
    items: Vec<LauncherItem>,
    set_message: SetState<String>,
    set_draft: SetState<DraftItem>,
    set_mode: SetState<UiMode>,
) -> Element {
    if items.is_empty() {
        return border(
            vstack((
                text_block("没有匹配入口").font_size(18.0).bold(),
                text_block("点右上角“新增”添加目录、程序或网址。"),
            ))
            .spacing(6.0),
        )
        .background(ThemeRef::CardBackground)
        .border_brush(Color::rgb(220, 220, 220))
        .border_thickness(Thickness::uniform(1.0))
        .corner_radius(8.0)
        .padding(Thickness::uniform(18.0))
        .into();
    }

    let cards: Vec<Element> = items
        .into_iter()
        .map(|item| {
            result_card(
                item,
                set_message.clone(),
                set_draft.clone(),
                set_mode.clone(),
            )
        })
        .collect();

    scroll_view(vstack(cards).spacing(8.0)).into()
}

fn result_card(
    item: LauncherItem,
    set_message: SetState<String>,
    set_draft: SetState<DraftItem>,
    set_mode: SetState<UiMode>,
) -> Element {
    let title = if item.favorite {
        format!("★ {}", item.name)
    } else {
        item.name.clone()
    };
    let status = launcher::validate_target(&item)
        .err()
        .unwrap_or_else(|| kind_label(&item.kind).to_string());
    let meta = item_meta(&item);
    let edit_item = item.clone();
    let folder_item = item.clone();
    let folder_button: Element = if item.kind == ItemKind::Program {
        button("位置")
            .on_click({
                let set_message = set_message.clone();
                move || {
                    if let Err(error) = launcher::open_containing_folder(&folder_item) {
                        set_message.call(error);
                    }
                }
            })
            .into()
    } else {
        text_block("").width(0.0).into()
    };

    border(
        hstack((
            card_icon(&item),
            vstack((
                text_block(title).bold().font_size(16.0),
                text_block(item.target.clone()),
                program_argument_line(&item),
                text_block(format!("{status} · {meta}")).font_size(12.0),
            ))
            .spacing(3.0)
            .width(650.0),
            button("编辑").on_click(move || {
                set_draft.call(DraftItem::from(edit_item.clone()));
                set_mode.call(UiMode::Edit { is_new: false });
            }),
            folder_button,
            button("打开").accent().on_click(move || {
                if let Err(error) = launcher::open_item(&item) {
                    set_message.call(error);
                }
            }),
        ))
        .spacing(10.0),
    )
    .background(ThemeRef::CardBackground)
    .border_brush(Color::rgb(205, 210, 216))
    .border_thickness(Thickness::uniform(1.0))
    .corner_radius(8.0)
    .padding(Thickness::uniform(10.0))
    .into()
}

fn settings_view(
    config: LauncherConfig,
    config_path: PathBuf,
    message: String,
    set_message: SetState<String>,
    async_state: MutationState<AsyncUiResult>,
    async_trigger: MutationTrigger<AsyncUiResult>,
    set_mode: SetState<UiMode>,
    section: SettingsSection,
) -> Element {
    let content = match section {
        SettingsSection::General => general_settings(
            config,
            config_path,
            set_message,
            async_state.clone(),
            async_trigger,
        ),
        SettingsSection::Hotkey => {
            hotkey_settings(config, config_path, message, async_state, async_trigger)
        }
        SettingsSection::Data => data_settings(config_path, set_message),
        SettingsSection::About => about_settings(),
    };

    border(
        vstack((
            hstack((
                text_block("设置").font_size(24.0).bold().width(760.0),
                button("关闭").on_click({
                    let set_mode = set_mode.clone();
                    move || set_mode.call(UiMode::Browse)
                }),
            ))
            .spacing(12.0),
            grid((
                settings_sidebar(section, set_mode)
                    .vertical_alignment(VerticalAlignment::Top)
                    .grid_column(0),
                settings_content_panel(content).grid_column(1),
            ))
            .columns([GridLength::Pixel(260.0), GridLength::STAR])
            .column_spacing(14.0),
        ))
        .spacing(14.0),
    )
    .background(Color::rgb(238, 242, 246))
    .border_brush(Color::rgb(205, 214, 226))
    .border_thickness(Thickness::uniform(1.0))
    .corner_radius(8.0)
    .padding(Thickness::uniform(16.0))
    .into()
}

fn settings_sidebar(section: SettingsSection, set_mode: SetState<UiMode>) -> Element {
    border(
        vstack((
            settings_nav_button(
                "常规",
                Symbol::Setting,
                SettingsSection::General,
                section,
                set_mode.clone(),
            ),
            settings_nav_button(
                "快捷键",
                Symbol::Keyboard,
                SettingsSection::Hotkey,
                section,
                set_mode.clone(),
            ),
            settings_nav_button(
                "数据",
                Symbol::SaveLocal,
                SettingsSection::Data,
                section,
                set_mode.clone(),
            ),
            settings_nav_button(
                "关于",
                Symbol::Help,
                SettingsSection::About,
                section,
                set_mode,
            ),
        ))
        .spacing(4.0),
    )
    .background(Color::rgb(236, 248, 248))
    .padding(Thickness::xy(10.0, 12.0))
    .into()
}

fn settings_content_panel(content: Element) -> Element {
    border(scroll_view(content))
        .background(Color::rgb(248, 250, 252))
        .border_brush(Color::rgb(200, 210, 224))
        .border_thickness(Thickness::uniform(1.0))
        .corner_radius(8.0)
        .padding(Thickness::uniform(14.0))
        .into()
}

fn settings_nav_button(
    label: &'static str,
    icon: Symbol,
    target: SettingsSection,
    current: SettingsSection,
    set_mode: SetState<UiMode>,
) -> Element {
    let text = label.to_string();
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
    let icon_text = char::from_u32(icon.0 as u32)
        .unwrap_or_default()
        .to_string();

    border(
        grid((
            border(text_block(""))
                .background(indicator_color)
                .corner_radius(2.0)
                .width(4.0)
                .height(24.0)
                .vertical_alignment(VerticalAlignment::Center)
                .grid_column(0),
            text_block(icon_text)
                .font_family("Segoe MDL2 Assets")
                .font_size(16.0)
                .horizontal_alignment(HorizontalAlignment::Center)
                .vertical_alignment(VerticalAlignment::Center)
                .grid_column(1),
            text_block(text)
                .font_size(14.0)
                .vertical_alignment(VerticalAlignment::Center)
                .grid_column(2),
        ))
        .columns([
            GridLength::Pixel(4.0),
            GridLength::Pixel(36.0),
            GridLength::STAR,
        ])
        .column_spacing(8.0),
    )
    .background(row_color)
    .corner_radius(6.0)
    .height(40.0)
    .on_tapped(move || set_mode.call(UiMode::Settings { section: target }))
    .width(240.0)
    .into()
}

fn setting_row(title: &'static str, description: impl Into<String>, control: Element) -> Element {
    let description = description.into();

    border(
        grid((
            vstack((
                text_block(title).font_size(14.0).bold(),
                text_block(description).font_size(12.0),
            ))
            .spacing(3.0)
            .grid_column(0),
            control.grid_column(1),
        ))
        .columns([GridLength::STAR, GridLength::Pixel(220.0)])
        .column_spacing(14.0),
    )
    .background(Color::rgb(255, 255, 255))
    .border_brush(Color::rgb(217, 225, 235))
    .border_thickness(Thickness::uniform(1.0))
    .corner_radius(8.0)
    .padding(Thickness::uniform(12.0))
    .into()
}

fn general_settings(
    config: LauncherConfig,
    config_path: PathBuf,
    set_message: SetState<String>,
    async_state: MutationState<AsyncUiResult>,
    async_trigger: MutationTrigger<AsyncUiResult>,
) -> Element {
    let settings = config.settings.clone();
    let shortcut_exists = windows_shortcut::desktop_shortcut_exists();
    let is_loading = async_state.is_loading();
    let shortcut_label = if shortcut_exists {
        "已存在"
    } else {
        "创建快捷方式"
    };
    let shortcut_button: Element = button(shortcut_label)
        .enabled(!shortcut_exists && !is_loading)
        .on_click(move || match windows_shortcut::create_desktop_shortcut() {
            Ok(()) => set_message.call("已创建桌面快捷方式".to_string()),
            Err(error) => set_message.call(error),
        })
        .into();

    vstack((
        text_block("常规").font_size(18.0).bold(),
        setting_row(
            "开机自启",
            "登录 Windows 后自动启动这个工具。",
            check_box(settings.launch_at_login)
                .content("开机自启")
                .enabled(!is_loading)
                .on_checked(save_settings(
                    config.clone(),
                    config_path.clone(),
                    async_trigger.clone(),
                    |settings, value| settings.launch_at_login = value,
                    "已更新开机自启",
                    true,
                ))
                .into(),
        ),
        setting_row(
            "启动窗口",
            "启动程序时是否直接显示主窗口。",
            check_box(settings.show_window_on_startup)
                .content("启动时显示主窗口")
                .enabled(!is_loading)
                .on_checked(save_settings(
                    config.clone(),
                    config_path.clone(),
                    async_trigger.clone(),
                    |settings, value| settings.show_window_on_startup = value,
                    "已更新启动显示设置",
                    false,
                ))
                .into(),
        ),
        setting_row(
            "关闭行为",
            "点关闭按钮时保留托盘后台入口。",
            check_box(settings.close_to_tray)
                .content("关闭窗口时最小化到托盘")
                .enabled(!is_loading)
                .on_checked(save_settings(
                    config,
                    config_path,
                    async_trigger,
                    |settings, value| settings.close_to_tray = value,
                    "已更新关闭行为",
                    false,
                ))
                .into(),
        ),
        setting_row("桌面快捷方式", "在桌面创建一个启动入口。", shortcut_button),
    ))
    .spacing(10.0)
    .into()
}

fn hotkey_settings(
    config: LauncherConfig,
    config_path: PathBuf,
    message: String,
    async_state: MutationState<AsyncUiResult>,
    async_trigger: MutationTrigger<AsyncUiResult>,
) -> Element {
    let settings = config.settings.clone();
    let is_loading = async_state.is_loading();
    let state: Element = if is_loading {
        ProgressRing::indeterminate().into()
    } else {
        text_block("").height(0.0).into()
    };

    vstack((
        text_block("快捷键").font_size(18.0).bold(),
        setting_row(
            "全局唤起",
            "在后台运行时使用快捷键打开窗口。",
            check_box(settings.global_hotkey_enabled)
                .content("启用全局唤起")
                .enabled(!is_loading)
                .on_checked(save_settings(
                    config,
                    config_path,
                    async_trigger,
                    |settings, value| settings.global_hotkey_enabled = value,
                    "已更新全局唤起设置",
                    false,
                ))
                .into(),
        ),
        setting_row(
            "快捷键",
            hotkey_status_message(&message)
                .unwrap_or_else(|| "第一版固定支持 Ctrl + Alt + Space。".to_string()),
            text_box(settings.global_hotkey)
                .header("快捷键")
                .placeholder_text("Ctrl + Alt + Space")
                .enabled(false && !is_loading)
                .into(),
        ),
        state,
    ))
    .spacing(10.0)
    .into()
}

fn hotkey_status_message(message: &str) -> Option<String> {
    message
        .contains("注册全局快捷键失败")
        .then(|| format!("快捷键冲突或注册失败：{message}"))
}

fn data_settings(config_path: PathBuf, set_message: SetState<String>) -> Element {
    let display_path = config_path.display().to_string();
    let open_path = config_path.clone();

    vstack((
        text_block("数据").font_size(18.0).bold(),
        setting_row("配置文件", display_path, text_block("").height(0.0).into()),
        setting_row(
            "配置目录",
            "用资源管理器打开配置文件所在目录。",
            button("打开位置")
                .on_click(move || {
                    let folder = open_path
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

                    match launcher::open_item(&item) {
                        Ok(()) => set_message.call("已打开配置位置".to_string()),
                        Err(error) => set_message.call(error),
                    }
                })
                .into(),
        ),
    ))
    .spacing(10.0)
    .into()
}

fn about_settings() -> Element {
    vstack((
        text_block("关于").font_size(18.0).bold(),
        setting_row(
            "Local Launcher",
            "本地快捷入口和导航工具。",
            text_block(format!("版本 {}", env!("CARGO_PKG_VERSION"))).into(),
        ),
        setting_row(
            "凭据策略",
            "第一版只保存账号名和备注。",
            text_block("不保存密码").into(),
        ),
    ))
    .spacing(10.0)
    .into()
}

fn editor_panel(
    config: LauncherConfig,
    set_config: SetState<LauncherConfig>,
    draft: DraftItem,
    set_draft: SetState<DraftItem>,
    config_path: PathBuf,
    set_message: SetState<String>,
    async_state: MutationState<AsyncUiResult>,
    async_trigger: MutationTrigger<AsyncUiResult>,
    set_mode: SetState<UiMode>,
    is_new: bool,
) -> Element {
    let kind_index = kind_index(draft.kind);
    let save_config = config.clone();
    let delete_config = config;

    border(
        vstack((
            hstack((
                text_block(if is_new {
                    "新增入口"
                } else {
                    "编辑入口"
                })
                .font_size(24.0)
                .bold()
                .width(760.0),
                button("取消").on_click({
                    let set_mode = set_mode.clone();
                    move || set_mode.call(UiMode::Browse)
                }),
            ))
            .spacing(12.0),
            text_box(draft.name.clone())
                .header("名称")
                .on_text_changed(update_draft(
                    draft.clone(),
                    set_draft.clone(),
                    |draft, value| {
                        draft.name = value;
                    },
                )),
            ComboBox::new(["目录", "程序", "外网", "内网"])
                .header("类型")
                .selected_index(kind_index)
                .on_selection_changed({
                    let set_draft = set_draft.clone();
                    let draft = draft.clone();
                    move |index| {
                        let mut next = draft.clone();
                        next.kind = kind_from_index(index);
                        next.icon_format = None;
                        next.icon_data = None;
                        set_draft.call(next);
                    }
                }),
            text_box(draft.target.clone())
                .header("目标")
                .on_text_changed(update_draft(
                    draft.clone(),
                    set_draft.clone(),
                    |draft, value| {
                        if draft.target != value {
                            draft.icon_format = None;
                            draft.icon_data = None;
                        }
                        draft.target = value;
                    },
                )),
            target_picker(draft.clone(), async_state.clone(), async_trigger.clone()),
            program_arguments_editor(draft.clone(), set_draft.clone()),
            text_box(draft.category.clone())
                .header("分类")
                .on_text_changed(update_draft(
                    draft.clone(),
                    set_draft.clone(),
                    |draft, value| {
                        draft.category = value;
                    },
                )),
            text_box(draft.tags.clone())
                .header("标签，逗号分隔")
                .on_text_changed(update_draft(
                    draft.clone(),
                    set_draft.clone(),
                    |draft, value| {
                        draft.tags = value;
                    },
                )),
            hstack((
                text_box(draft.username.clone())
                    .header("账号名")
                    .on_text_changed(update_draft(
                        draft.clone(),
                        set_draft.clone(),
                        |draft, value| {
                            draft.username = value;
                        },
                    ))
                    .width(300.0),
                check_box(draft.favorite).content("收藏").on_checked({
                    let set_draft = set_draft.clone();
                    let draft = draft.clone();
                    move |value| {
                        let mut next = draft.clone();
                        next.favorite = value;
                        set_draft.call(next);
                    }
                }),
            ))
            .spacing(12.0),
            text_box(draft.notes.clone())
                .header("备注")
                .on_text_changed(update_draft(
                    draft.clone(),
                    set_draft.clone(),
                    |draft, value| {
                        draft.notes = value;
                    },
                )),
            hstack((
                button("保存").accent().on_click({
                    let config_path = config_path.clone();
                    let set_config = set_config.clone();
                    let set_message = set_message.clone();
                    let set_mode = set_mode.clone();
                    let draft = draft.clone();
                    move || {
                        let item = draft.clone().into_item();
                        let mut next = save_config.clone();
                        next.items.retain(|existing| existing.id != item.id);
                        next.items.push(item);
                        match data_store::save(&config_path, &next) {
                            Ok(()) => {
                                set_config.call(next);
                                set_message.call("已保存".to_string());
                                set_mode.call(UiMode::Browse);
                            }
                            Err(error) => set_message.call(error),
                        }
                    }
                }),
                button("删除").enabled(!is_new).on_click({
                    let config_path = config_path.clone();
                    let set_config = set_config.clone();
                    let set_message = set_message.clone();
                    let set_mode = set_mode.clone();
                    let draft = draft.clone();
                    move || {
                        let mut next = delete_config.clone();
                        next.items.retain(|existing| existing.id != draft.id);
                        match data_store::save(&config_path, &next) {
                            Ok(()) => {
                                set_config.call(next);
                                set_message.call("已删除".to_string());
                                set_mode.call(UiMode::Browse);
                            }
                            Err(error) => set_message.call(error),
                        }
                    }
                }),
                button("取消").on_click(move || set_mode.call(UiMode::Browse)),
            ))
            .spacing(8.0),
        ))
        .spacing(10.0),
    )
    .background(ThemeRef::CardBackground)
    .border_brush(Color::rgb(205, 210, 216))
    .border_thickness(Thickness::uniform(1.0))
    .corner_radius(8.0)
    .padding(Thickness::uniform(16.0))
    .into()
}

fn save_settings(
    config: LauncherConfig,
    config_path: PathBuf,
    async_trigger: MutationTrigger<AsyncUiResult>,
    update: impl Fn(&mut LauncherSettings, bool) + Clone + Send + 'static,
    message: &'static str,
    update_startup_shortcut: bool,
) -> impl Fn(bool) + Clone + 'static {
    move |value| {
        let config = config.clone();
        let config_path = config_path.clone();
        let update = update.clone();
        async_trigger.fire(move || {
            update_settings(
                config,
                &config_path,
                value,
                update,
                message,
                update_startup_shortcut,
            )
        });
    }
}

fn update_settings(
    mut config: LauncherConfig,
    config_path: &PathBuf,
    value: bool,
    update: impl Fn(&mut LauncherSettings, bool),
    message: &str,
    update_startup_shortcut: bool,
) -> std::result::Result<AsyncUiResult, String> {
    let old_settings = config.settings.clone();
    let mut next_settings = config.settings.clone();
    if update_startup_shortcut {
        windows_shortcut::set_launch_at_login(value)?;
    }
    update(&mut next_settings, value);
    config.settings = next_settings.clone();
    if let Err(error) = crate::tray::configure(next_settings.clone()) {
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
        message: message.to_string(),
    })
}

fn target_picker(
    draft: DraftItem,
    async_state: MutationState<AsyncUiResult>,
    async_trigger: MutationTrigger<AsyncUiResult>,
) -> Element {
    if async_state.is_loading() {
        return ProgressRing::indeterminate().into();
    }

    match draft.kind {
        ItemKind::Folder => button("选择目录")
            .on_click(move || {
                let current = draft.clone();
                async_trigger.fire(move || match picker::pick_folder() {
                    Ok(Some(path)) => {
                        let mut next = current;
                        next.target = path;
                        next.icon_format = None;
                        next.icon_data = None;
                        Ok(AsyncUiResult::DraftUpdated {
                            draft: next,
                            message: "已选择目录".to_string(),
                        })
                    }
                    Ok(None) => Ok(AsyncUiResult::Message("已取消选择目录".to_string())),
                    Err(error) => Err(error),
                });
            })
            .into(),
        ItemKind::Program => button("选择程序")
            .on_click(move || {
                let current = draft.clone();
                async_trigger.fire(move || match picker::pick_program() {
                    Ok(Some(path)) => {
                        let mut next = current;
                        next.target = path;
                        if let Ok(icon) = icon_data::extract_program_icon(&next.target) {
                            next.icon_format = Some(icon.format);
                            next.icon_data = Some(icon.data);
                        } else {
                            next.icon_format = None;
                            next.icon_data = None;
                        }
                        Ok(AsyncUiResult::DraftUpdated {
                            draft: next,
                            message: "已选择程序".to_string(),
                        })
                    }
                    Ok(None) => Ok(AsyncUiResult::Message("已取消选择程序".to_string())),
                    Err(error) => Err(error),
                });
            })
            .into(),
        ItemKind::Url | ItemKind::IntranetUrl => hstack((
            text_block("请输入 http:// 或 https:// 开头的网址")
                .font_size(12.0)
                .width(300.0),
            button("获取标题").on_click(move || {
                let current = draft.clone();
                async_trigger.fire(move || match title_fetcher::fetch_title(&current.target) {
                    Ok(title) => {
                        let mut next = current;
                        next.name = title;
                        if let Ok(icon) = icon_data::fetch_url_icon(&next.kind, &next.target) {
                            next.icon_format = Some(icon.format);
                            next.icon_data = Some(icon.data);
                        }
                        Ok(AsyncUiResult::DraftUpdated {
                            draft: next,
                            message: "已获取网页标题".to_string(),
                        })
                    }
                    Err(error) => Err(error),
                });
            }),
        ))
        .spacing(8.0)
        .into(),
    }
}

fn program_arguments_editor(draft: DraftItem, set_draft: SetState<DraftItem>) -> Element {
    if draft.kind != ItemKind::Program {
        return text_block("").width(0.0).into();
    }

    text_box(draft.arguments.clone())
        .header("启动参数")
        .on_text_changed(update_draft(draft, set_draft, |draft, value| {
            draft.arguments = value;
        }))
        .into()
}

fn footer_bar(
    config: LauncherConfig,
    config_path: PathBuf,
    message: String,
    set_message: SetState<String>,
    async_state: MutationState<AsyncUiResult>,
    async_trigger: MutationTrigger<AsyncUiResult>,
) -> Element {
    let status = if message.is_empty() {
        credential::STATUS.to_string()
    } else {
        message
    };
    let count = config.items.len();
    let export_config = config.clone();
    let import_config = config;
    let import_control: Element = if async_state.is_loading() {
        ProgressRing::indeterminate().into()
    } else {
        button("导入")
            .on_click({
                let config_path = config_path.clone();
                move || {
                    let current = import_config.clone();
                    let config_path = config_path.clone();
                    async_trigger.fire(move || match picker::pick_json_file()? {
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
                    });
                }
            })
            .into()
    };

    border(
        grid((
            text_block(format!("{count} 个入口 · {status}")).grid_column(0),
            import_control.grid_column(1),
            button("导出")
                .on_click(move || {
                    match data_store::export_to(
                        &PathBuf::from("launcher-export.json"),
                        &export_config,
                    ) {
                        Ok(()) => set_message.call("已导出 launcher-export.json".to_string()),
                        Err(error) => set_message.call(error),
                    }
                })
                .grid_column(2),
        ))
        .columns([GridLength::STAR, GridLength::Auto, GridLength::Auto])
        .column_spacing(8.0),
    )
    .background(Color::rgb(229, 233, 239))
    .border_brush(Color::rgb(199, 208, 220))
    .border_thickness(Thickness::uniform(1.0))
    .padding(Thickness::xy(18.0, 10.0))
    .into()
}

fn update_draft(
    draft: DraftItem,
    set_draft: SetState<DraftItem>,
    update: impl Fn(&mut DraftItem, String) + Clone + 'static,
) -> impl Fn(String) + Clone + 'static {
    move |value| {
        let mut next = draft.clone();
        update(&mut next, value);
        set_draft.call(next);
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

fn program_argument_line(item: &LauncherItem) -> Element {
    if let Some(summary) = program_argument_summary(item) {
        text_block(summary).font_size(12.0).into()
    } else {
        text_block("").height(0.0).into()
    }
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

fn card_icon(item: &LauncherItem) -> Element {
    if let Some(uri) = icon_data::icon_uri(item) {
        return border(Image::new_with_uri(uri).width(30.0).height(30.0))
            .background(Color::rgb(248, 250, 252))
            .border_brush(Color::rgb(218, 223, 230))
            .border_thickness(Thickness::uniform(1.0))
            .corner_radius(6.0)
            .padding(Thickness::uniform(6.0))
            .width(74.0)
            .into();
    }

    border(text_block(kind_label(&item.kind)).bold().font_size(13.0))
        .background(kind_color(&item.kind))
        .corner_radius(6.0)
        .padding(Thickness::xy(10.0, 6.0))
        .width(74.0)
        .into()
}

fn kind_index(kind: ItemKind) -> i32 {
    match kind {
        ItemKind::Folder => 0,
        ItemKind::Program => 1,
        ItemKind::Url => 2,
        ItemKind::IntranetUrl => 3,
    }
}

fn kind_from_index(index: i32) -> ItemKind {
    match index {
        0 => ItemKind::Folder,
        1 => ItemKind::Program,
        3 => ItemKind::IntranetUrl,
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
        assert!(source.contains("button(\"⚙\")"));
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
        assert!(settings_nav_source(source).contains("Segoe MDL2 Assets"));
        assert!(settings_nav_source(source).contains("GridLength::Pixel(36.0)"));
        assert!(settings_nav_source(source).contains(".height(40.0)"));
        assert!(settings_nav_source(source).contains(".on_tapped("));
        assert!(!settings_nav_source(source).contains("button("));
        assert!(!settings_nav_source(source).contains(".icon(icon)"));
        assert!(!settings_nav_source(source).contains(".accent()"));
    }

    #[test]
    fn settings_mode_replaces_main_shell() {
        let source = production_source();
        let settings_gate = source
            .find("if let UiMode::Settings { section } = mode.clone()")
            .unwrap_or(usize::MAX);
        let main_header = source.find("app_header(query").unwrap_or_default();

        assert!(settings_gate < main_header);
        assert!(source.contains("return settings_view("));
        assert!(source.contains("unreachable!(\"settings mode returns before main shell\")"));
    }

    #[test]
    fn settings_controls_are_disabled_while_loading() {
        let source = production_source();

        assert!(source.contains("let is_loading = async_state.is_loading();"));
        assert!(source.contains(".enabled(!is_loading)"));
        assert!(source.contains(".enabled(!shortcut_exists && !is_loading)"));
    }

    #[test]
    fn hotkey_settings_show_registration_failure_inline() {
        let source = production_source();

        assert!(source.contains("fn hotkey_status_message("));
        assert!(source.contains("注册全局快捷键失败"));
        assert!(source.contains("hotkey_status_message(&message)"));
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

        assert!(edit_arm.contains("scroll_view("));
        assert!(edit_arm.contains("editor_panel("));
    }

    #[test]
    fn page_has_distinct_header_and_footer_sections() {
        let source = include_str!("ui.rs");
        let header_fn = concat!("fn ", "app_header(");
        let footer_fn = concat!("fn ", "footer_bar(");

        assert!(source.contains(header_fn));
        assert!(source.contains(footer_fn));
    }

    #[test]
    fn page_has_distinct_filter_toolbar() {
        let source = include_str!("ui.rs");
        let toolbar_fn = concat!("fn ", "filter_toolbar(");

        assert!(source.contains(toolbar_fn));
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
}
