// Copyright 2024-2026 OpenJC (https://www.openjc.cc)
// 
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// 
//     http://www.apache.org/licenses/LICENSE-2.0
// 
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! 原生菜单（macOS 系统菜单栏 + 跨平台托盘）双语构建
//! 作者：JC
//!
//! 设计要点：
//! - 菜单文案跟随语言（locale 形如 "zh-CN" / "en" / "zh-Hans" 等 BCP-47）
//! - `build_app_menu` 仅在 macOS 下设置系统菜单栏；其他平台返回 Ok 但不挂载
//! - `build_tray_menu` 跨平台托盘菜单同步 i18n
//! - 同一个菜单 id（如 "tools:settings"）在两个函数里保持一致，前端通过 menu:action 事件统一处理

// tauri::menu 为桌面专属模块（移动端 Android/iOS 不提供原生菜单），导入与作用其中的函数限定为 desktop
#[cfg(desktop)]
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
#[cfg(target_os = "macos")]
use tauri::menu::Submenu;
use tauri::{AppHandle, Runtime};

/// 判断 locale 是否属于中文（zh / zh-CN / zh-Hans / zh-Hant / zh-HK / zh-TW ...）
pub fn is_zh(locale: &str) -> bool {
    locale
        .split(['-', '_'])
        .next()
        .map(|s| s.eq_ignore_ascii_case("zh"))
        .unwrap_or(false)
}

/// 中英双语文案：构造时同时持有两种语言，运行时按 is_zh 选其一（&'static str，无运行时分配）
struct Mt {
    zh: &'static str,
    en: &'static str,
}

impl Mt {
    const fn new(zh: &'static str, en: &'static str) -> Self {
        Self { zh, en }
    }
    fn pick(&self, is_zh: bool) -> &'static str {
        if is_zh { self.zh } else { self.en }
    }
}

/// 品牌化双语文案：随 `pro-edition` feature 决定是否在文案中的 "JC Shell" 后追加后缀。
/// 菜单 API 只接受 &'static str，无法运行期拼接，故用编译期 concat!；
/// 与 common::constants::APP_NAME 同一品牌规则（前端见 utils/edition.ts）。
// 非 macOS 构建时 build_app_menu 整体被 cfg 掉，宏暂无使用者，屏蔽告警
#[allow(unused_macros)]
macro_rules! brand_mt {
    ($zh:literal, $en:literal) => {{
        #[cfg(not(feature = "pro-edition"))]
        {
            Mt::new($zh, $en)
        }
        #[cfg(feature = "pro-edition")]
        {
            Mt::new(concat!($zh, " Pro"), concat!($en, " Pro"))
        }
    }};
}

/// 构建并挂载 macOS 系统菜单栏菜单
/// 非 macOS 平台直接返回 Ok（菜单由前端 HTML TopMenuBar 提供）
pub fn build_app_menu<R: Runtime>(app: &AppHandle<R>, locale: &str) -> tauri::Result<()> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, locale);
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        let zh = is_zh(locale);

        // App 子菜单文案
        let m_about = brand_mt!("关于 JC Shell", "About JC Shell");
        let m_settings = Mt::new("设置…", "Settings…");
        let m_hide = brand_mt!("隐藏 JC Shell", "Hide JC Shell");
        let m_hide_others = Mt::new("隐藏其他", "Hide Others");
        let m_show_all = Mt::new("全部显示", "Show All");
        let m_quit = brand_mt!("退出 JC Shell", "Quit JC Shell");
        // File
        let m_file = Mt::new("文件", "File");
        let m_new_tab = Mt::new("新建标签", "New Tab");
        let m_close_tab = Mt::new("关闭标签", "Close Tab");
        // Edit
        let m_edit = Mt::new("编辑", "Edit");
        let m_clear = Mt::new("清除", "Clear");
        // View
        let m_view = Mt::new("视图", "View");
        let m_toggle_sidebar = Mt::new("切换侧边栏", "Toggle Sidebar");
        let m_toggle_file = Mt::new("切换文件面板", "Toggle File Panel");
        let m_toggle_device = Mt::new("切换设备面板", "Toggle Device Panel");
        let m_split_none = Mt::new("取消分屏", "Split None");
        let m_split_h = Mt::new("水平分屏", "Split Horizontal");
        let m_split_v = Mt::new("垂直分屏", "Split Vertical");
        let m_fullscreen = Mt::new("切换全屏", "Toggle Fullscreen");
        let m_zoom_in = Mt::new("放大", "Zoom In");
        let m_zoom_out = Mt::new("缩小", "Zoom Out");
        let m_zoom_reset = Mt::new("实际大小", "Actual Size");
        // Terminal
        let m_terminal = Mt::new("终端", "Terminal");
        let m_new_local = Mt::new("新建本地终端", "New Local Terminal");
        let m_new_ssh = Mt::new("新建 SSH 会话", "New SSH Session");
        let m_reconnect = Mt::new("重连", "Reconnect");
        let m_clear_screen = Mt::new("清屏", "Clear");
        let m_export_log = Mt::new("导出日志", "Export Log");
        // Tools
        let m_tools = Mt::new("工具", "Tools");
        let m_host_manager = Mt::new("主机管理", "Host Manager");
        let m_transfer_queue = Mt::new("传输队列", "Transfer Queue");
        let m_import_sessions = Mt::new("导入会话…", "Import Sessions…");
        let m_export_sessions = Mt::new("导出会话…", "Export Sessions…");
        // QQ 群（独立一级菜单，从「帮助」下挪出）
        let m_qq = Mt::new("QQ群", "QQ Groups");
        // Help
        let m_help = Mt::new("帮助", "Help");
        let m_shortcuts = Mt::new("键盘快捷键", "Keyboard Shortcuts");
        let m_qq_groups = Mt::new("QQ 群", "QQ Groups");
        let m_feedback = Mt::new("反馈", "Feedback");
        let m_check_update = Mt::new("检查更新", "Check for Updates");

        // App 子菜单（macOS 菜单栏最左侧，自动带 About / Hide / Quit 等）
        let app_submenu = Submenu::with_items(
            app,
            crate::common::constants::APP_NAME,
            true,
            &[
                &PredefinedMenuItem::about(app, Some(m_about.pick(zh)), None)?,
                &PredefinedMenuItem::separator(app)?,
                &MenuItem::with_id(
                    app,
                    "tools:settings",
                    m_settings.pick(zh),
                    true,
                    Some("CmdOrCtrl+,"),
                )?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::hide(app, Some(m_hide.pick(zh)))?,
                &PredefinedMenuItem::hide_others(app, Some(m_hide_others.pick(zh)))?,
                &PredefinedMenuItem::show_all(app, Some(m_show_all.pick(zh)))?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::quit(app, Some(m_quit.pick(zh)))?,
            ],
        )?;

        let file_submenu = Submenu::with_items(
            app,
            m_file.pick(zh),
            true,
            &[
                &MenuItem::with_id(
                    app,
                    "file:newTab",
                    m_new_tab.pick(zh),
                    true,
                    Some("CmdOrCtrl+T"),
                )?,
                &MenuItem::with_id(
                    app,
                    "file:closeTab",
                    m_close_tab.pick(zh),
                    true,
                    Some("CmdOrCtrl+W"),
                )?,
            ],
        )?;

        let edit_submenu = Submenu::with_items(
            app,
            m_edit.pick(zh),
            true,
            &[
                &PredefinedMenuItem::copy(app, None)?,
                &PredefinedMenuItem::paste(app, None)?,
                &PredefinedMenuItem::select_all(app, None)?,
                &PredefinedMenuItem::separator(app)?,
                &MenuItem::with_id(
                    app,
                    "edit:clear",
                    m_clear.pick(zh),
                    true,
                    Some("CmdOrCtrl+L"),
                )?,
            ],
        )?;

        let view_submenu = Submenu::with_items(
            app,
            m_view.pick(zh),
            true,
            &[
                &MenuItem::with_id(
                    app,
                    "view:toggleSidebar",
                    m_toggle_sidebar.pick(zh),
                    true,
                    Some("CmdOrCtrl+B"),
                )?,
                &MenuItem::with_id(
                    app,
                    "view:toggleFilePanel",
                    m_toggle_file.pick(zh),
                    true,
                    None::<&str>,
                )?,
                &MenuItem::with_id(
                    app,
                    "view:toggleDevicePanel",
                    m_toggle_device.pick(zh),
                    true,
                    None::<&str>,
                )?,
                &PredefinedMenuItem::separator(app)?,
                &MenuItem::with_id(
                    app,
                    "view:splitNone",
                    m_split_none.pick(zh),
                    true,
                    None::<&str>,
                )?,
                &MenuItem::with_id(
                    app,
                    "view:splitH",
                    m_split_h.pick(zh),
                    true,
                    None::<&str>,
                )?,
                &MenuItem::with_id(
                    app,
                    "view:splitV",
                    m_split_v.pick(zh),
                    true,
                    None::<&str>,
                )?,
                &PredefinedMenuItem::separator(app)?,
                &MenuItem::with_id(
                    app,
                    "view:fullscreen",
                    m_fullscreen.pick(zh),
                    true,
                    Some("F11"),
                )?,
                &MenuItem::with_id(
                    app,
                    "view:zoomIn",
                    m_zoom_in.pick(zh),
                    true,
                    Some("CmdOrCtrl+="),
                )?,
                &MenuItem::with_id(
                    app,
                    "view:zoomOut",
                    m_zoom_out.pick(zh),
                    true,
                    Some("CmdOrCtrl+-"),
                )?,
                &MenuItem::with_id(
                    app,
                    "view:zoomReset",
                    m_zoom_reset.pick(zh),
                    true,
                    None::<&str>,
                )?,
            ],
        )?;

        let terminal_submenu = Submenu::with_items(
            app,
            m_terminal.pick(zh),
            true,
            &[
                &MenuItem::with_id(
                    app,
                    "terminal:newLocal",
                    m_new_local.pick(zh),
                    true,
                    Some("CmdOrCtrl+Shift+T"),
                )?,
                &MenuItem::with_id(
                    app,
                    "terminal:newSsh",
                    m_new_ssh.pick(zh),
                    true,
                    None::<&str>,
                )?,
                &MenuItem::with_id(
                    app,
                    "terminal:reconnect",
                    m_reconnect.pick(zh),
                    true,
                    None::<&str>,
                )?,
                &PredefinedMenuItem::separator(app)?,
                &MenuItem::with_id(
                    app,
                    "terminal:clear",
                    m_clear_screen.pick(zh),
                    true,
                    Some("CmdOrCtrl+K"),
                )?,
                &MenuItem::with_id(
                    app,
                    "terminal:exportLog",
                    m_export_log.pick(zh),
                    true,
                    None::<&str>,
                )?,
            ],
        )?;

        let tools_submenu = Submenu::with_items(
            app,
            m_tools.pick(zh),
            true,
            &[
                &MenuItem::with_id(
                    app,
                    "tools:hostManager",
                    m_host_manager.pick(zh),
                    true,
                    None::<&str>,
                )?,
                &MenuItem::with_id(
                    app,
                    "tools:transferQueue",
                    m_transfer_queue.pick(zh),
                    true,
                    None::<&str>,
                )?,
                &PredefinedMenuItem::separator(app)?,
                &MenuItem::with_id(
                    app,
                    "tools:importSessions",
                    m_import_sessions.pick(zh),
                    true,
                    None::<&str>,
                )?,
                &MenuItem::with_id(
                    app,
                    "tools:exportSessions",
                    m_export_sessions.pick(zh),
                    true,
                    None::<&str>,
                )?,
            ],
        )?;

        let help_submenu = Submenu::with_items(
            app,
            m_help.pick(zh),
            true,
            &[
                &MenuItem::with_id(
                    app,
                    "help:shortcuts",
                    m_shortcuts.pick(zh),
                    true,
                    Some("CmdOrCtrl+/"),
                )?,
                &MenuItem::with_id(
                    app,
                    "help:feedback",
                    m_feedback.pick(zh),
                    true,
                    None::<&str>,
                )?,
                &PredefinedMenuItem::separator(app)?,
                &MenuItem::with_id(
                    app,
                    "help:checkUpdate",
                    m_check_update.pick(zh),
                    true,
                    None::<&str>,
                )?,
            ],
        )?;

        // QQ 群独立成一级菜单：macOS 顶级菜单必须是 Submenu 且至少含一项，
        // 故内部保留唯一的「社区QQ群」入口，id 沿用 help:qqGroups（与前端同源处理）。
        let qq_submenu = Submenu::with_items(
            app,
            m_qq.pick(zh),
            true,
            &[&MenuItem::with_id(
                app,
                "help:qqGroups",
                m_qq_groups.pick(zh),
                true,
                None::<&str>,
            )?],
        )?;

        let menu = Menu::with_items(
            app,
            &[
                &app_submenu,
                &file_submenu,
                &edit_submenu,
                &view_submenu,
                &terminal_submenu,
                &tools_submenu,
                &help_submenu,
                &qq_submenu,
            ],
        )?;
        app.set_menu(menu)?;
    }
    #[cfg(target_os = "macos")]
    Ok(())
}

/// 重建托盘菜单（跨平台，i18n 跟随 locale）
/// 注：调用方负责先 remove 旧 tray 再 build 新 tray；Tauri 2.x 不支持 in-place 替换 tray 菜单
/// 托盘为桌面专属功能，移动端不提供，故限定 desktop
#[cfg(desktop)]
pub fn build_tray_menu_items<R: Runtime>(
    app: &AppHandle<R>,
    locale: &str,
) -> tauri::Result<Menu<R>> {
    let zh = is_zh(locale);
    let show_i = MenuItem::with_id(
        app,
        "show_main",
        if zh { "打开主窗口" } else { "Open Main Window" },
        true,
        None::<&str>,
    )?;
    let settings_i = MenuItem::with_id(
        app,
        "open_settings",
        if zh { "设置" } else { "Settings" },
        true,
        None::<&str>,
    )?;
    let check_i = MenuItem::with_id(
        app,
        "check_update",
        if zh { "检查更新" } else { "Check for Updates" },
        true,
        None::<&str>,
    )?;
    let quit_i = MenuItem::with_id(
        app,
        "quit_app",
        if zh { "退出进程" } else { "Quit" },
        true,
        None::<&str>,
    )?;
    let sep = PredefinedMenuItem::separator(app)?;
    Menu::with_items(app, &[&show_i, &settings_i, &check_i, &sep, &quit_i])
}
