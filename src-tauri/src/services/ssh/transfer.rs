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

//! SSH 文件传输（SFTP 优先 / base64 兜底）—— 支持文件与目录、可取消、进度聚合
//! 作者：JC
//! 目录上传：SFTP 逐级建目录 + 逐文件分块写入（单会话复用）；兜底 `mkdir -p` + 逐文件 base64。
//! 目录下载：SFTP 递归遍历 + 逐文件拉取；兜底服务端 `tar cf` 打包后 base64 拉回本地解包。

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Emitter};

use crate::common::{AppError, AppResult};

use super::russh_impl::{exec_remote_command, open_exec_stream, open_sftp_session, shell_quote, SshConfig};

/// 取消错误文案（前端按此文案识别"已取消"状态，区别于失败）
pub const CANCEL_MSG: &str = "传输已取消";

fn err_cancelled() -> AppError {
    AppError::Ssh(CANCEL_MSG.into())
}

fn is_cancelled_err(e: &AppError) -> bool {
    matches!(e, AppError::Ssh(m) if m == CANCEL_MSG)
}

/// 传输进度事件载荷
#[derive(serde::Serialize, Clone)]
struct TransferProgressPayload {
    transfer_id: String,
    transferred: u64,
    total: u64,
    done: bool,
    error: Option<String>,
}

/// 传输上下文：聚合进度（目录传输跨文件累计）、200ms 节流、取消检查
pub(crate) struct TransferCtx {
    app: AppHandle,
    transfer_id: String,
    total: u64,
    transferred: AtomicU64,
    last_emit: Mutex<tokio::time::Instant>,
    cancel: Arc<AtomicBool>,
}

impl TransferCtx {
    fn new(app: &AppHandle, transfer_id: &str, total: u64, cancel: Arc<AtomicBool>) -> Arc<Self> {
        Arc::new(Self {
            app: app.clone(),
            transfer_id: transfer_id.to_string(),
            total,
            transferred: AtomicU64::new(0),
            last_emit: Mutex::new(tokio::time::Instant::now() - Duration::from_millis(200)),
            cancel,
        })
    }

    /// 取消检查（每个数据块调用一次）
    fn check_cancel(&self) -> AppResult<()> {
        if self.cancel.load(Ordering::Relaxed) {
            Err(err_cancelled())
        } else {
            Ok(())
        }
    }

    /// 累加已传输字节并按 200ms 节流发送进度事件
    fn add(&self, n: u64) {
        let cur = self.transferred.fetch_add(n, Ordering::Relaxed) + n;
        let due = {
            let mut last = self.last_emit.lock().unwrap();
            if last.elapsed() >= Duration::from_millis(200) {
                *last = tokio::time::Instant::now();
                true
            } else {
                false
            }
        };
        if due {
            self.emit(cur);
        }
    }

    fn emit(&self, transferred: u64) {
        let _ = self.app.emit("transfer-progress", TransferProgressPayload {
            transfer_id: self.transfer_id.clone(),
            transferred,
            total: self.total,
            done: false,
            error: None,
        });
    }

    /// 收尾事件：成功/失败/取消都发 done（取消按 error=CANCEL_MSG 传递，前端识别为"已取消"）
    fn finish(&self, result: &AppResult<()>) {
        let (transferred, error) = match result {
            Ok(_) => (self.total, None),
            Err(e) => (self.transferred.load(Ordering::Relaxed), Some(e.to_string())),
        };
        let _ = self.app.emit("transfer-progress", TransferProgressPayload {
            transfer_id: self.transfer_id.clone(),
            transferred,
            total: self.total,
            done: true,
            error,
        });
    }
}

// ===== 目录遍历 =====

/// 递归遍历本地目录，返回 (相对路径[/分隔], 字节数)
fn walk_local_files(root: &std::path::Path) -> AppResult<Vec<(String, u64)>> {
    let mut out = Vec::new();
    let mut stack: Vec<std::path::PathBuf> = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let rd = std::fs::read_dir(&dir)
            .map_err(|e| AppError::Io(std::io::Error::new(e.kind(), format!("读取目录失败 {}: {e}", dir.display()))))?;
        for entry in rd.flatten() {
            let p = entry.path();
            let ft = entry.file_type().ok();
            if ft.as_ref().map(|t| t.is_dir()).unwrap_or(false) {
                stack.push(p);
            } else if ft.as_ref().map(|t| t.is_file()).unwrap_or(false) {
                let rel = p.strip_prefix(root).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_default();
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                if !rel.is_empty() {
                    out.push((rel, size));
                }
            }
        }
    }
    Ok(out)
}

/// 递归遍历远程目录，返回 (相对路径[/分隔], 字节数)
async fn walk_remote_files(sftp: &russh_sftp::client::SftpSession, root: &str) -> AppResult<Vec<(String, u64)>> {
    let mut out = Vec::new();
    let mut stack: Vec<(String, String)> = vec![(root.to_string(), String::new())];
    while let Some((dir, rel_prefix)) = stack.pop() {
        let rd = sftp
            .read_dir(dir.clone())
            .await
            .map_err(|e| AppError::Ssh(format!("读取远程目录失败 {dir}: {e}")))?;
        for entry in rd {
            let rel = if rel_prefix.is_empty() {
                entry.file_name()
            } else {
                format!("{}/{}", rel_prefix, entry.file_name())
            };
            let ty = entry.metadata().file_type();
            if ty.is_dir() {
                stack.push((entry.path(), rel));
            } else if ty.is_file() {
                let size = entry.metadata().len();
                out.push((rel, size));
            }
        }
    }
    Ok(out)
}

// ===== SFTP 目录列出（文件浏览器专用）=====
// 背景：原先文件浏览器用 exec 跑 `ls -la` 再解析文本，目录列出会和终端共用 exec 通道。
// 跑部署脚本 / 大文件上传时该通道被占满，5 秒一次的自动刷新就周期性失败，
// 表现为"列表清空 + 底部红字 服务暂不可用"。改用 SFTP read_dir 后与终端彻底解耦。

/// 结构化远程目录项：文件浏览器直接渲染，不再解析 `ls -la` 文本
#[derive(serde::Serialize, Clone)]
pub struct RemoteDirEntry {
    pub name: String,
    /// "dir" | "file" | "symlink"
    pub entry_type: String,
    pub size: u64,
    /// 格式化后的修改时间（YYYY-MM-DD HH:MM）；服务端未提供时为空串
    pub modified: String,
    /// ls -l 风格权限串（如 -rw-r--r-- / drwxr-xr-x）
    pub permissions: String,
    pub uid: u32,
    pub gid: u32,
}

/// ls -l 风格权限串（含 setuid / setgid / sticky）
fn format_mode(mode: u32) -> String {
    let type_ch = match mode & 0o170000 {
        0o040000 => 'd',
        0o120000 => 'l',
        0o100000 => '-',
        0o060000 => 'b',
        0o020000 => 'c',
        0o010000 => 'p',
        0o140000 => 's',
        _ => '-',
    };
    let bit = |shift: u32, ch: char| -> char { if mode & shift != 0 { ch } else { '-' } };
    let suid = mode & 0o4000 != 0;
    let sgid = mode & 0o2000 != 0;
    let sticky = mode & 0o1000 != 0;
    // setuid/setgid/sticky 占用执行位：有 x 显示小写 s/t，无 x 显示大写 S/T
    let x_owner = if suid { if mode & 0o100 != 0 { 's' } else { 'S' } } else { bit(0o100, 'x') };
    let x_group = if sgid { if mode & 0o010 != 0 { 's' } else { 'S' } } else { bit(0o010, 'x') };
    let x_other = if sticky { if mode & 0o001 != 0 { 't' } else { 'T' } } else { bit(0o001, 'x') };
    format!(
        "{}{}{}{}{}{}{}{}{}{}",
        type_ch,
        bit(0o400, 'r'), bit(0o200, 'w'), x_owner,
        bit(0o040, 'r'), bit(0o020, 'w'), x_group,
        bit(0o004, 'r'), bit(0o002, 'w'), x_other,
    )
}

fn format_time(t: std::time::SystemTime) -> String {
    use chrono::{DateTime, Local};
    let dt: DateTime<Local> = t.into();
    dt.format("%Y-%m-%d %H:%M").to_string()
}

/// 在已建立的 SFTP 会话上读取并映射单个目录（调用方负责拿到会话与超时控制）。
/// 注意：本函数**不关闭** SFTP 通道（供调用方复用同一条会话连续列多个目录）；
/// 需要关闭时由调用方负责。目录映射逻辑与 `sftp_list_dir` 共用。
pub(crate) async fn sftp_read_and_map(
    sftp: &russh_sftp::client::SftpSession,
    path: &str,
) -> AppResult<Vec<RemoteDirEntry>> {
    let rd = sftp
        .read_dir(path.to_string())
        .await
        .map_err(|e| AppError::Ssh(format!("读取远程目录失败 {path}: {e}")))?;
    let mut out = Vec::new();
    for entry in rd {
        let name = entry.file_name();
        if name == "." || name == ".." {
            continue;
        }
        let meta = entry.metadata();
        let entry_type = if meta.file_type().is_dir() {
            "dir"
        } else if meta.file_type().is_file() {
            "file"
        } else {
            "symlink"
        };
        out.push(RemoteDirEntry {
            name,
            entry_type: entry_type.to_string(),
            size: meta.len(),
            modified: meta.modified().map(format_time).unwrap_or_default(),
            permissions: format_mode(meta.permissions.unwrap_or(0)),
            uid: meta.uid.unwrap_or(0),
            gid: meta.gid.unwrap_or(0),
        });
    }
    Ok(out)
}

/// 用 SFTP read_dir 列出单个目录（不经 shell，不与终端 exec 通道争用）。
/// 失败时返回 Err，调用方（前端）回退 exec 的 `ls -la` 通道。
/// 注：`RusshSessionHandle::list_dir` 现改为缓存 SFTP 会话（仅首屏重拨一次），
/// 复用 `sftp_read_and_map` + `open_sftp_session`，故本兜底函数已合并入调用方。


/// 远程逐级建目录（已存在则忽略）
async fn remote_mkdir_p(sftp: &russh_sftp::client::SftpSession, dir: &str) {
    let mut acc = String::new();
    for seg in dir.split('/').filter(|s| !s.is_empty()) {
        acc = if acc.is_empty() {
            if dir.starts_with('/') { format!("/{seg}") } else { seg.to_string() }
        } else {
            format!("{acc}/{seg}")
        };
        let _ = sftp.create_dir(acc.clone()).await;
    }
}

fn remote_parent(path: &str) -> Option<String> {
    let idx = path.rfind('/')?;
    if idx == 0 {
        Some("/".to_string())
    } else {
        Some(path[..idx].to_string())
    }
}

// ===== 单文件分块拷贝（进度计入 ctx） =====

async fn copy_file_to_remote(
    ctx: &TransferCtx,
    sftp: &russh_sftp::client::SftpSession,
    local_path: &str,
    remote_path: &str,
) -> AppResult<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut file = tokio::fs::File::open(local_path)
        .await
        .map_err(|e| AppError::Ssh(format!("打开本地文件失败 {local_path}: {e}")))?;
    let mut remote = sftp
        .create(remote_path)
        .await
        .map_err(|e| AppError::Ssh(format!("创建远程文件失败 {remote_path}: {e}")))?;
    let result: AppResult<()> = async {
        // 块大小 1MB：降低 SFTP 请求-响应往返次数（内网大文件吞吐关键），SSH 协议层会自动分片
        let mut buf = vec![0u8; 1024 * 1024];
        loop {
            ctx.check_cancel()?;
            let n = file
                .read(&mut buf)
                .await
                .map_err(|e| AppError::Ssh(format!("读取本地文件失败: {e}")))?;
            if n == 0 {
                break;
            }
            remote
                .write_all(&buf[..n])
                .await
                .map_err(|e| AppError::Ssh(format!("发送数据失败: {e}")))?;
            ctx.add(n as u64);
        }
        // 显式关闭远端写句柄。AsyncWrite::poll_close 会先 flush 再发 CLOSE。
        // 原来只 flush 不 close：File 的 Drop 虽然同步触发 CLOSE 但无法 await，
        // 深层 node_modules 等成百上千小文件的目录上传时，句柄在服务端堆积，
        // 下一个 sftp.create() 直接报 "Limit exceeded: handle limit reached"。
        remote.close().await.map_err(|e| AppError::Ssh(format!("关闭远程文件失败: {e}")))
    }
    .await;
    if let Err(e) = &result {
        if is_cancelled_err(e) {
            let _ = sftp.remove_file(remote_path).await; // 清理残缺文件
        }
    }
    result
}

async fn copy_file_from_remote(
    ctx: &TransferCtx,
    sftp: &russh_sftp::client::SftpSession,
    remote_path: &str,
    local_path: &str,
) -> AppResult<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut remote = sftp
        .open(remote_path)
        .await
        .map_err(|e| AppError::Ssh(format!("打开远程文件失败 {remote_path}: {e}")))?;
    if let Some(parent) = std::path::Path::new(local_path).parent() {
        tokio::fs::create_dir_all(parent).await.ok();
    }
    let mut file = tokio::fs::File::create(local_path)
        .await
        .map_err(|e| AppError::Ssh(format!("创建本地文件失败 {local_path}: {e}")))?;
    let result: AppResult<()> = async {
        // 块大小 1MB：降低 SFTP 请求-响应往返次数（内网大文件吞吐关键），SSH 协议层会自动分片
        let mut buf = vec![0u8; 1024 * 1024];
        loop {
            ctx.check_cancel()?;
            let n = remote
                .read(&mut buf)
                .await
                .map_err(|e| AppError::Ssh(format!("读取远程文件失败: {e}")))?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n])
                .await
                .map_err(|e| AppError::Ssh(format!("写入本地文件失败: {e}")))?;
            ctx.add(n as u64);
        }
        // 显式关闭远端读句柄（原因同 copy_file_to_remote 的注释）
        remote.close().await.map_err(|e| AppError::Ssh(format!("关闭远程文件失败: {e}")))?;
        file.flush().await.ok();
        Ok(())
    }
    .await;
    if let Err(e) = &result {
        if is_cancelled_err(e) {
            let _ = tokio::fs::remove_file(local_path).await; // 清理残缺文件
        }
    }
    result
}

// ===== SFTP 主通道：文件/目录 =====

async fn upload_via_sftp(
    config: &SshConfig,
    ctx: &TransferCtx,
    local_path: &str,
    remote_path: &str,
) -> AppResult<()> {
    let (sftp, drive) = open_sftp_session(config)
        .await?
        .ok_or_else(|| AppError::Ssh("sftp 子系统不可用".into()))?;
    if let Some(parent) = remote_parent(remote_path) {
        remote_mkdir_p(&sftp, &parent).await;
    }
    let result = copy_file_to_remote(ctx, &sftp, local_path, remote_path).await;
    let _ = sftp.close().await;
    drive.abort();
    result
}

async fn download_via_sftp(
    config: &SshConfig,
    ctx: &TransferCtx,
    remote_path: &str,
    local_path: &str,
) -> AppResult<()> {
    let (sftp, drive) = open_sftp_session(config)
        .await?
        .ok_or_else(|| AppError::Ssh("sftp 子系统不可用".into()))?;
    let result = copy_file_from_remote(ctx, &sftp, remote_path, local_path).await;
    let _ = sftp.close().await;
    drive.abort();
    result
}

async fn upload_dir_via_sftp(
    config: &SshConfig,
    ctx: &TransferCtx,
    files: &[(String, u64)],
    local_root: &std::path::Path,
    remote_root: &str,
) -> AppResult<()> {
    let (sftp, drive) = open_sftp_session(config)
        .await?
        .ok_or_else(|| AppError::Ssh("sftp 子系统不可用".into()))?;
    let result: AppResult<()> = async {
        remote_mkdir_p(&sftp, remote_root).await;
        let mut dirs_created = std::collections::HashSet::new();
        for (rel, _) in files {
            ctx.check_cancel()?;
            if let Some(idx) = rel.rfind('/') {
                let dir_rel = &rel[..idx];
                if dirs_created.insert(dir_rel.to_string()) {
                    remote_mkdir_p(&sftp, &format!("{remote_root}/{dir_rel}")).await;
                }
            }
            let local = local_root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
            let local_str = local.to_string_lossy().to_string();
            copy_file_to_remote(ctx, &sftp, &local_str, &format!("{remote_root}/{rel}")).await?;
        }
        Ok(())
    }
    .await;
    let _ = sftp.close().await;
    drive.abort();
    result
}

async fn download_dir_via_sftp(
    sftp: &russh_sftp::client::SftpSession,
    ctx: &TransferCtx,
    files: &[(String, u64)],
    remote_root: &str,
    local_root: &std::path::Path,
) -> AppResult<()> {
    let result: AppResult<()> = async {
        for (rel, _) in files {
            ctx.check_cancel()?;
            let local = local_root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
            let local_str = local.to_string_lossy().to_string();
            copy_file_from_remote(ctx, sftp, &format!("{remote_root}/{rel}"), &local_str).await?;
        }
        Ok(())
    }
    .await;
    result
}

// ===== base64 兜底通道：文件/目录 =====

/// 单文件上传兜底：`base64 -d > remote` exec 通道灌数据
async fn upload_file_base64(
    config: &SshConfig,
    ctx: &TransferCtx,
    local_path: &str,
    remote_path: &str,
) -> AppResult<()> {
    use base64::Engine;
    use tokio::io::AsyncReadExt;

    let mut file = tokio::fs::File::open(local_path)
        .await
        .map_err(|e| AppError::Ssh(format!("打开本地文件失败 {local_path}: {e}")))?;
    let cmd = format!("base64 -d > {}", shell_quote(remote_path));
    let (mut channel, drive_handle) = open_exec_stream(config, &cmd).await?;

    let result: AppResult<()> = async {
        let engine = base64::engine::general_purpose::STANDARD;
        let mut buf = vec![0u8; 48 * 1024];
        loop {
            ctx.check_cancel()?;
            let n = file
                .read(&mut buf)
                .await
                .map_err(|e| AppError::Ssh(format!("读取本地文件失败: {e}")))?;
            if n == 0 {
                break;
            }
            let encoded = engine.encode(&buf[..n]);
            channel
                .data(encoded.as_bytes())
                .await
                .map_err(|e| AppError::Ssh(format!("发送数据失败: {e}")))?;
            ctx.add(n as u64);
        }
        let _ = channel.eof().await;
        let mut exit_status: Option<u32> = None;
        while let Some(msg) = channel.wait().await {
            match msg {
                russh::ChannelMsg::ExitStatus { exit_status: s } => {
                    exit_status = Some(s);
                    break;
                }
                russh::ChannelMsg::Eof | russh::ChannelMsg::Close => break,
                _ => {}
            }
        }
        if let Some(s) = exit_status {
            if s != 0 {
                return Err(AppError::Ssh(format!("远端写入失败 (exit {s})")));
            }
        }
        Ok(())
    }
    .await;

    let _ = channel.close().await;
    drive_handle.abort();
    if let Err(e) = &result {
        if is_cancelled_err(e) {
            let _ = exec_remote_command(config, &format!("rm -f {}", shell_quote(remote_path))).await;
        }
    }
    result
}

/// 单文件下载兜底：远程 `base64 < remote` 输出流式解码写入本地
async fn download_file_base64(
    config: &SshConfig,
    ctx: &TransferCtx,
    remote_path: &str,
    local_path: &str,
) -> AppResult<()> {
    use base64::Engine;

    let cmd = format!("base64 {}", shell_quote(remote_path));
    let (mut channel, drive_handle) = open_exec_stream(config, &cmd).await?;

    let result: AppResult<()> = async {
        let engine = base64::engine::general_purpose::STANDARD;
        if let Some(parent) = std::path::Path::new(local_path).parent() {
            tokio::fs::create_dir_all(parent).await.ok();
        }
        let mut file = std::fs::File::create(local_path)
            .map_err(|e| AppError::Ssh(format!("创建本地文件失败 {local_path}: {e}")))?;
        use std::io::Write;
        let mut pending: Vec<u8> = Vec::new();
        let mut exit_status: Option<u32> = None;
        while let Some(msg) = channel.wait().await {
            ctx.check_cancel()?;
            match msg {
                russh::ChannelMsg::Data { data } => {
                    pending.extend_from_slice(data.as_ref());
                    let clean: Vec<u8> = pending.iter().copied().filter(|b| !b"\r\n".contains(b)).collect();
                    let decodable = clean.len() / 4 * 4;
                    if decodable > 0 {
                        let decoded = engine
                            .decode(&clean[..decodable])
                            .map_err(|e| AppError::Ssh(format!("Base64 解码失败: {e}")))?;
                        file.write_all(&decoded)
                            .map_err(|e| AppError::Ssh(format!("写入本地文件失败: {e}")))?;
                        ctx.add(decoded.len() as u64);
                        pending = clean[decodable..].to_vec();
                    } else {
                        pending = clean;
                    }
                }
                russh::ChannelMsg::ExitStatus { exit_status: s } => {
                    exit_status = Some(s);
                }
                russh::ChannelMsg::Eof | russh::ChannelMsg::Close => break,
                _ => {}
            }
        }
        let tail: Vec<u8> = pending.iter().copied().filter(|b| !b"\r\n".contains(b)).collect();
        if !tail.is_empty() {
            if let Ok(decoded) = engine.decode(&tail) {
                let _ = file.write_all(&decoded);
                ctx.add(decoded.len() as u64);
            }
        }
        let _ = file.flush();
        if let Some(s) = exit_status {
            if s != 0 {
                return Err(AppError::Ssh(format!("远端读取失败 (exit {s})")));
            }
        }
        Ok(())
    }
    .await;

    let _ = channel.close().await;
    drive_handle.abort();
    if let Err(e) = &result {
        if is_cancelled_err(e) {
            let _ = std::fs::remove_file(local_path);
        }
    }
    result
}

/// 目录上传兜底：一次 `mkdir -p` 批量建目录 + 逐文件 base64 上传
async fn upload_dir_base64(
    config: &SshConfig,
    ctx: &TransferCtx,
    files: &[(String, u64)],
    local_root: &std::path::Path,
    remote_root: &str,
) -> AppResult<()> {
    let mut dirs: Vec<String> = vec![remote_root.to_string()];
    for (rel, _) in files {
        if let Some(idx) = rel.rfind('/') {
            dirs.push(format!("{remote_root}/{}", &rel[..idx]));
        }
    }
    dirs.sort();
    dirs.dedup();
    let quoted: Vec<String> = dirs.iter().map(|d| shell_quote(d)).collect();
    exec_remote_command(config, &format!("mkdir -p {}", quoted.join(" "))).await?;
    for (rel, _) in files {
        ctx.check_cancel()?;
        let local = local_root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        let local_str = local.to_string_lossy().to_string();
        upload_file_base64(config, ctx, &local_str, &format!("{remote_root}/{rel}")).await?;
    }
    Ok(())
}

/// 目录下载兜底：服务端 `tar cf` 打包到临时文件 → base64 拉回 → 本地解包
async fn download_dir_tar(
    config: &SshConfig,
    ctx: &TransferCtx,
    remote_root: &str,
    local_root: &std::path::Path,
) -> AppResult<()> {
    let name = remote_root.trim_end_matches('/').rsplit('/').next().unwrap_or("dir");
    let tmp = format!("/tmp/.jc_shell_dl_{}.tar", std::process::id());
    let parent = remote_parent(remote_root).unwrap_or_else(|| "/".to_string());
    // -C 到父目录按目录名打包：包内条目不带顶层目录名，解包到 <local_root>/<name>
    exec_remote_command(
        config,
        &format!("tar cf {} -C {} {}", shell_quote(&tmp), shell_quote(&parent), shell_quote(name)),
    )
    .await?;
    let size = exec_remote_command(config, &format!("stat -c %s {}", shell_quote(&tmp)))
        .await
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0);
    let tmp_local = std::env::temp_dir().join(format!("jc_shell_dl_{}.tar", std::process::id()));
    let tmp_local_str = tmp_local.to_string_lossy().to_string();
    // 打包文件进度另起（大小与目录总字节不同，仅用于反馈拉取进度）
    let dl_ctx = TransferCtx::new(&ctx.app, &ctx.transfer_id, size, ctx.cancel.clone());
    let result = download_file_base64(config, &dl_ctx, &tmp, &tmp_local_str).await;
    let _ = exec_remote_command(config, &format!("rm -f {}", shell_quote(&tmp))).await;
    result?;
    // 本地解包到 <local_root>/<目录名>
    let dest = local_root.join(name);
    let tmp_local_for_cleanup = tmp_local.clone();
    let unpack = tokio::task::spawn_blocking(move || -> AppResult<()> {
        let value = tmp_local.clone();
        let f = std::fs::File::open(&value)
            .map_err(|e| AppError::Io(std::io::Error::new(e.kind(), format!("打开临时压缩包失败: {e}"))))?;
        std::fs::create_dir_all(&dest)
            .map_err(|e| AppError::Io(std::io::Error::new(e.kind(), format!("创建目标目录失败: {e}"))))?;
        let mut ar = tar::Archive::new(f);
        ar.unpack(&dest)
            .map_err(|e| AppError::Io(std::io::Error::new(e.kind(), format!("解包失败: {e}"))))?;
        Ok(())
    })
    .await
    .map_err(|e| AppError::Internal(format!("解包任务异常: {e}")))?;
    let _ = std::fs::remove_file(&tmp_local_for_cleanup);
    unpack
}

// ===== 对外入口（russh_impl 的 trait 实现调用这里） =====

/// 上传单文件：SFTP 优先，不可用回退 base64
pub(crate) async fn upload_file(
    config: &SshConfig,
    app: &AppHandle,
    transfer_id: &str,
    local_path: &str,
    remote_path: &str,
    cancel: Arc<AtomicBool>,
) -> AppResult<()> {
    let total = std::fs::metadata(local_path).map(|m| m.len()).unwrap_or(0);
    let ctx = TransferCtx::new(app, transfer_id, total, cancel);
    // 探测 SFTP 可用性后必须显式 close + abort：
    // JoinHandle 被 drop 只会「解绑」任务而非终止它，SSH 会话与其 TCP 连接会一直存活，
    // 每次传输泄漏一条连接，累积后耗尽服务端会话资源（表现为连接/重连失败）。
    let sftp_ok = match open_sftp_session(config).await? {
        Some((sftp, drive)) => {
            let _ = sftp.close().await;
            drive.abort();
            true
        }
        None => false,
    };
    let result = if sftp_ok {
        upload_via_sftp(config, &ctx, local_path, remote_path).await
    } else {
        tracing::info!("服务端 sftp 子系统不可用，上传回退 base64 通道");
        upload_file_base64(config, &ctx, local_path, remote_path).await
    };
    ctx.finish(&result);
    result
}

/// 下载单文件：SFTP 优先，不可用回退 base64
pub(crate) async fn download_file(
    config: &SshConfig,
    app: &AppHandle,
    transfer_id: &str,
    remote_path: &str,
    local_path: &str,
    cancel: Arc<AtomicBool>,
) -> AppResult<()> {
    // 先取文件大小（失败不阻断，仅影响百分比精度）
    let total = exec_remote_command(config, &format!("stat -c %s {}", shell_quote(remote_path)))
        .await
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0);
    let ctx = TransferCtx::new(app, transfer_id, total, cancel);
    // 同 upload_file：探测用的会话必须显式关闭，避免每次下载泄漏一条 SSH 连接
    let sftp_ok = match open_sftp_session(config).await? {
        Some((sftp, drive)) => {
            let _ = sftp.close().await;
            drive.abort();
            true
        }
        None => false,
    };
    let result = if sftp_ok {
        download_via_sftp(config, &ctx, remote_path, local_path).await
    } else {
        tracing::info!("服务端 sftp 子系统不可用，下载回退 base64 通道");
        download_file_base64(config, &ctx, remote_path, local_path).await
    };
    ctx.finish(&result);
    result
}

/// 上传本地路径（文件或目录）
pub(crate) async fn upload_path(
    config: &SshConfig,
    app: &AppHandle,
    transfer_id: &str,
    local_path: &str,
    remote_path: &str,
    cancel: Arc<AtomicBool>,
) -> AppResult<()> {
    let meta = std::fs::metadata(local_path)
        .map_err(|e| AppError::Io(std::io::Error::new(e.kind(), format!("读取本地路径失败 {local_path}: {e}"))))?;
    if !meta.is_dir() {
        return upload_file(config, app, transfer_id, local_path, remote_path, cancel).await;
    }
    let local_root = std::path::PathBuf::from(local_path);
    let files = walk_local_files(&local_root)?;
    let total: u64 = files.iter().map(|(_, s)| s).sum();
    let ctx = TransferCtx::new(app, transfer_id, total, cancel);
    // 同 upload_file：探测用的会话必须显式关闭，避免每次目录上传泄漏一条 SSH 连接
    let sftp_ok = match open_sftp_session(config).await? {
        Some((sftp, drive)) => {
            let _ = sftp.close().await;
            drive.abort();
            true
        }
        None => false,
    };
    let result = if sftp_ok {
        upload_dir_via_sftp(config, &ctx, &files, &local_root, remote_path).await
    } else {
        tracing::info!("服务端 sftp 子系统不可用，目录上传回退 base64 通道");
        upload_dir_base64(config, &ctx, &files, &local_root, remote_path).await
    };
    ctx.finish(&result);
    result
}

/// 下载远程路径（文件或目录）
pub(crate) async fn download_path(
    config: &SshConfig,
    app: &AppHandle,
    transfer_id: &str,
    remote_path: &str,
    local_path: &str,
    cancel: Arc<AtomicBool>,
) -> AppResult<()> {
    let local_root = std::path::PathBuf::from(local_path);
    match open_sftp_session(config).await? {
        Some((sftp, drive)) => {
            // 单会话内完成：类型判断 → 目录遍历 → 逐文件拉取（避免重复建连）
            let is_dir = sftp
                .metadata(remote_path)
                .await
                .map(|m| m.file_type().is_dir())
                .map_err(|e| AppError::Ssh(format!("读取远程路径失败 {remote_path}: {e}")))?;
            if !is_dir {
                let total = sftp.metadata(remote_path).await.map(|m| m.len()).unwrap_or(0);
                let ctx = TransferCtx::new(app, transfer_id, total, cancel);
                let result = copy_file_from_remote(&ctx, &sftp, remote_path, local_path).await;
                let _ = sftp.close().await;
                drive.abort();
                ctx.finish(&result);
                return result;
            }
            let walk = walk_remote_files(&sftp, remote_path).await;
            match walk {
                Ok(files) => {
                    let total: u64 = files.iter().map(|(_, s)| s).sum();
                    let ctx = TransferCtx::new(app, transfer_id, total, cancel);
                    let result = download_dir_via_sftp(&sftp, &ctx, &files, remote_path, &local_root).await;
                    let _ = sftp.close().await;
                    drive.abort();
                    ctx.finish(&result);
                    result
                }
                Err(e) => {
                    let _ = sftp.close().await;
                    drive.abort();
                    Err(e)
                }
            }
        }
        None => {
            // 无 sftp：先判断类型；目录走服务端 tar 打包拉回，文件走 base64
            let is_dir = exec_remote_command(
                config,
                &format!("test -d {} && echo DIR || echo FILE", shell_quote(remote_path)),
            )
            .await
            .map(|o| o.trim().contains("DIR"))
            .unwrap_or(false);
            if !is_dir {
                return download_file(config, app, transfer_id, remote_path, local_path, cancel).await;
            }
            let ctx = TransferCtx::new(app, transfer_id, 0, cancel);
            let result = download_dir_tar(config, &ctx, remote_path, &local_root).await;
            ctx.finish(&result);
            result
        }
    }
}

// ===== 本地会话工具（供 local_pty/terminal_cmd 复用的编码转换） =====

/// 按 WHATWG 编码标签把文本编码为字节（未知标签按 UTF-8）
pub fn encode_text(content: &str, encoding: &str) -> Vec<u8> {
    let enc = if encoding.is_empty() {
        encoding_rs::UTF_8
    } else {
        encoding_rs::Encoding::for_label(encoding.as_bytes()).unwrap_or(encoding_rs::UTF_8)
    };
    let (bytes, _, _) = enc.encode(content);
    bytes.into_owned()
}
