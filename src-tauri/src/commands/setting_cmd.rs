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

//! 应用设置 Tauri Commands（通用 key/value 读写 + 系统级设置）
//! 作者：JC

use std::sync::Arc;

use crate::common::{AppError, AppResult};
use crate::models::setting::SettingDO;
use crate::repositories::setting_repo::SettingRepository;
use crate::AppState;

/// 读取设置值（不存在返回 null）
#[tauri::command]
pub async fn get_setting(
    key: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> AppResult<Option<String>> {
    let repo = SettingRepository::new(&state.db);
    let row = repo.get(&key).await?;
    Ok(row.map(|d| d.value))
}

/// 写入设置值（UPSERT）
#[tauri::command]
pub async fn set_setting(
    key: String,
    value: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> AppResult<()> {
    let repo = SettingRepository::new(&state.db);
    repo.upsert(&key, &value).await
}

/// 删除设置值
#[tauri::command]
pub async fn delete_setting(
    key: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> AppResult<bool> {
    let repo = SettingRepository::new(&state.db);
    repo.delete(&key).await
}

/// 全量设置列表（数据备份导出用）
#[tauri::command]
pub async fn list_all_settings(
    state: tauri::State<'_, Arc<AppState>>,
) -> AppResult<Vec<SettingDO>> {
    let repo = SettingRepository::new(&state.db);
    repo.list_all().await
}

/// 获取应用数据目录（设置页展示/打开用；%AppData%\JC Shell，与安装目录隔离）
#[tauri::command]
pub fn get_data_dir() -> AppResult<String> {
    Ok(crate::common::paths::app_data_dir().to_string_lossy().to_string())
}

/// 开机自启动（Windows：写注册表 Run 项；其他平台暂不支持）
#[tauri::command]
pub async fn set_auto_launch(enabled: bool) -> AppResult<()> {
    crate::common::blocking::run(move || set_auto_launch_inner(enabled)).await
}

fn set_auto_launch_inner(enabled: bool) -> AppResult<()> {
    #[cfg(windows)]
    {
        const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
        let exe = std::env::current_exe()
            .map_err(|e| AppError::Internal(format!("获取当前程序路径失败: {e}")))?;
        let exe_str = exe
            .to_str()
            .ok_or_else(|| AppError::Internal("程序路径含非法字符".into()))?;
        let status = if enabled {
            std::process::Command::new("reg")
                .args(["add", RUN_KEY, "/v", "JCShell", "/t", "REG_SZ", "/d", exe_str, "/f"])
                .status()
        } else {
            std::process::Command::new("reg")
                .args(["delete", RUN_KEY, "/v", "JCShell", "/f"])
                .status()
        }
        .map_err(|e| AppError::Internal(format!("执行注册表命令失败: {e}")))?;
        // 关闭自启动时注册表项不存在也会返回非 0，视为成功；开启时必须成功
        if enabled && !status.success() {
            return Err(AppError::Internal("写入开机自启动注册表失败".into()));
        }
        tracing::info!(enabled, "开机自启动已更新");
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = enabled;
        Err(AppError::Validation("当前平台暂不支持开机自启动".into()))
    }
}

/// 校验密钥文件（安全设置「校验密钥权限」开启时前端连接前调用）：
/// 文件必须存在；Unix 下额外拒绝组/其他用户可写的过宽权限。
#[tauri::command]
pub async fn check_key_file(path: String) -> AppResult<()> {
    crate::common::blocking::run(move || check_key_file_inner(path)).await
}

fn check_key_file_inner(path: String) -> AppResult<()> {
    let p = std::path::Path::new(&path);
    let meta = std::fs::metadata(p)
        .map_err(|_| AppError::Validation(format!("密钥文件不存在或不可读：{path}")))?;
    if !meta.is_file() {
        return Err(AppError::Validation(format!("密钥路径不是有效文件：{path}")));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = meta.permissions().mode() & 0o777;
        if mode & 0o022 != 0 {
            return Err(AppError::Validation(format!(
                "密钥文件权限过宽（{mode:03o}），请执行 chmod 600 后重试"
            )));
        }
    }
    Ok(())
}

/// 系统代理信息（地址 + 类型）
#[derive(serde::Serialize)]
pub struct SystemProxyInfo {
    pub addr: String,
    /// socks5 / http
    pub kind: String,
}

/// 读取系统代理（「跟随系统」模式用）：
/// Windows 读 IE 代理注册表；其他平台读环境变量。未配置返回 null。
#[tauri::command]
pub async fn get_system_proxy() -> AppResult<Option<SystemProxyInfo>> {
    crate::common::blocking::run(get_system_proxy_inner).await
}

fn get_system_proxy_inner() -> AppResult<Option<SystemProxyInfo>> {
    #[cfg(windows)]
    {
        const IE_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";
        let query = |value: &str| -> Option<String> {
            let out = std::process::Command::new("reg")
                .args(["query", IE_KEY, "/v", value])
                .output()
                .ok()?;
            let text = String::from_utf8_lossy(&out.stdout);
            text.lines()
                .find(|l| l.trim_start().starts_with(value))
                .and_then(|l| l.split_whitespace().last())
                .map(|s| s.to_string())
        };
        let enabled = query("ProxyEnable").map(|v| v != "0x0").unwrap_or(false);
        if !enabled {
            return Ok(None);
        }
        let raw = match query("ProxyServer") {
            Some(v) if !v.is_empty() => v,
            _ => return Ok(None),
        };
        // 兼容 "host:port" 与 "http=host:port;socks=host:port" 两种格式
        if raw.contains('=') {
            let mut http_entry: Option<String> = None;
            let mut socks_entry: Option<String> = None;
            for part in raw.split(';') {
                let (k, v) = part.split_once('=').unwrap_or(("", part));
                match k.trim() {
                    "http" | "https" => http_entry = Some(v.trim().to_string()),
                    "socks" => socks_entry = Some(v.trim().to_string()),
                    _ => {}
                }
            }
            if let Some(addr) = http_entry {
                return Ok(Some(SystemProxyInfo { addr, kind: "http".into() }));
            }
            if let Some(addr) = socks_entry {
                return Ok(Some(SystemProxyInfo { addr, kind: "socks5".into() }));
            }
            return Ok(None);
        }
        Ok(Some(SystemProxyInfo { addr: raw, kind: "http".into() }))
    }
    #[cfg(not(windows))]
    {
        for (var, kind) in [("ALL_PROXY", "socks5"), ("all_proxy", "socks5"), ("HTTPS_PROXY", "http"), ("https_proxy", "http")] {
            if let Ok(v) = std::env::var(var) {
                let v = v.trim().trim_end_matches('/');
                if v.is_empty() {
                    continue;
                }
                let (k, addr) = if let Some(rest) = v.strip_prefix("socks5://") {
                    ("socks5", rest)
                } else if let Some(rest) = v.strip_prefix("http://") {
                    ("http", rest)
                } else {
                    (kind, v)
                };
                return Ok(Some(SystemProxyInfo { addr: addr.to_string(), kind: k.into() }));
            }
        }
        Ok(None)
    }
}

/// 弹出系统保存对话框，返回用户选定的完整路径（取消返回 null）
#[tauri::command]
pub fn pick_save_path(title: String, default_name: String) -> AppResult<Option<String>> {
    #[cfg(desktop)]
    {
        let file = rfd::FileDialog::new()
            .set_title(&title)
            .set_file_name(&default_name)
            .add_filter("JSON", &["json"])
            .save_file();
        Ok(file.map(|p| p.to_string_lossy().to_string()))
    }
    #[cfg(mobile)]
    {
        let _ = (title, default_name);
        Ok(None)
    }
}

/// 保存文本文件（导出会话/备份时，前端保存对话框选定路径后写入）
#[tauri::command]
pub async fn save_text_file(path: String, content: String) -> AppResult<()> {
    crate::common::blocking::run(move || save_text_file_inner(path, content)).await
}

fn save_text_file_inner(path: String, content: String) -> AppResult<()> {
    std::fs::write(&path, content)
        .map_err(|e| AppError::Internal(format!("写入文件失败 {path}: {e}")))
}
