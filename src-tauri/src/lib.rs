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

//! JC Shell —— Rust 后端入口（Tauri 应用组装）
//! 作者：JC

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::Emitter;

pub mod common;
pub mod models;
pub mod commands;
pub mod services;
pub mod repositories;
pub mod menu;
pub mod locale_state;

use std::sync::Arc;

use once_cell::sync::OnceCell;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::SqlitePool;
use std::time::Duration;
use tauri::Manager;

#[cfg(windows)]
use std::os::windows::ffi::OsStringExt;

use crate::commands::port_forward_cmd::PortForwardManager;
use crate::commands::terminal_cmd::TerminalSessionManager;
use crate::services::update::UpdateService;
use crate::commands::update_cmd::UpdateServiceState;
use crate::services::telemetry::TelemetryService;
use crate::common::{AppError, AppResult, DB_FILE_NAME, OAUTH_DEFAULT_SERVER_BASE_URL};
use crate::repositories::setting_repo::SettingRepository;
use crate::services::auth::{AuthClientConfig, OAuthClient};
use crate::services::config::{AuthConfig, ConfigService, YamlConfigService};

/// 全局共享状态（通过 tauri State 获取）
pub struct AppState {
    pub db: SqlitePool,
}

// ===== Windows 任务栏图标（独立于窗口左上角图标） =====
// Windows 下标题栏角标用 ICON_SMALL、任务栏按钮用 ICON_BIG，可用 WM_SETICON 分别设置。
// 这里只覆盖 ICON_BIG（任务栏），左上角 ICON_SMALL 保持 Tauri 默认 icon.ico 不变。
#[cfg(windows)]
mod taskbar_icon {
    use std::ffi::c_void;

    const WM_SETICON: u32 = 0x0080;
    const ICON_BIG: usize = 1;
    const LR_DEFAULTCOLOR: u32 = 0;
    const VERSION_32: u32 = 0x0003_0000;

    #[link(name = "user32")]
    extern "system" {
        fn CreateIconFromResourceEx(
            presbits: *const u8,
            dw_res_size: u32,
            icon: i32,
            version: u32,
            cx_desired: i32,
            cy_desired: i32,
            flags: u32,
        ) -> *mut c_void;
        fn SendMessageW(hwnd: *mut c_void, msg: u32, wparam: usize, lparam: isize) -> isize;
    }

    /// 编译期嵌入的任务栏专用图标（多尺寸 PNG 压缩 ICO）
    static TASKBAR_ICO: &[u8] = include_bytes!("../icons/taskbar.ico");

    /// 设置窗口任务栏图标（不影响标题栏左上角图标）
    pub fn apply(window: &tauri::WebviewWindow) {
        let Ok(hwnd) = window.hwnd() else {
            tracing::warn!("获取窗口句柄失败，跳过任务栏图标设置");
            return;
        };
        // 任务栏标准尺寸 32x32，系统按 DPI 自动选择 ico 内更高分辨率条目
        let hicon = unsafe {
            CreateIconFromResourceEx(
                TASKBAR_ICO.as_ptr(),
                TASKBAR_ICO.len() as u32,
                1,
                VERSION_32,
                32,
                32,
                LR_DEFAULTCOLOR,
            )
        };
        if hicon.is_null() {
            tracing::warn!("任务栏图标创建失败");
            return;
        }
        unsafe {
            SendMessageW(hwnd.0, WM_SETICON, ICON_BIG, hicon as isize);
        }
        tracing::info!("任务栏图标已设置");
    }
}

static APP_STATE: OnceCell<Arc<AppState>> = OnceCell::new();

/// 取 AppState 静态引用（非 Tauri 上下文场景）
pub fn app_state() -> Option<&'static Arc<AppState>> {
    APP_STATE.get()
}


/// Windows：将含空格/特殊字符的长路径转换为 8.3 短路径（如 "JC Shell" -> "JC~1"）
/// 彻底规避 sqlx 0.8 内部把 Path 转 URL 时因空格截断触发的 SQLITE_CANTOPEN (code 14)
#[cfg(windows)]
unsafe fn get_short_path_w(long: &std::path::Path) -> Option<std::path::PathBuf> {
    use std::os::windows::ffi::OsStrExt;
    extern "system" {
        fn GetShortPathNameW(
            lpszLongPath: *const u16,
            lpszShortPath: *mut u16,
            cchBuffer: u32,
        ) -> u32;
    }
    let mut long_wide: Vec<u16> = long.as_os_str().encode_wide().collect();
    long_wide.push(0);
    // 第一轮取所需缓冲大小
    let need = unsafe { GetShortPathNameW(long_wide.as_ptr(), std::ptr::null_mut(), 0) };
    if need == 0 {
        return None;
    }
    let mut buf = vec![0u16; need as usize];
    let wrote = unsafe { GetShortPathNameW(long_wide.as_ptr(), buf.as_mut_ptr(), need) };
    if wrote == 0 || wrote >= need {
        return None;
    }
    let len = wrote as usize;
    buf.truncate(len);
    Some(std::path::PathBuf::from(std::ffi::OsString::from_wide(&buf)))
}

#[cfg(not(windows))]
unsafe fn get_short_path_w(long: &std::path::Path) -> Option<std::path::PathBuf> {
    Some(long.to_path_buf())
}

/// 创建数据目录 + SQLite 连接池
/// 集成 master key（OS KeyChain 存）+ DB 完整性 HMAC 校验：
/// 1. 从 KeyChain 读 master key，没有则生成（另有数据目录内文件备份兜底）
/// 2. 用 master key 算 HMAC，绑定 DB 路径，验证或创建 .hmac 标签文件
/// 3. 打开 SQLite 连接池；密文不可读时从旧位置 DB 兜底恢复
async fn init_db() -> AppResult<SqlitePool> {
    // 数据目录：%AppData%\JC Shell —— 必须与安装目录（%LocalAppData%\JC Shell）隔离，
    // 否则 NSIS 卸载器清安装目录时会连带删掉用户数据（重装后“会话丢失”）
    let app_dir = crate::common::paths::app_data_dir();
    std::fs::create_dir_all(&app_dir)?;
    // 存量迁移：历史版本数据落在安装目录里，首次启动搬到新目录（含 WAL 旁路文件）
    crate::common::paths::migrate_legacy_data(&app_dir);

    // 1) master key：KeyChain → 数据目录内备份文件 → 新生成。
    // 备份文件必须存在：Windows 凭据管理器条目可能丢失，仅靠 KeyChain 会导致全库永久不可读。
    let master_key_hex =
        crate::services::keystore::db_key::get_or_create_db_master_key(&app_dir.join(KEY_FILE_NAME))?;

    let db_path = app_dir.join(DB_FILE_NAME);
    // Windows：先取 JC Shell 的 8.3 短路径（JC~1 无空格），绕开 sqlx 0.8 内部 URL 化时空格触发 SQLITE_CANTOPEN
    let effective_path = {
        let p = unsafe { get_short_path_w(&db_path) };
        p.unwrap_or_else(|| db_path.clone())
    };

    // 2) DB 完整性校验：用 master key 算 HMAC 绑定到实际打开路径
    crate::services::keystore::db_hmac::verify_or_create_hmac(&master_key_hex, &effective_path)?;

    let opts = SqliteConnectOptions::new()
        .filename(&effective_path)
        .create_if_missing(true)
        .busy_timeout(Duration::from_secs(8))
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(10)
        .connect_with(opts)
        .await
        .map_err(|e| {
            AppError::Ssh(format!(
                "数据库连接失败 (路径: {} -> {}): {e}",
                db_path.display(),
                effective_path.display()
            ))
        })?;

    // 建表（首次启动自动初始化）
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS app_setting (
            key TEXT PRIMARY KEY NOT NULL,
            value TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )"#,
    )
    .execute(&pool)
    .await?;

    // 注册 master key 供 repo 层透明加解密使用；并把历史版本残留的明文设置值补加密，
    // 保证 DB 文件被第三方工具打开时读不到任何明文用户数据。
    // 注意：先做“不可读兜底恢复”再补加密——若 key 与库内密文不匹配，
    // 需先从旧位置 DB 恢复数据，避免用户数据永久不可读。
    crate::services::keystore::db_crypto::register_master_key(master_key_hex.clone());
    recover_unreadable_settings(&pool).await?;
    migrate_plaintext_settings(&pool).await?;

    Ok(pool)
}

/// master key 备份文件名（与 DB 同目录，隐藏属性）
const KEY_FILE_NAME: &str = "jcshell.key";

/// 兜底恢复：当前 key 解不开库内密文（key 丢失后重新生成过）时，
/// 从旧位置（%LocalAppData%\JC Shell）的存量 DB 重新导入全部设置。
/// 触发条件严格：必须库内存在 JCENC 密文且解密失败才执行，正常启动零开销。
async fn recover_unreadable_settings(pool: &SqlitePool) -> AppResult<()> {
    use sqlx::Row;
    let rows = sqlx::query("SELECT key, value FROM app_setting")
        .fetch_all(pool)
        .await?;
    let unreadable = rows.iter().any(|r| {
        let v: String = r.get("value");
        crate::services::keystore::db_crypto::is_encrypted(&v)
            && crate::services::keystore::db_crypto::decrypt_value(&v)
                .ok()
                .flatten()
                .is_none()
    });
    if !unreadable {
        return Ok(());
    }
    tracing::warn!("检测到设置密文与当前 master key 不匹配，尝试从旧位置数据库恢复");

    let legacy_db = crate::common::paths::legacy_data_dir().join(DB_FILE_NAME);
    if !legacy_db.exists() {
        tracing::warn!("旧位置无可用数据库（{}），无法恢复", legacy_db.display());
        return Ok(());
    }
    let legacy_path = unsafe { get_short_path_w(&legacy_db) }.unwrap_or(legacy_db.clone());
    let legacy_opts = SqliteConnectOptions::new()
        .filename(&legacy_path)
        .read_only(true)
        .busy_timeout(Duration::from_secs(3));
    let Ok(legacy_pool) = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(legacy_opts)
        .await
    else {
        tracing::warn!("旧位置数据库打开失败，无法恢复");
        return Ok(());
    };
    // 旧库可能没有 app_setting 表（更早期版本），失败则放弃恢复
    let Ok(legacy_rows) = sqlx::query("SELECT key, value, updated_at FROM app_setting")
        .fetch_all(&legacy_pool)
        .await
    else {
        tracing::warn!("旧位置数据库无 app_setting 表，无法恢复");
        return Ok(());
    };
    legacy_pool.close().await;
    if legacy_rows.is_empty() {
        return Ok(());
    }
    let mut restored = 0usize;
    for r in legacy_rows {
        let key: String = r.get("key");
        let raw: String = r.get("value");
        let updated_at: String = r.get("updated_at");
        // 旧值可能是明文或前端 enc1: 密文，统一用当前 key 重新加密落盘，保留原时间戳；
        // 单条失败跳过不影响其余数据恢复。
        let Ok(enc) = crate::services::keystore::db_crypto::encrypt_value(&raw) else {
            continue;
        };
        let ok = sqlx::query(
            r#"INSERT INTO app_setting (key, value, updated_at) VALUES (?, ?, ?)
               ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at"#,
        )
        .bind(&key)
        .bind(&enc)
        .bind(&updated_at)
        .execute(pool)
        .await
        .is_ok();
        if ok {
            restored += 1;
        }
    }
    tracing::info!("已从旧位置数据库恢复 {restored} 条设置");
    Ok(())
}

/// 一次性迁移：把 app_setting 中未加密的历史明文值重写为密文（幂等，全部已加密时为空操作）
async fn migrate_plaintext_settings(pool: &SqlitePool) -> AppResult<()> {
    use sqlx::Row;
    let rows = sqlx::query("SELECT key, value FROM app_setting")
        .fetch_all(pool)
        .await?;
    for r in rows {
        let key: String = r.get("key");
        let value: String = r.get("value");
        if crate::services::keystore::db_crypto::is_encrypted(&value) {
            continue;
        }
        let enc = crate::services::keystore::db_crypto::encrypt_value(&value)?;
        sqlx::query("UPDATE app_setting SET value = ? WHERE key = ?")
            .bind(&enc)
            .bind(&key)
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// 从 app_setting 表加载持久化的 OAuth 配置 (不存在则保留默认值)
async fn load_oauth_config(db: &SqlitePool) -> Option<AuthClientConfig> {
    let repo = SettingRepository::new(db);
    let row = repo.get("oauth_config").await.ok().flatten()?;
    let json = row.value;
    serde_json::from_str::<AuthClientConfig>(&json).ok()
}

/// 把 YAML auth 节点中非空字段合并到编译默认值
fn merge_auth_into_default(auth: &AuthConfig) -> AuthClientConfig {
    use crate::common::{OAUTH_CLIENT_ID, OAUTH_DEFAULT_SCOPES, OAUTH_DEFAULT_SERVER_BASE_URL};
    AuthClientConfig {
        server_base_url: auth
            .server_base_url
            .clone()
            .unwrap_or_else(|| OAUTH_DEFAULT_SERVER_BASE_URL.to_string()),
        client_id: auth
            .client_id
            .clone()
            .unwrap_or_else(|| OAUTH_CLIENT_ID.to_string()),
        scopes: auth
            .scopes
            .clone()
            .unwrap_or_else(|| OAUTH_DEFAULT_SCOPES.to_string()),
    }
}

fn is_some_auth(a: &AuthConfig) -> bool {
    a.server_base_url.is_some() || a.client_id.is_some() || a.scopes.is_some()
}

/// 关闭所有 WebView2 窗口的 DevTools（Ctrl+Shift+I / F12 / 右键检查元素 全部失效）。
/// 仅 Windows 下有效；窗口创建处也应调用它作为即时兜底。
#[cfg(target_os = "windows")]
pub(crate) fn disable_devtools_all(app: &tauri::AppHandle) {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings3;
    for (_label, win) in app.webview_windows() {
        let _ = win.with_webview(|webview| unsafe {
            if let Ok(core) = webview.controller().CoreWebView2() {
                if let Ok(settings) = core.Settings() {
                    // 1) 关 DevTools 本体
                    let _ = settings.SetAreDevToolsEnabled(false);
                    // 2) 再关「浏览器加速键」：部分 WebView2 版本即便关了 DevTools，
                    //    Ctrl+Shift+I / F12 仍会唤出调试窗口，必须一并禁用这些加速键。
                    //    （不影响 Ctrl+C/V/X/A 等由网页内容处理的编辑快捷键）
                    if let Ok(s3) = windows_core::Interface::cast::<ICoreWebView2Settings3>(&settings) {
                        let _ = s3.SetAreBrowserAcceleratorKeysEnabled(false);
                    }
                }
            }
        });
    }
}

// ===== 外部扩展点（由依赖本 crate 的外部工程使用；社区版走 run()，行为不变）=====

/// 外部扩展命令统一前缀：社区版命令永不使用该前缀。
/// 分发闭包据此把 `pro_*` 命令路由给外部扩展处理器，其余命令走社区版命令表。
pub const PRO_COMMAND_PREFIX: &str = "pro_";

/// 外部扩展点，由依赖本 crate 的外部工程构造并传入 [`run_with_ext`]。
///
/// 社区版使用 [`run`]（传 `ProExtension::default()`），不注册任何外部扩展能力。
#[derive(Default)]
pub struct ProExtension {
    /// 外部扩展命令处理器（`tauri::generate_handler!` 生成的闭包）。
    /// 只接收 `pro_` 前缀的命令；返回 false 表示未处理（正常不会发生）。
    pub command_handler:
        Option<Box<dyn Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync>>,
    /// Builder 定制回调：在社区版插件注册之后、`.setup()` 之前应用，
    /// 供外部扩展注册自己的插件。
    pub customize_builder:
        Option<Box<dyn FnOnce(tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry>>>,
}

/// 为 `tauri::generate_handler!` 生成的命令表闭包提供期望类型。
///
/// 背景：`invoke_handler` 要求 `Fn(Invoke<Wry>) -> bool`，命令表直接作为实参传入时
/// 闭包签名由该约束推断；提取为 `let` 绑定后失去期望类型会报 E0282，
/// 故用本函数在调用点把期望类型「还」给闭包。
#[inline]
fn as_command_table<F>(f: F) -> F
where
    F: Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static,
{
    f
}

/// Tauri 运行时初始化（社区版入口，无外部扩展）
///
/// `tauri::mobile_entry_point`：移动端（Android/iOS）必需，由该宏生成 JNI/入口符号；
/// 缺少它会导致安卓构建报 "does not include required runtime symbols"。
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    run_with_ext(ProExtension::default());
}

/// Tauri 运行时初始化（带外部扩展，由依赖本 crate 的外部工程使用）
///
/// 社区版全部逻辑原样保留，仅在两处插入扩展点：
/// 1. `customize_builder`：社区版插件注册后、setup 前应用；
/// 2. `command_handler`：`pro_` 前缀命令交给外部扩展处理器，其余走社区版命令表。
pub fn run_with_ext(ext: ProExtension) {
    run_with_ext_ctx(ext, tauri::generate_context!());
}

/// [`run_with_ext`] 的 Context 可注入版本（需要自定义 context 的构建目标必须用这个）。
///
/// `generate_context!()` 按「宏展开所在 crate」定位 tauri.conf.json：它若只在
/// 社区版 crate 内展开，烙进二进制的便是社区版的窗口标题、devUrl 与前端资产
/// 快照；需要自定义窗口标题 / devUrl / 前端资产的构建目标，必须在自己的 lib.rs
/// 调用本函数并传入自己工程展开的 context。
pub fn run_with_ext_ctx(ext: ProExtension, context: tauri::Context<tauri::Wry>) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "jc_shell_lib=info,warn".into()),
        )
        .try_init();

    // OAuthClient 默认配置初始化 (DB 初始化后异步从 setting 表覆盖)
    let oauth_client = Arc::new(OAuthClient::new(AuthClientConfig::default()));

    // 启动期强校验: 编译期默认 server_base_url 不能为空 (空=构建时 OPENJC_SERVER_BASE_URL 未注入)
    if crate::common::OAUTH_DEFAULT_SERVER_BASE_URL.trim().is_empty() {
        tracing::error!(
            "[bootstrap] OAUTH_DEFAULT_SERVER_BASE_URL is empty! 构建期环境变量 OPENJC_SERVER_BASE_URL 未注入. \
请用 `npm run tauri:build:dev` 或 `npm run tauri:build:prod` 构建 (它们会通过 with-env.mjs 加载 .env), \
不要直接用 `cargo tauri build` / `npx tauri build` 跳过环境加载."
        );
    } else {
        tracing::info!(
            "[bootstrap] OAUTH_DEFAULT_SERVER_BASE_URL = {}",
            crate::common::OAUTH_DEFAULT_SERVER_BASE_URL
        );
    }

    // WebView2 用户数据目录：固定持久化目录（%LocalAppData%/jcshell-webview2）
    // 历史版本每次启动创建随机临时目录（%TEMP%/jcshell_webview2_*），导致
    // localStorage 全量丢失 —— 设置、登录态、界面偏好等每次重启全部还原默认，
    // 用户开启的"最小化到托盘"等选项重启后即失效。此处改为固定目录保证跨启动持久化。
    // 目录名刻意避开安装目录（%LocalAppData%\JC Shell），否则卸载时 localStorage 被连带清除；
    // 旧目录数据首次启动自动迁移。
    // 注：Tauri 2 无 Builder::webview_user_data_dir；改用 WebView2 Loader 官方识别
    // 的环境变量 WEBVIEW2_USER_DATA_FOLDER。SingletonLock 文件锁由 OS 管理，进程退出
    // 自动释放，残留文件无害。目录被占用时第二个实例 WebView2 初始化会报
    // ERROR_BUSY (0x800700AA) 产生空白无响应窗口，故下方已启用
    // tauri-plugin-single-instance 保证全局仅一个实例（重复启动直接激活已有窗口）。
    crate::common::paths::migrate_legacy_webview2();
    let webview2_dir = crate::common::paths::webview2_data_dir();
    let _ = std::fs::create_dir_all(&webview2_dir);
    // 清理历史版本遗留的随机临时目录（%TEMP%/jcshell_webview2_*），避免磁盘堆积；
    // 含 EBWebView 数据的目录视为可能有进程使用，跳过不删。
    if let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.starts_with("jcshell_webview2_") {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            if !meta.is_dir() || entry.path().join("EBWebView").exists() {
                continue;
            }
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
    std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", &webview2_dir);

    let builder = tauri::Builder::default();
    // 单实例保护：重复启动时不新建进程窗口（避免 WebView2 数据目录冲突导致的空白窗口），
    // 而是激活已有实例：显示 + 取消最小化 + 聚焦主窗口。
    // 这也正是 deep link 的落地效果：浏览器点 jc-shell:// 会启动一个新进程，
    // 该进程被这里拦下并激活已有主窗口后退出，用户看到的就是「客户端被唤起」。
    // 注：启用 deep-link feature 后，新进程 argv 里的协议 URL 由插件自动转发给已有实例，
    // 本回调无需解析 URL 内容。
    // 移动端（iOS/Android）单实例语义不适用，跳过注册。
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.show();
            let _ = w.unminimize();
            let _ = w.set_focus();
        }
    }));
    // 生物识别解锁（指纹 / 人脸）：仅移动端接入。桌面端没有该 crate、也没有生物识别桥，
    // 前端的能力开关同样为 false，解锁路径仍只有 PIN —— PC 行为完全不变。
    #[cfg(mobile)]
    let builder = builder.plugin(tauri_plugin_biometric::init());
    let builder = builder
        // 自定义 URL scheme（jc-shell://）：授权页授权成功后通过该协议唤起本应用。
        // 必须注册在 single-instance 之后。
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init());
    // 原生菜单点击事件：把菜单项 id 转发到前端，由前端复用 TopMenuBar 的 onItemClick 逻辑。
    // 仅桌面端有原生菜单；移动端无菜单，on_menu_event 为桌面专属 API，跳过。
    #[cfg(desktop)]
    let builder = builder.on_menu_event(|app, event| {
        let id = event.id.as_ref().to_string();
        let _ = app.emit_to("main", "menu:action", id);
    });
    // ===== 外部扩展点 1：builder 定制（社区版插件注册后、setup 前应用）=====
    let builder = match ext.customize_builder {
        Some(customize) => customize(builder),
        None => builder,
    };
    builder
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(move |app| {
            // Android：dirs::config_dir() 返回 None，paths::app_data_dir() 会回退到
            // CWD（"/"，只读），SQLite/配置写入全部报 "Read-only file system"。
            // 在 setup 最早期把 Tauri 解析出的应用私有数据目录
            // （/data/data/<pkg>/files）注入为全局覆盖，后续所有
            // app_data_dir() 调用方（DB / config.yml / ssh-keys / updates）自动生效。
            // 必须先于 init_db 的 spawn 执行（同闭包内同步顺序保证 happens-before）。
            #[cfg(target_os = "android")]
            {
                if let Ok(dir) = app.path().app_data_dir() {
                    let data_dir = dir.join(crate::common::APP_DIR_NAME);
                    if let Err(e) = std::fs::create_dir_all(&data_dir) {
                        tracing::error!("[bootstrap] Android: 创建数据目录失败 {}: {e}", data_dir.display());
                    } else {
                        crate::common::paths::set_app_data_dir_override(data_dir);
                        tracing::info!("[bootstrap] Android 数据目录覆盖已注入");
                    }
                } else {
                    tracing::error!("[bootstrap] Android: 无法解析 app_data_dir，后端存储将不可用");
                }
            }
            // 注册 jc-shell:// 协议：授权页授权成功后点「打开 JC Shell」即由此唤起客户端。
            // - Windows 写注册表 HKCU、Linux 写 .desktop 文件，均不需要管理员权限；
            // - macOS 不支持运行时注册，靠 tauri.conf.json 里静态声明的 schemes 生效。
            // 注册失败仅记日志、不阻断启动：协议唤起是锦上添花，不能让它拖垮主流程。
            #[cfg(any(windows, target_os = "linux"))]
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                if let Err(e) = app.deep_link().register_all() {
                    tracing::warn!("[deep-link] 注册 jc-shell:// 协议失败: {e}");
                }
            }

            // 任务栏图标换成专用熊猫图（窗口左上角图标保持默认不变）
            #[cfg(windows)]
            if let Some(window) = app.get_webview_window("main") {
                taskbar_icon::apply(&window);
            }

            // macOS 保留原生三圆点（Overlay 标题栏，见 tauri.conf.json），仅在精简模式下隐藏（sync_zen_mode）；
            // Windows / Linux 走自绘 HTML chrome，需要去掉原生标题栏（见 DockLayout.vue）。
            #[cfg(any(windows, target_os = "linux"))]
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_decorations(false);
                let _ = window.set_shadow(true);
            }

            // 初始化全局语言状态（从系统 locale 读取），并构建 macOS 系统菜单栏
            // 非 macOS 平台菜单由前端 HTML TopMenuBar 提供，无需处理
            crate::locale_state::init_from_system();
            crate::locale_state::apply_initial_menu(&app.handle());

            // TerminalSessionManager 不依赖 DB，同步注册即可
            app.manage(TerminalSessionManager::new());
            // 端口转发隧道管理器（不依赖 DB，同步注册）
            app.manage(PortForwardManager::default());
            // UpdateService（自动更新服务，初始占位，后续按 OAuth 配置原地更新后端地址）
            // 注入 AppHandle 用于向前端 emit 下载进度事件（update-download-progress）
            let mut update_svc = UpdateService::new(
                OAUTH_DEFAULT_SERVER_BASE_URL.to_string(),
                "jc-shell".to_string(),
            );
            update_svc.set_app_handle(app.handle().clone());
            // 更新上报携带登录态（会员 token），后台可把更新记录关联到用户
            update_svc.set_oauth(oauth_client.clone());
            app.manage(UpdateServiceState::new(update_svc));
            // OAuthClient 同步注册 (默认配置，DB ready 后异步覆盖)
            app.manage(oauth_client.clone());
            // 客户端使用遥测（DAU/使用时长上报）：与 OAuth 共享后端地址配置
            app.manage(Arc::new(TelemetryService::new(oauth_client.clone())));
            // settings 窗口关闭时隐藏而非销毁，下次打开可复用
            if let Some(settings_window) = app.get_webview_window("settings") {
                let settings_win = settings_window.clone();
                settings_window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = settings_win.hide();
                    }
                });
            }

            // 预创建 Tab 拖拽浮层（visible:false），让 WebView2 在后台完成冷启动，
            // 避免首次拖拽 Tab 时浮层延迟 1~2 秒才出现
            // ⚠️ 仅桌面：Android 上所有窗口都渲染进同一个 Activity，后创建的
            // ghost 窗口会永久置顶盖住主窗口（表现为白屏 + “● 终端”占位文本）。
            // 拖拽依赖 DeviceState 全局光标轮询，本就是桌面专属功能。
            #[cfg(desktop)]
            commands::precreate_tab_ghost(&app.handle());

            // 关闭 WebView2 DevTools：Tauri 2 在 debug 构建中默认启用 DevTools 且运行时 API 无法关闭
            // （WebviewWindow 只有 open/close/is_open，没有禁用；config/builder 的 devtools:false 在 debug 下被忽略）。
            // 唯一可靠的办法：拿到原生 WebView2 控制器，直接调 ICoreWebView2Settings::SetAreDevToolsEnabled(false)。
            // 生效后 Ctrl+Shift+I / F12 / 右键检查元素 等所有路径都无法打开调试窗口。
            #[cfg(target_os = "windows")]
            {
                disable_devtools_all(app.handle());
                // 兜底重试：窗口/WebView 初始化存在时序差异，稍后补几次确保全部生效
                let h = app.handle().clone();
                std::thread::spawn(move || {
                    for delay in [400u64, 1200, 3000] {
                        std::thread::sleep(std::time::Duration::from_millis(delay));
                        disable_devtools_all(&h);
                    }
                });
            }

            // 系统托盘：「最小化到托盘」后窗口隐藏，托盘是唯一的恢复/退出入口。
            // 菜单：打开主窗口 / 设置 / 检查更新 / 退出进程
            // 检查更新：通过 emit 事件通知前端 update store 调 check_update command，
            //          tray handler 自身不直接调 service（避免重复初始化 OAuth client 等）
            #[cfg(desktop)]
            {
                use tauri::{
                    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
                    Emitter, Manager,
                };
                /// 恢复主窗口：show + unminimize + set_focus
                /// （托盘左键单击与菜单「打开主窗口」共用同一套逻辑）
                fn show_main_window(app: &tauri::AppHandle) {
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = w.show();
                        let _ = w.unminimize();
                        let _ = w.set_focus();
                    }
                }
                let menu = crate::menu::build_tray_menu_items(
                    app.handle(),
                    &crate::locale_state::current_locale(),
                )?;
                let tray_icon = app.default_window_icon().cloned().ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::NotFound, "default window icon missing")
                })?;
                // 托盘事件回调要求 'static，先克隆一份 AppHandle 移入闭包
                let tray_app_handle = app.handle().clone();
                TrayIconBuilder::with_id("jc-shell-tray")
                    .icon(tray_icon)
                    .tooltip("JC Shell")
                    .menu(&menu)
                    // 仅右键弹菜单；左键改由 on_tray_icon_event 直接打开主窗口
                    .show_menu_on_left_click(false)
                    .on_menu_event(|app, event| match event.id.as_ref() {
                        // 打开主窗口：show + unminimize + focus
                        "show_main" => show_main_window(app),
                        // 设置：复用 open_settings command 的窗口管理逻辑
                        // 1) 已存在 → show + focus + center
                        // 2) 不存在 → 动态创建（理论 tauri.conf.json 已定义，这里兜底）
                        "open_settings" => {
                            // 复用 open_settings_window：自动跟随主窗口置顶状态，
                            // 避免主窗口被图钉固定时设置窗口被压在下面
                            if let Err(e) =
                                crate::commands::window_cmd::open_settings_window(app, None)
                            {
                                tracing::warn!("托盘设置菜单：打开设置窗口失败: {e}");
                            }
                        }
                        // 检查更新：emit 事件到 main webview，前端 update store 监听后调 check_update command
                        "check_update" => {
                            // 1) 把主窗口 show 出来（避免用户在看不到窗口的情况下点检查更新）
                            if let Some(w) = app.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.unminimize();
                            }
                            // 2) emit 事件
                            if let Err(e) = app.emit_to("main", "tray://check_update", ()) {
                                tracing::warn!("托盘检查更新：emit 事件失败: {e}");
                            }
                        }
                        // 退出进程：app.exit(0) 保证整个 Tauri App 真正退出
                        "quit_app" => {
                            app.exit(0);
                        }
                        _ => {}
                    })
                    .on_tray_icon_event(move |_tray, event| {
                        // 左键单击：直接打开主窗口（show_menu_on_left_click=false 后左键不再弹菜单）
                        // 右键保持原行为，由系统弹出菜单 —— 与用户预期一致
                        // 只响应 Up，避免部分平台按下/抬起各触发一次导致重复执行
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event
                        {
                            show_main_window(&tray_app_handle);
                        }
                    })
                    .build(app)?;
            }

            // AppState（含 SQLite pool）异步初始化，完成后 manage + 加载 OAuth 配置
            let app_handle = app.handle().clone();
            let oauth_for_db = oauth_client.clone();
            tauri::async_runtime::spawn(async move {
                match init_db().await {
                    Ok(db) => {
                        // 配置优先级 (从低到高):
                        // 1. 编译常量 OAUTH_DEFAULT_* (已在 OAuthClient 初始化)
                        // 2. YAML 文件 auth 节点 (%LocalAppData%/JC Shell/config.yml)
                        let yaml_cfg = YamlConfigService::new().load().await.ok().map(|c| c.auth);
                        if let Some(auth) = yaml_cfg.filter(is_some_auth) {
                            let merged = merge_auth_into_default(&auth);
                            oauth_for_db.update_config(merged).await;
                            tracing::info!("OAuth 配置已从 config.yml 加载");
                        }
                        // 3. SQLite app_setting.oauth_config (优先级最高, 不破坏已有 set_oauth_config)
                        if let Some(cfg) = load_oauth_config(&db).await {
                            oauth_for_db.update_config(cfg).await;
                            tracing::info!("OAuth 配置已从 app_setting 加载 (覆盖 YAML)");
                        }
                        let state = Arc::new(AppState { db });
                        APP_STATE.set(state.clone()).ok();
                        app_handle.manage(state);
                        tracing::info!("JC Shell 后端初始化完成");
                    }
                    Err(e) => {
                        tracing::warn!(%e, "JC Shell 后端初始化失败（不影响前端核心 UI 展示），请检查 SQLite 目录读写权限");
                    }
                }
            });
            // 兜底：主窗口初始 visible=false，正常由前端 JS 在 onMounted 调 show()；
            // 若 WebView2/前端异常导致 JS 未执行，窗口会永远不可见（进程存在但无窗口）。
            // 启动 5 秒后若仍不可见则强制显示并告警，避免“卡死”假象。
            let fallback_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_secs(5)).await;
                if let Some(w) = fallback_handle.get_webview_window("main") {
                    match w.is_visible() {
                        Ok(false) => {
                            tracing::warn!("启动 5 秒后主窗口仍不可见（前端可能未执行 show），强制显示");
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                        Err(e) => tracing::warn!("查询主窗口可见性失败: {e}"),
                        _ => {}
                    }
                }
            });
            Ok(())
        })
        .invoke_handler({
            // 社区版命令表提取为变量，供下方分发闭包调用
            let community_handler = as_command_table(tauri::generate_handler![
            // terminal
            commands::create_session, commands::connect_ssh, commands::close_session,
            commands::resize_session, commands::send_session_input, commands::list_sessions,
            commands::exec_remote_command, commands::get_shell_cwd, commands::is_shell_command,
            commands::create_local_shell, commands::open_terminal_window,
            commands::upload_file, commands::download_file, commands::transfer_remote_file, commands::cancel_transfer,
            commands::upload_file_bytes,
            commands::read_remote_file, commands::write_remote_file,
            commands::delete_local_path, commands::pick_folder_path, commands::open_in_file_manager,
            commands::detect_archive_tools, commands::archive_compress, commands::archive_extract,
            commands::make_remote_dir, commands::create_remote_file, commands::set_file_permissions,
            commands::get_local_home_dir, commands::list_local_dir, commands::list_local_roots,
            commands::list_remote_dir,
            commands::get_local_system_info, commands::get_local_cpu_info, commands::get_local_mem_info,
            commands::get_local_disk_info, commands::get_local_net_rate,
            commands::stash_term_buffer, commands::take_term_buffer, commands::copy_local_file,
            commands::move_local_file,
            // host
            commands::upsert_host, commands::list_hosts, commands::delete_host,
            // ai
            commands::ai_chat, commands::ai_abort, commands::list_model_configs,
            commands::upsert_model_config, commands::delete_model_config, commands::test_model_config,
            // ai agent
            commands::ai_agent_query, commands::ai_guard_check, commands::ai_asset_balance,
            commands::ai_asset_txns, commands::ai_usage_daily,
            // 配置云同步(仅登录用户; 合并算法在前端, Rust 侧只代理 HTTP)
            commands::sync_pull_states, commands::sync_pull_doc, commands::sync_push_doc,
            // plugin
            commands::list_plugins, commands::toggle_plugin, commands::uninstall_plugin,
            // setting
            commands::get_setting, commands::set_setting, commands::delete_setting,
            commands::list_all_settings, commands::get_data_dir, commands::set_auto_launch,
            commands::check_key_file,
            // ssh 私钥托管（选择/读取/落盘/清理）
            commands::pick_key_file, commands::read_key_file,
            commands::materialize_key, commands::prune_ssh_keys,
                        commands::get_system_proxy, commands::pick_save_path, commands::save_text_file,
            // 跨软件会话导入（选择/读取/列目录/探测已安装客户端）
            commands::pick_import_file, commands::pick_import_folder,
            commands::read_local_text_file, commands::list_import_files,
            commands::scan_installed_clients,
            // security（统一安全密码：本地保护 + 锁定）
            commands::security_status, commands::security_set_password,
            commands::security_verify, commands::security_disable,
            commands::security_update_scopes, commands::security_wipe_and_disable,
            commands::security_set_biometric,
            commands::security_get_sync, commands::security_set_sync,
            // auth
            commands::start_login, commands::get_auth_state, commands::logout,
            commands::cancel_login,
            commands::get_oauth_config, commands::set_oauth_config,
            // auth: APP 内手机号登录（移动端）
            commands::phone_captcha_get, commands::phone_captcha_check,
            commands::phone_sms_send, commands::phone_sms_cooldown, commands::phone_login,
            // window
            commands::open_settings, commands::open_feedback, commands::open_profile,
            commands::open_external_url,
            commands::tab_ghost_begin, commands::tab_ghost_snap, commands::tab_ghost_unsnap,
            commands::tab_ghost_cancel, commands::sync_zen_mode,
            // update
            commands::check_update, commands::get_update_status,
            commands::download_update, commands::silent_download_update,
            commands::cancel_update_download, commands::get_download_info,
            commands::install_update, commands::get_install_scope,
            commands::app_exit,
            commands::hide_main_window,
            // qq group
            commands::get_qq_groups,
            // 客户端启动弹窗（内容由管理员在后台配置，后台渲染 HTML 后下发）
            commands::get_launch_dialog,
            // 法律协议（用户协议 / 隐私政策）：客户端拉取配置 + 登录后提交同意
            commands::get_legal_agreement,
            commands::submit_agreement,
            commands::legal_page_url,
            // client error log (前端静默上报，失败不抛)
            commands::report_client_errors,
            // 客户端使用遥测（启动/心跳/退出，前端静默上报）
            commands::telemetry_session_start,
            commands::telemetry_session_heartbeat,
            commands::telemetry_session_end,
            // 端口转发：连通性探测（真实 TCP 连接）+ 隧道建立/拆除
            commands::tcp_probe,
            commands::port_forward_start,
            commands::port_forward_stop,
            commands::port_forward_running,
            commands::port_forward_probe,
            commands::remote_client_probe,
            // locale
            locale_state::get_app_locale, locale_state::set_app_locale,
            ]);
            // ===== 外部扩展点 2：命令分发 =====
            // pro_ 前缀 → 外部扩展处理器；其余命令 → 社区版命令表（行为不变）
            move |invoke: tauri::ipc::Invoke<tauri::Wry>| match &ext.command_handler {
                Some(pro) if invoke.message.command().starts_with(PRO_COMMAND_PREFIX) => {
                    pro(invoke)
                }
                _ => community_handler(invoke),
            }
        })
        .run(context)
        .expect("启动 JC Shell 失败");
}