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

//! SSH 私钥托管：文件选择 / 读取 / 落盘托管 / 过期清理
//! 作者：JC
//!
//! 背景：会话若只保存用户私钥的「原始路径」，用户一旦删除或移动该文件，
//! 这个会话就再也连不上。这里改为「导入即托管」：
//!   - 私钥内容随会话配置保存（前端 host 文档）；
//!   - 连接前由本模块把内容落到应用数据目录（%AppData%\JC Shell\ssh-keys），
//!     再把该绝对路径交给 russh 读取。
//! 文件按内容 SHA-256 命名带来三个好处：
//!   1. 同一把钥匙被多个会话引用时只存一份；
//!   2. 内容即身份，天然去重，不会产生孤儿文件；
//!   3. 内容没变则复用同一文件，重写一次即可刷新 mtime（供"保存期限"清理判活跃）。

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use tauri::command;

use crate::common::paths::ssh_keys_dir;
use crate::common::{AppError, AppResult};

/// 私钥体积上限（8KB）：正常 RSA/ED25519 私钥远小于此，超限几乎必然是选错了文件
const MAX_KEY_BYTES: usize = 8 * 1024;

/// 读取到的私钥文件内容
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyFileData {
    /// 私钥文本（PEM / OpenSSH 格式原样）
    pub content: String,
    /// 原始文件名，仅用于界面展示
    pub name: String,
}

/// 弹出系统「打开文件」对话框选择私钥；取消返回 None。
///
/// 这里**不设置任何扩展名过滤器**。rfd 会把过滤器当作扩展名白名单，
/// `add_filter("所有文件", &["*"])` 里的 "*" 在 macOS 上会被解析成扩展名 "*"，
/// 结果是所有文件都被置灰、连 .pem 都点不中（Windows 上同样不可靠）。
/// 私钥本身也常常没有扩展名（如 id_rsa），所以整体不过滤，交由用户自由选择。
#[command]
pub fn pick_key_file() -> Option<String> {
    #[cfg(desktop)]
    {
        rfd::FileDialog::new()
            .set_title("选择 SSH 私钥文件")
            .pick_file()
            .map(|p| p.to_string_lossy().to_string())
    }
    #[cfg(mobile)]
    {
        None
    }
}

/// 读取私钥文件内容（前端 webview 无权读取任意路径，必须经由此命令）
///
/// 读盘放阻塞线程池：同步命令跑在 UI 主线程上，网络盘/被杀软扫描的文件
/// 读取可能停顿数百毫秒，会把所有窗口一起卡住。
#[command]
pub async fn read_key_file(path: String) -> AppResult<KeyFileData> {
    crate::common::blocking::run(move || read_key_file_inner(path)).await
}

fn read_key_file_inner(path: String) -> AppResult<KeyFileData> {
    let p = PathBuf::from(&path);
    let meta = std::fs::metadata(&p)
        .map_err(|_| AppError::Validation(format!("密钥文件不存在或不可读：{path}")))?;
    if !meta.is_file() {
        return Err(AppError::Validation(format!("这不是一个文件：{path}")));
    }
    if meta.len() as usize > MAX_KEY_BYTES {
        return Err(AppError::Validation(
            "文件过大，看起来不是私钥（上限 8KB）".into(),
        ));
    }
    let content = std::fs::read_to_string(&p)
        .map_err(|e| AppError::Validation(format!("读取密钥文件失败：{e}")))?;
    if content.trim().is_empty() {
        return Err(AppError::Validation("密钥文件是空的".into()));
    }
    let name = p
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    Ok(KeyFileData { content, name })
}

/// 私钥内容 -> 托管文件绝对路径（可直接交给 russh 读取）。
/// 每次调用都会重写文件：既刷新 mtime 供清理策略判活跃，也顺带自愈被改坏的文件。
#[command]
pub async fn materialize_key(content: String) -> AppResult<String> {
    crate::common::blocking::run(move || materialize_key_inner(content)).await
}

fn materialize_key_inner(content: String) -> AppResult<String> {
    if content.trim().is_empty() {
        return Err(AppError::Validation("私钥内容为空".into()));
    }
    let dir = ssh_keys_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| AppError::Internal(format!("创建密钥托管目录失败：{e}")))?;

    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let path = dir.join(hex_encode(&hasher.finalize()));

    std::fs::write(&path, content.as_bytes())
        .map_err(|e| AppError::Internal(format!("写入托管密钥失败：{e}")))?;
    set_private_perms(&path);
    Ok(path.to_string_lossy().to_string())
}

/// 清理超过 max_idle_days 天未使用的托管私钥，返回删除数量。
/// max_idle_days = 0 表示永久保留（不清理）。
/// 安全前提：私钥内容存在会话配置里，托管文件只是缓存，删掉后下次连接会自动重建。
#[command]
pub async fn prune_ssh_keys(max_idle_days: u64) -> AppResult<usize> {
    crate::common::blocking::run(move || prune_ssh_keys_inner(max_idle_days)).await
}

fn prune_ssh_keys_inner(max_idle_days: u64) -> AppResult<usize> {
    if max_idle_days == 0 {
        return Ok(0);
    }
    let dir = ssh_keys_dir();
    if !dir.exists() {
        return Ok(0);
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let cutoff = now.saturating_sub(max_idle_days.saturating_mul(86_400));

    let mut removed = 0usize;
    let entries = std::fs::read_dir(&dir)
        .map_err(|e| AppError::Internal(format!("读取密钥目录失败：{e}")))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let mtime = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if mtime < cutoff {
            if std::fs::remove_file(&path).is_ok() {
                removed += 1;
            }
        }
    }
    Ok(removed)
}

/// 私钥必须是仅本人可读写：Unix 下 OpenSSH 与 russh 都会拒绝权限过宽的私钥
#[cfg(unix)]
fn set_private_perms(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

/// Windows 无 POSIX 权限模型，AppData 本身即用户私有目录，无需处理
#[cfg(not(unix))]
fn set_private_perms(_path: &Path) {}

fn hex_encode(bytes: &[u8]) -> String {
    const LUT: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(LUT[(b >> 4) as usize] as char);
        s.push(LUT[(b & 0x0f) as usize] as char);
    }
    s
}
