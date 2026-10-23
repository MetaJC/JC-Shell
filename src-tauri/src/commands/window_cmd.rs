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

//! 窗口控制命令（独立子窗口管理）
//! 作者：JC

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_opener::OpenerExt;

use crate::common::{AppError, AppResult, OAUTH_DEFAULT_SERVER_BASE_URL};
use crate::services::auth::OAuthClient;

/// 在系统默认浏览器/应用中打开一个外部 URL（用户协议、隐私政策等外链）。
///
/// 用 `tauri-plugin-opener` 实现：跨平台（含 Android/iOS）都能正确唤起系统浏览器，
/// 与登录授权页拉起浏览器走的是同一条已验证路径。不要改用 plugin-shell 的 `open`，
/// 它在移动端不可用，会抛异常。
#[tauri::command]
pub fn open_external_url(app: AppHandle, url: String) -> AppResult<()> {
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| AppError::Internal(format!("打开外部链接失败: {e}")))
}

/// 设置窗口切 tab 事件（主窗口触发，已开的设置窗口监听后切到目标 tab）
const SETTINGS_TAB_EVENT: &str = "settings:switch-tab";

/// 打开设置窗口（若已存在则显示并聚焦，若已隐藏则恢复显示）
/// <p>
/// 可选参数 `tab`：目标 tab（general / host / shortcut / about 等），
/// - 窗口不存在：拼到 URL `?tab=<tab>`，SettingsView 启动时读取并切换；
/// - 窗口已存在：emit `settings:switch-tab` 事件，已开的 SettingsView 监听后切换。
/// 打开/复用设置窗口（UI、Ctrl+,、托盘菜单等全部入口共用）。
/// tab: 目标分组（general / host ...），None 表示保持当前。
///
/// 关键：设置窗口的「置顶」状态跟随主窗口。
/// 主窗口被图钉固定（always_on_top）时，设置窗口也必须 always_on_top，
/// 否则普通窗口永远无法盖在置顶的主窗口之上，弹窗会被压在下面。
/// 主窗口未固定时则保持普通窗口（不改变原有行为）。
pub fn open_settings_window(app: &AppHandle, tab: Option<&str>) -> Result<(), String> {
    // 移动端兜底：Tauri Android 所有窗口渲染进同一 Activity 视图栈，任何辅助窗口
    // 都会永久置顶盖住主窗口。前端已在移动端走主窗口内设置弹窗（useSettingsOpener），
    // 这里拒绝创建，防止独立窗口再次遮挡。
    if cfg!(mobile) {
        let _ = (app, tab);
        return Err("移动端请在主窗口内使用设置面板".to_string());
    }
    // 主窗口是否处于置顶（图钉固定）状态
    let main_pinned = app
        .get_webview_window("main")
        .and_then(|w| w.is_always_on_top().ok())
        .unwrap_or(false);

    let tab_clean = tab
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    // 已存在：直接复用（先同步置顶，再 show + 聚焦到最前）
    if let Some(window) = app.get_webview_window("settings") {
        // set_always_on_top / center 为桌面专属 API，移动端跳过（always_on_top 在创建时已由 builder 设定）
        #[cfg(desktop)]
        let _ = window.set_always_on_top(main_pinned);
        let _ = window.show();
        let _ = window.set_focus();
        #[cfg(desktop)]
        let _ = window.center();
        if let Some(t) = tab_clean.as_ref() {
            // 已开窗口：通过事件驱动切 tab（比重启 webview 轻量）
            let _ = window.emit(SETTINGS_TAB_EVENT, t.clone());
        }
        return Ok(());
    }
    // 窗口不存在：通过 URL ?tab= 传递（SettingsView onMount 时读取）
    let url_path = match tab_clean {
        Some(t) => format!("/settings?tab={}", url_encode_segment(&t)),
        None => "/settings".to_string(),
    };
    let mut builder = WebviewWindowBuilder::new(
        app,
        "settings",
        WebviewUrl::App(url_path.into()),
    )
    .title(format!("{} - 设置", crate::common::constants::APP_NAME))
    .inner_size(980.0, 700.0)
    // 关闭 DevTools：产品态不允许调试窗口弹出；与 tauri.conf.json 的 devtools:false 保持一致
    .devtools(false);
    // 以下为桌面专属 API（always_on_top / center 在 Tauri 2 移动端被 #[cfg(desktop)] 关闭），移动端跳过；
    // always_on_top 让设置窗口跟随主窗口的图钉置顶状态，桌面行为不变。
    #[cfg(desktop)]
    {
        builder = builder.always_on_top(main_pinned).center();
    }
    match builder.build() {
        Ok(_) => {
            // Windows: debug 构建下 devtools:false 被忽略，经原生 WebView2 接口禁用
            #[cfg(target_os = "windows")]
            crate::disable_devtools_all(app);
            Ok(())
        }
        Err(e) => Err(format!("创建设置窗口失败: {e}")),
    }
}

#[tauri::command]
pub async fn open_settings(app: AppHandle, tab: Option<String>) -> Result<(), String> {
    open_settings_window(&app, tab.as_deref())
}

/// 打开主站反馈页（URL = server_base_url + /feedback?product=jc-shell）
/// 走 OAuthClient 拿运行时配置（DB > YAML > 编译常量），不再硬编码域名
/// <p>
/// 改为在 Tauri 内置 webview 窗口里打开（独立窗口 label=feedback），
/// 避免打开外部浏览器到主站 /feedback 路径（后端未部署该路径会 404）。
/// 已开窗口：show + focus + center。
#[tauri::command]
pub async fn open_feedback(
    app: AppHandle,
    oauth: tauri::State<'_, Arc<OAuthClient>>,
) -> AppResult<()> {
    let cfg = oauth.cfg_snapshot().await;
    let base = if cfg.server_base_url.trim().is_empty() {
        if OAUTH_DEFAULT_SERVER_BASE_URL.trim().is_empty() {
            return Err(AppError::Internal(
                "后端地址未配置 (server_base_url 为空). 请检查 .env 是否正确加载, 或在「设置 → 后端」中手动填写.".into(),
            ));
        }
        OAUTH_DEFAULT_SERVER_BASE_URL.to_string()
    } else {
        cfg.server_base_url
    };
    let url = format!(
        "{}/feedback?product={}",
        base.trim_end_matches('/'),
        crate::commands::qq_group_cmd::DEFAULT_PRODUCT_CODE
    );
    tracing::info!("[open-feedback] external_url={}", url);
    // 移动端：跳过内置窗口（同 settings —— 辅助窗口在 Android 上会永久置顶盖住主窗口），
    // 直接用系统浏览器打开反馈页
    if cfg!(mobile) {
        return OpenerExt::opener(&app)
            .open_url(&url, None::<&str>)
            .map_err(|e| AppError::Internal(format!("打开反馈页失败: {e}")));
    }
    // 1) 已存在 feedback 窗口：show + focus + center 后直接返回
    if let Some(window) = app.get_webview_window("feedback") {
        let _ = window.show();
        let _ = window.set_focus();
        // center() 为桌面专属 API，移动端跳过
        #[cfg(desktop)]
        let _ = window.center();
        return Ok(());
    }
    // 2) 优先创建内置 webview 窗口（路由 /feedback?product=...），URL 携带原始主站地址供前端读取
    let encoded = url_encode_segment(&url);
    let url_path = format!("/feedback?ext={}", encoded);
    let mut builder = WebviewWindowBuilder::new(
        &app,
        "feedback",
        WebviewUrl::App(url_path.into()),
    )
    .title(format!("{} - 提交反馈", crate::common::constants::APP_NAME))
    .inner_size(720.0, 640.0)
    .resizable(true)
    // 关闭 DevTools：产品态不允许调试窗口弹出
    .devtools(false);
    // center() 为桌面专属 API，移动端无此能力，跳过
    #[cfg(desktop)]
    {
        builder = builder.center();
    }
    match builder.build() {
        Ok(_) => {
            // 关闭 WebView2 DevTools（与 main / settings 同样的兜底逻辑）
            #[cfg(target_os = "windows")]
            if let Some(win) = app.get_webview_window("feedback") {
                let _ = win.with_webview(|webview| unsafe {
                    if let Ok(core) = webview.controller().CoreWebView2() {
                        if let Ok(settings) = core.Settings() {
                            let _ = settings.SetAreDevToolsEnabled(false);
                        }
                    }
                });
            }
            Ok(())
        }
        Err(e) => {
            // 3) 内置窗口创建失败（如 webview 路由尚未注册）回退到系统浏览器打开主站
            tracing::warn!("[open-feedback] 内置窗口创建失败，回退到系统浏览器: {e}");
            OpenerExt::opener(&app)
                .open_url(&url, None::<&str>)
                .map_err(|e2| AppError::Internal(format!("打开反馈页失败: {e2}")))?;
            Ok(())
        }
    }
}

/// 打开个人中心（系统浏览器打开主站 /profile 页面）
/// <p>
/// URL 携带一次性 `t=<access_token>` 参数，网页端验证后写入 localStorage 并立即清除 URL；
/// `menu` 可选，用于头像菜单直达指定面板（如 `invite` = 邀请得积分），仅放行已知 key，避免任意字符串拼进 URL；
/// 未登录时提示用户先登录。
#[tauri::command]
pub async fn open_profile(
    app: AppHandle,
    oauth: tauri::State<'_, Arc<OAuthClient>>,
    menu: Option<String>,
) -> AppResult<()> {
    let cfg = oauth.cfg_snapshot().await;
    let base = if cfg.server_base_url.trim().is_empty() {
        if OAUTH_DEFAULT_SERVER_BASE_URL.trim().is_empty() {
            return Err(AppError::Internal(
                "后端地址未配置 (server_base_url 为空). 请检查 .env 是否正确加载, 或在「设置 → 后端」中手动填写.".into(),
            ));
        }
        OAUTH_DEFAULT_SERVER_BASE_URL.to_string()
    } else {
        cfg.server_base_url
    };
    let token = oauth.access_token()?;
    const PROFILE_MENUS: [&str; 6] =
        ["account", "usage", "coupon", "invite", "orders", "subscription"];
    let menu_key = menu
        .as_deref()
        .map(str::trim)
        .filter(|m| PROFILE_MENUS.contains(m));
    let mut query: Vec<String> = Vec::new();
    let mut log_query: Vec<String> = Vec::new();
    if let Some(t) = token.as_deref() {
        query.push(format!("t={t}"));
        // 日志不能落 token 明文, 与旧版本一致打码
        log_query.push("t=***".to_string());
    }
    if let Some(m) = menu_key {
        query.push(format!("menu={m}"));
        log_query.push(format!("menu={m}"));
    }
    let root = base.trim_end_matches('/');
    let url = if query.is_empty() {
        format!("{root}/profile")
    } else {
        format!("{root}/profile?{}", query.join("&"))
    };
    let log_suffix = if log_query.is_empty() { String::new() } else { format!("?{}", log_query.join("&")) };
    tracing::info!("[open-profile] external_url={root}/profile{log_suffix}");
    OpenerExt::opener(&app)
        .open_url(&url, None::<&str>)
        .map_err(|e| AppError::Internal(format!("打开个人中心失败: {e}")))?;
    Ok(())
}

// ===== Tab 拖拽浮层（ghost）=====
// DOM 虚影画不出 WebView 边界，且鼠标移出窗口后页面收不到 mousemove/mouseup，
// 故虚影由原生小窗口承载（无边框/置顶/跳过任务栏/忽略鼠标事件），
// 位置与松手检测由 tab_ghost_poll 轮询全局光标驱动。

/// 拖拽浮层窗口 label
const TAB_GHOST_LABEL: &str = "tab-ghost";

/// 确保 tab-ghost 浮层窗口存在（不存在则创建，visible:false）。
/// 该窗口在应用启动时预创建，避免首次拖拽时 WebView2 冷启动导致虚影延迟出现。
fn ensure_tab_ghost_window(app: &AppHandle, default_title: &str) -> AppResult<tauri::WebviewWindow> {
    if let Some(w) = app.get_webview_window(TAB_GHOST_LABEL) {
        return Ok(w);
    }
    let url = format!("#/tab-ghost?title={}", url_encode_segment(default_title));
    let mut ghost = WebviewWindowBuilder::new(app, TAB_GHOST_LABEL, WebviewUrl::App(url.into()))
        .title("tab-drag-ghost")
        .inner_size(220.0, 32.0)
        .resizable(false)
        .visible(false)
        .devtools(false);
    // 以下原生窗框/任务栏属性均为桌面专属（Tauri 2 中 always_on_top / skip_taskbar /
    // decorations / maximizable / minimizable / shadow 均被 #[cfg(desktop)] 关闭），移动端无原生窗框，跳过。
    #[cfg(desktop)]
    {
        ghost = ghost
            .always_on_top(true)
            .skip_taskbar(true)
            .maximizable(false)
            .minimizable(false)
            .decorations(false)
            .shadow(true);
    }
    let ghost = ghost
        .build()
        .map_err(|e| AppError::Internal(format!("创建拖拽浮层失败: {e}")))?;
    // 浮层不拦截鼠标：光标事件穿透到下方窗口，位置完全由轮询驱动
    // set_ignore_cursor_events 为桌面专属 API（移动端无光标拦截概念），跳过
    #[cfg(desktop)]
    let _ = ghost.set_ignore_cursor_events(true);
    // Windows: debug 构建下 devtools:false 被忽略，原生接口兜底（与主窗口一致）
    #[cfg(target_os = "windows")]
    {
        let _ = ghost.with_webview(|webview| unsafe {
            if let Ok(core) = webview.controller().CoreWebView2() {
                if let Ok(settings) = core.Settings() {
                    let _ = settings.SetAreDevToolsEnabled(false);
                }
            }
        });
    }
    Ok(ghost)
}

/// 启动时预创建拖拽浮层（异步，避免阻塞 setup）
pub fn precreate_tab_ghost(app: &AppHandle) {
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = ensure_tab_ghost_window(&app_handle, "终端");
    });
}

/// 光标状态（屏幕物理坐标）
#[derive(serde::Serialize, Clone)]
pub struct GhostCursorState {
    x: i32,
    y: i32,
}

/// 拖拽代际号：新的 begin / cancel 会让旧的跟随循环自动退出（防止多循环并存）
static GHOST_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// 是否已吸附到 Tab 栏（吸附时虚影位置由 tab_ghost_snap 决定，不再跟随光标）
static GHOST_SNAPPED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// 当前吸附宿主窗口 label（空 = 未吸附）。跨窗口拖拽时由命中的目标窗口抢占，
/// 保证「只有真正命中的那个窗口」能设置/取消吸附，避免互相误取消。
static GHOST_SNAP_OWNER: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

/// 浮层主题色（源窗口实际生效的 CSS 变量值，保证虚影与界面同风格，避免默认白底）
#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct GhostTheme {
    pub bg: (u8, u8, u8),
    pub text: (u8, u8, u8),
    pub accent: (u8, u8, u8),
    pub success: (u8, u8, u8),
}

/// 松手事件（广播给所有窗口，各自判断是否命中自己的 Tab 栏/是否为拖拽源）
#[derive(serde::Serialize, Clone)]
pub struct GhostDropPayload {
    x: i32,
    y: i32,
    #[serde(rename = "sourceLabel")]
    source_label: String,
}

/**
 * 开始 Tab 拖拽：显示浮层并启动 Rust 端跟随循环（16ms/轮，约 60Hz）。
 *
 * 为什么不在前端轮询：前端 setInterval + invoke 每轮至少一次跨进程往返，
 * 频率受限于 IPC 且会被主线程阻塞拖慢，表现为虚影跟不上光标。
 * 跟随循环放在 Rust 端**专用线程**里（不是 tokio 运行时，避免饿死运行时工作线程），
 * 位置更新直接 set_position；只在「松手」和「位置变化且节流到 ~64ms」时才通知前端。
 *
 * 性能要点（改动前现场）：循环内每轮都 `DeviceState::new()`（8ms 一次）＋ 125Hz 的
 * 跨线程 `set_position`（每次都要等主线程处理完），叠加新窗口创建时会把主线程占满，
 * 表现为拖出 Tab 建窗口时整个应用定住。
 *
 * - source_label：拖出 Tab 的窗口 label（松手时把落点 emit 给它）
 * - width：浮层宽度（逻辑像素，按标题长度估算，贴近真实标签宽度）
 */
#[tauri::command]
pub async fn tab_ghost_begin(
    title: String,
    source_label: String,
    x: i32,
    y: i32,
    width: f64,
    theme: Option<GhostTheme>,
    app: AppHandle,
) -> AppResult<()> {
    // 移动端兜底：Tab 拖拽是桌面专属（依赖全局鼠标轮询），且 ghost 辅助窗口
    // 在 Android 上会永久置顶盖住主窗口。预创建已在 lib.rs 用 #[cfg(desktop)] 关闭，
    // 这里防触摸事件意外触发拖拽时动态创建。
    if cfg!(mobile) {
        let _ = (title, source_label, x, y, width, theme, app);
        return Err(AppError::Internal("移动端不支持 Tab 拖拽".into()));
    }
    let ghost = ensure_tab_ghost_window(&app, &title)?;
    let _ = ghost.emit("tab-ghost:title", title);
    // 主题：窗口底色取源窗口的 --jc-bg（否则 WebView2 默认白底），
    // 同时把文字/强调/成功色注入页面，保证虚影与界面完全同风格
    if let Some(t) = theme {
        let _ = ghost.set_background_color(Some(tauri::window::Color(t.bg.0, t.bg.1, t.bg.2, 255)));
        let _ = ghost.emit("tab-ghost:theme", t);
    }
    let _ = ghost.set_size(tauri::LogicalSize::new(width.max(90.0), 28.0));
    let _ = ghost.set_position(tauri::PhysicalPosition::new(x + 2, y + 2));
    let _ = ghost.show();

    let gen = GHOST_GEN.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
    GHOST_SNAPPED.store(false, std::sync::atomic::Ordering::SeqCst);
    if let Ok(mut owner) = GHOST_SNAP_OWNER.lock() {
        owner.clear();
    }
    // 广播拖拽开始：所有窗口收到后调用 measureGhostGeom() 测量自身 Tab 栏几何，
    // 供拖拽中的吸附（tab-ghost:move）与松手时的落点判定（onGhostDrop 的 barRect）使用。
    // ⚠️ 必须保留：缺少此广播会让 ghostGeom 恒为 null，导致松手时落点判定被跳过、无法拖出窗口。
    let _ = app.emit("tab-ghost:begin", ());
    // 跟随循环跑在**专用原生线程**里，而不是 tokio 运行时：
    // 1) 它本质是 60Hz 轮询，不需要异步运行时；放进 tokio 会长期占用工作线程，
    //    运行时被占满时其它 IPC 命令只能排队，界面表现为整体变卡；
    // 2) DeviceState 只构造一次并全程复用 —— 早期实现每轮 `DeviceState::new()`
    //    （8ms 一次）等于每秒上百次平台设备句柄创建/销毁，是拖拽卡顿的主要开销；
    // 3) 位置没变就不做窗口操作、不广播事件，鼠标静止时主线程完全不受打扰。
    // 移动端无多窗口拖拽场景，device_query 亦不可用，整个跟随线程跳过。
    #[cfg(desktop)]
    {
        let app2 = app.clone();
        std::thread::Builder::new()
            .name("jc-tab-ghost-follow".into())
            .spawn(move || {
                use device_query::{DeviceQuery, DeviceState};
                use std::sync::atomic::Ordering;
                let device = DeviceState::new();
                let mut ticks: u32 = 0;
                // 已下发的浮层位置 / 已广播的位置：用于去重，避免无谓的窗口操作与事件
                let mut applied: Option<(i32, i32)> = None;
                let mut broadcast: Option<(i32, i32)> = None;
                loop {
                    // 已被新的拖拽/取消取代 → 退出
                    if GHOST_GEN.load(Ordering::SeqCst) != gen {
                        break;
                    }
                    let mouse = device.get_mouse();
                    #[allow(unused_mut)]
                    let (mut x, mut y) = mouse.coords;
                    // macOS 上 device_query 返回逻辑点，换算为物理像素（Windows/X11 已是像素）
                    #[cfg(target_os = "macos")]
                    {
                        let scale = app2
                            .primary_monitor()
                            .ok()
                            .flatten()
                            .map(|m| m.scale_factor())
                            .unwrap_or(1.0);
                        x = (x as f64 * scale) as i32;
                        y = (y as f64 * scale) as i32;
                    }
                    // 左键抬起 → 清理吸附态、隐藏浮层，最后才广播落点。
                    // 顺序很重要：跟随线程必须先彻底停下（不再操作浮层窗口），
                    // 各窗口才会去创建/接管新窗口 —— 否则窗口操作会和 WebView2 的
                    // 窗口创建抢主线程，正是"拖出即卡死"的现场。
                    if !mouse.button_pressed.get(1).copied().unwrap_or(false) {
                        GHOST_SNAPPED.store(false, Ordering::SeqCst);
                        if let Ok(mut owner) = GHOST_SNAP_OWNER.lock() {
                            owner.clear();
                        }
                        if let Some(g) = app2.get_webview_window(TAB_GHOST_LABEL) {
                            let _ = g.hide();
                            let _ = g.emit("tab-ghost:snapped", false);
                        }
                        let _ = app2.emit(
                            "tab-ghost:drop",
                            GhostDropPayload {
                                x,
                                y,
                                source_label: source_label.clone(),
                            },
                        );
                        break;
                    }
                    // 未吸附时跟随光标（吸附位置由 tab_ghost_snap 设置）
                    if !GHOST_SNAPPED.load(Ordering::SeqCst) && applied != Some((x, y)) {
                        if let Some(g) = app2.get_webview_window(TAB_GHOST_LABEL) {
                            let _ = g.set_position(tauri::PhysicalPosition::new(x + 2, y + 2));
                        }
                        applied = Some((x, y));
                    }
                    // 位置事件（节流到每 4 轮 ≈ 64ms，避免高频事件挤占主线程）。
                    // 全局广播而非只发给源窗口：拖回其它窗口（如拖回主窗口）时，
                    // 目标窗口也要能判定落点并吸附到自己的 Tab 栏，实现双向吸附。
                    ticks = ticks.wrapping_add(1);
                    if ticks % 4 == 0 && broadcast != Some((x, y)) {
                        let _ = app2.emit("tab-ghost:move", GhostCursorState { x, y });
                        broadcast = Some((x, y));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(16));
                }
            })
            .map_err(|e| AppError::Internal(format!("启动拖拽跟随线程失败: {e}")))?;
    }
    Ok(())
}

/// 吸附：把浮层固定到调用方窗口 Tab 栏内的插入位置（屏幕物理坐标）
/// owner = 调用方窗口 label：只有「首个命中的窗口」或「当前宿主本人」能设置吸附
#[tauri::command]
pub async fn tab_ghost_snap(x: i32, y: i32, owner: String, app: AppHandle) -> AppResult<()> {
    {
        let Ok(mut cur) = GHOST_SNAP_OWNER.lock() else {
            return Ok(());
        };
        if !cur.is_empty() && *cur != owner {
            return Ok(()); // 已由其它窗口吸附，忽略本次
        }
        *cur = owner;
    }
    GHOST_SNAPPED.store(true, std::sync::atomic::Ordering::SeqCst);
    if let Some(g) = app.get_webview_window(TAB_GHOST_LABEL) {
        let _ = g.set_position(tauri::PhysicalPosition::new(x, y));
        let _ = g.emit("tab-ghost:snapped", true);
    }
    Ok(())
}

/// 取消吸附：仅当调用方是当前吸附宿主时才生效（避免误取消其它窗口的吸附）
#[tauri::command]
pub async fn tab_ghost_unsnap(owner: String, app: AppHandle) -> AppResult<()> {
    {
        let Ok(mut cur) = GHOST_SNAP_OWNER.lock() else {
            return Ok(());
        };
        if *cur != owner {
            return Ok(());
        }
        cur.clear();
    }
    GHOST_SNAPPED.store(false, std::sync::atomic::Ordering::SeqCst);
    if let Some(g) = app.get_webview_window(TAB_GHOST_LABEL) {
        let _ = g.emit("tab-ghost:snapped", false);
    }
    Ok(())
}

/// 结束拖拽（取消/异常兜底）：递增代际让循环退出并隐藏浮层
#[tauri::command]
pub async fn tab_ghost_cancel(app: AppHandle) -> AppResult<()> {
    use std::sync::atomic::Ordering;
    GHOST_GEN.fetch_add(1, Ordering::SeqCst);
    GHOST_SNAPPED.store(false, Ordering::SeqCst);
    if let Ok(mut owner) = GHOST_SNAP_OWNER.lock() {
        owner.clear();
    }
    if let Some(g) = app.get_webview_window(TAB_GHOST_LABEL) {
        let _ = g.hide();
        let _ = g.emit("tab-ghost:snapped", false);
    }
    Ok(())
}

/// 精简模式 → 原生窗口外观同步（仅 macOS 有实际效果）
///
/// macOS 原生三圆点是否显示取决于窗口有没有原生窗框：
/// 它们是 AppKit 的原生控件，绘制在 webview 之上，HTML 元素遮不住，只能开关窗框来显隐。
/// - 进入精简：set_decorations(false) → 窗口变无边框，三圆点随之消失；
/// - 退出精简：set_decorations(true) 恢复窗框；它会重置 styleMask 丢掉 Overlay 的
///   fullSizeContentView（内容被原生标题栏压下去），所以要再补一次 set_title_bar_style(Overlay)，
///   让三圆点回到标题栏左上角原位、内容重新铺满。
///
/// 退出时的补 Overlay 必须延后：tao 的 set_decorations 内部是异步的
/// （DispatchQueue.main.exec_async），它排队时会用不含 FullSizeContentView 的旧 mask 覆盖。
/// 若同步紧跟着补 Overlay，会被随后落地的这次覆盖抹掉，所以等 styleMask 生效后再补。
///
/// 「其余任何情况都要显示三圆点」：普通模式 / 全屏 / 设置窗口都不会调用本命令，
/// 窗口维持 tauri.conf.json 里 decorations + Overlay 的配置。
#[tauri::command]
pub fn sync_zen_mode(window: tauri::Window, zen: bool) -> AppResult<()> {
    #[cfg(target_os = "macos")]
    {
        let _ = window.set_decorations(!zen);
        // 无边框默认丢失窗口阴影，补回来保持视觉一致
        let _ = window.set_shadow(true);
        if !zen {
            let delayed = window.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(60));
                let _ = delayed.set_title_bar_style(tauri::TitleBarStyle::Overlay);
            });
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (window, zen);
    Ok(())
}

/// 轻量 URL 段编码（仅白名单字符直通；避免引号/尖括号污染 URL）
fn url_encode_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'~' | b'.' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}