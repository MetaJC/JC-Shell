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

//! 应用目录路径 —— 数据目录必须与安装目录隔离
//! 作者：JC
//!
//! NSIS currentUser 安装目录为 %LocalAppData%\JC Shell，卸载器会清空整个安装目录。
//! 历史版本把数据目录（DB / config / webview2）也放在该目录下，导致卸载连带删除
//! 用户全部数据（重装后"会话丢失"）。现改为：
//! - 数据目录：%AppData%\JC Shell（config_dir，卸载保留）
//! - WebView2：%LocalAppData%\jcshell-webview2（改名避开安装目录，卸载保留）

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::common::{APP_DIR_NAME, CONFIG_FILE_NAME, DB_FILE_NAME, SSH_KEYS_DIR_NAME};

/// WebView2 用户数据目录名（位于 %LocalAppData% 下；不能与安装目录 "JC Shell" 同名）
///
/// 不同构建目标必须各自独立：两者的 WebView2 自定义协议同为
/// http://tauri.localhost，共享数据目录会让 HTTP 缓存互相串用。
pub fn webview2_dir_name() -> &'static str {
    // 启用 pro-edition feature 时使用独立目录名
    if cfg!(feature = "pro-edition") {
        "jcshell-pro-webview2"
    } else {
        "jcshell-webview2"
    }
}

/// 数据目录运行时覆盖（OnceLock：启动阶段设置一次，之后只读）
/// Android 上 dirs::config_dir() 返回 None，默认会回退到 CWD（"/"，只读），
/// 导致 SQLite/配置写入报 "Read-only file system"。由启动阶段把 Tauri
/// 解析出的应用私有目录（/data/data/<pkg>/files/JC Shell）注入进来。
static APP_DATA_DIR_OVERRIDE: OnceLock<PathBuf> = OnceLock::new();

/// 注入数据目录覆盖（须在任何 app_data_dir() 调用前完成，通常在 Tauri setup 最早期）
pub fn set_app_data_dir_override(dir: PathBuf) {
    let _ = APP_DATA_DIR_OVERRIDE.set(dir);
}

/// 应用数据目录：%AppData%\JC Shell（与安装目录隔离，卸载不清除）
pub fn app_data_dir() -> PathBuf {
    if let Some(dir) = APP_DATA_DIR_OVERRIDE.get() {
        return dir.clone();
    }
    dirs::config_dir()
        .map(|d| d.join(APP_DIR_NAME))
        .unwrap_or_else(|| PathBuf::from(".").join(APP_DIR_NAME))
}

/// 旧数据目录：%LocalAppData%\JC Shell（历史版本数据所在地，现为安装目录）
pub fn legacy_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .map(|d| d.join(APP_DIR_NAME))
        .unwrap_or_else(|| PathBuf::from(".").join(APP_DIR_NAME))
}

/// SSH 私钥托管目录：%AppData%\JC Shell\ssh-keys
/// 会话只保存私钥"内容"，连接前由 ssh_key_cmd 落到这里供 russh 读取，
/// 这样用户删掉原文件也不影响已保存的会话。
pub fn ssh_keys_dir() -> PathBuf {
    app_data_dir().join(SSH_KEYS_DIR_NAME)
}

/// WebView2 数据目录：%LocalAppData%\jcshell-webview2（另一构建目标为 jcshell-pro-webview2）
pub fn webview2_data_dir() -> PathBuf {
    let name = webview2_dir_name();
    dirs::data_local_dir()
        .map(|d| d.join(name))
        .unwrap_or_else(|| std::env::temp_dir().join(name))
}

/// 移动单个文件：优先 rename，跨卷时回退 copy + 删源
fn move_file(src: &Path, dst: &Path) -> std::io::Result<()> {
    match std::fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(_) => {
            std::fs::copy(src, dst)?;
            std::fs::remove_file(src)
        }
    }
}

/// 一次性迁移旧安装目录中的存量数据（DB + YAML config）；新目录已有 DB 时跳过。
/// 注意：.hmac 标签与路径绑定，不迁移，直接删除由新路径重新生成
/// （master key 存 OS KeyChain 不随卸载消失，新标签可正常生成与校验）。
pub fn migrate_legacy_data(app_dir: &Path) {
    let new_db = app_dir.join(DB_FILE_NAME);
    if new_db.exists() {
        return;
    }
    let old_dir = legacy_data_dir();
    let old_db = old_dir.join(DB_FILE_NAME);
    if !old_db.exists() {
        return;
    }
    // WAL 模式：已提交数据可能残留在 -wal/-shm，必须随主文件一起迁移
    for suffix in ["", "-wal", "-shm"] {
        let src = old_dir.join(format!("{DB_FILE_NAME}{suffix}"));
        if src.exists() {
            let dst = app_dir.join(format!("{DB_FILE_NAME}{suffix}"));
            if let Err(e) = move_file(&src, &dst) {
                tracing::warn!(src = %src.display(), dst = %dst.display(), err = %e, "旧数据迁移失败");
                return;
            }
        }
    }
    // 路径绑定的 HMAC 标签：删旧，新路径启动时自动重建
    let _ = std::fs::remove_file(old_dir.join(format!("{DB_FILE_NAME}.hmac")));
    // YAML 配置
    let cfg = old_dir.join(CONFIG_FILE_NAME);
    if cfg.exists() {
        let _ = move_file(&cfg, &app_dir.join(CONFIG_FILE_NAME));
    }
    tracing::info!(from = %old_dir.display(), to = %app_dir.display(), "旧安装目录存量数据迁移完成");
}

/// 一次性迁移 WebView2 目录（必须在设置 WEBVIEW2_USER_DATA_FOLDER 环境变量前调用）。
/// 保留 localStorage（登录态、界面偏好）在卸载/升级后不丢失。
pub fn migrate_legacy_webview2() {
    let new_dir = webview2_data_dir();
    if new_dir.exists() {
        return;
    }
    let old = legacy_data_dir().join("webview2");
    if !old.exists() {
        return;
    }
    match std::fs::rename(&old, &new_dir) {
        Ok(()) => tracing::info!(to = %new_dir.display(), "WebView2 数据目录迁移完成"),
        Err(e) => tracing::warn!(err = %e, "WebView2 数据目录迁移失败，将使用全新目录"),
    }
}
