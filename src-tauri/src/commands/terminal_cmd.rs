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

//! 终端会话 Tauri Commands —— 真实 SSH 连接
//! 作者：JC

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use once_cell::sync::Lazy;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::common::{AppResult, AppError};
use crate::models::terminal::{ConnectSshDTO, CreateSessionDTO, ResizeDTO, SessionInputDTO, TerminalSession, TerminalSessionState};
use crate::services::ssh::russh_impl;
use crate::services::SshSessionHandle;

/// SSH 状态事件载荷
#[derive(serde::Serialize, Clone)]
struct SshStatusPayload {
    session_id: String,
    event: String,
    message: Option<String>,
}

/// 会话条目
struct SessionEntry {
    info: TerminalSession,
    ssh: Option<Arc<dyn SshSessionHandle>>,
    /// 本地 PTY 会话（跨会话传输时本地侧直接走文件系统操作）
    is_local: bool,
}

/// 会话管理器
pub struct TerminalSessionManager {
    sessions: std::sync::Mutex<HashMap<String, SessionEntry>>,
}

impl TerminalSessionManager {
    pub fn new() -> Self {
        Self {
            sessions: std::sync::Mutex::new(HashMap::new()),
        }
    }
}

impl Default for TerminalSessionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 创建终端会话
#[tauri::command]
pub async fn create_session(
    dto: CreateSessionDTO,
    manager: State<'_, TerminalSessionManager>,
) -> AppResult<TerminalSession> {
    use chrono::Local;
    let id = crate::common::security::gen_session_id();
    let title = dto.title.unwrap_or_else(|| crate::common::DEFAULT_LOCAL_TAB_TITLE.to_string());
    let session = TerminalSession {
        id: id.clone(),
        host_id: dto.host_id,
        title,
        state: TerminalSessionState::Disconnected,
        cols: dto.cols,
        rows: dto.rows,
        created_at: Local::now(),
        error: None,
    };
    manager.sessions.lock().unwrap().insert(id.clone(), SessionEntry {
        info: session.clone(),
        ssh: None,
        is_local: false,
    });
    tracing::debug!(session_id = %session.id, "创建终端会话");
    Ok(session)
}

/// SSH 连接
#[tauri::command]
pub async fn connect_ssh(
    dto: ConnectSshDTO,
    app: AppHandle,
    manager: State<'_, TerminalSessionManager>,
) -> AppResult<TerminalSession> {
    use chrono::Local;
    let id = crate::common::security::gen_session_id();
    let title = format!("{}@{}", dto.username, dto.host);
    let mut session = TerminalSession {
        id: id.clone(),
        host_id: None,
        title,
        state: TerminalSessionState::Connecting,
        cols: dto.cols,
        rows: dto.rows,
        created_at: Local::now(),
        error: None,
    };

    let handle = russh_impl::connect_ssh(
        &app,
        &id,
        &dto.host,
        dto.port,
        &dto.username,
        &dto.auth_type,
        dto.password.as_deref(),
        dto.private_key_path.as_deref(),
        dto.passphrase.as_deref(),
        dto.cols,
        dto.rows,
        dto.connect_timeout_secs,
        dto.keep_alive_secs,
        dto.proxy_addr.as_deref(),
        dto.proxy_kind.as_deref(),
    )
    .await;

    match handle {
        Ok(h) => {
            session.state = TerminalSessionState::Connected;
            manager.sessions.lock().unwrap().insert(id.clone(), SessionEntry {
                info: session.clone(),
                ssh: Some(Arc::from(h)),
                is_local: false,
            });
            let _ = app.emit("ssh-status", SshStatusPayload {
                session_id: id,
                event: "connected".into(),
                message: None,
            });
            Ok(session)
        }
        Err(e) => {
            session.state = TerminalSessionState::Error;
            session.error = Some(e.to_string());
            let _ = app.emit("ssh-status", SshStatusPayload {
                session_id: id,
                event: "error".into(),
                message: Some(e.to_string()),
            });
            Err(e)
        }
    }
}

/// 关闭会话
#[tauri::command]
pub async fn close_session(
    session_id: String,
    manager: State<'_, TerminalSessionManager>,
) -> AppResult<bool> {
    let entry = manager.sessions.lock().unwrap().remove(&session_id);
    if let Some(entry) = entry {
        if let Some(ssh) = entry.ssh {
            let _ = ssh.close().await;
        }
        tracing::debug!(session_id = %session_id, "关闭终端会话");
        Ok(true)
    } else {
        Ok(false)
    }
}

/// 调整终端窗口大小
#[tauri::command]
pub async fn resize_session(
    dto: ResizeDTO,
    manager: State<'_, TerminalSessionManager>,
) -> AppResult<bool> {
    let ssh_handle = {
        let mut lock = manager.sessions.lock().unwrap();
        if let Some(entry) = lock.get_mut(&dto.session_id) {
            entry.info.cols = dto.cols;
            entry.info.rows = dto.rows;
            entry.ssh.clone()
        } else {
            return Ok(false);
        }
    };
    if let Some(ssh) = ssh_handle {
        let _ = ssh.resize(dto.cols, dto.rows).await;
    }
    Ok(true)
}

/// 发送输入到会话
#[tauri::command]
pub async fn send_session_input(
    dto: SessionInputDTO,
    manager: State<'_, TerminalSessionManager>,
) -> AppResult<bool> {
    tracing::info!(session_id = %dto.session_id, bytes = dto.data.len(), "send_session_input 收到前端输入");
    let ssh_handle = {
        let lock = manager.sessions.lock().unwrap();
        lock.get(&dto.session_id)
            .and_then(|e| e.ssh.clone())
    };
    if let Some(ssh) = ssh_handle {
        ssh.write(dto.data.as_bytes()).await?;
        Ok(true)
    } else {
        tracing::warn!(session_id = %dto.session_id, "会话未找到，无法发送输入");
        Err(AppError::Ssh("会话未连接 SSH".into()))
    }
}

/// 执行远程命令入参
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ExecCommandDTO {
    pub session_id: String,
    pub command: String,
}

/// 列出当前所有会话
#[tauri::command]
pub async fn list_sessions(
    manager: State<'_, TerminalSessionManager>,
) -> AppResult<Vec<TerminalSession>> {
    let lock = manager.sessions.lock().unwrap();
    let mut list: Vec<TerminalSession> = lock.values().map(|e| e.info.clone()).collect();
    list.sort_by_key(|s| std::cmp::Reverse(s.created_at));
    Ok(list)
}

/// 执行远程命令（通过 exec channel，独立于 shell channel）
#[tauri::command]
pub async fn exec_remote_command(
    dto: ExecCommandDTO,
    manager: State<'_, TerminalSessionManager>,
) -> AppResult<String> {
    let ssh_handle = {
        let lock = manager.sessions.lock().unwrap();
        lock.get(&dto.session_id)
            .and_then(|e| e.ssh.clone())
    };
    if let Some(ssh) = ssh_handle {
        ssh.exec_command(&dto.command).await
    } else {
        Err(crate::common::AppError::Ssh("会话未连接 SSH".into()))
    }
}

/// 获取交互式 Shell 的当前工作目录（终端↔文件系统联动用）
#[tauri::command]
pub async fn get_shell_cwd(
    session_id: String,
    manager: State<'_, TerminalSessionManager>,
) -> AppResult<String> {
    let ssh_handle = {
        let lock = manager.sessions.lock().unwrap();
        lock.get(&session_id)
            .and_then(|e| e.ssh.clone())
    };
    if let Some(ssh) = ssh_handle {
        ssh.shell_cwd().await
    } else {
        Err(crate::common::AppError::Ssh("会话未连接 SSH".into()))
    }
}

/// 命令名探测入参（Agent 模式区分「直接执行的命令」与「自然语言任务」）
#[derive(Debug, Clone, serde::Deserialize)]
pub struct CommandProbeDTO {
    pub session_id: String,
    pub name: String,
}

/// 探测命令名在会话对应环境中是否真实存在（PATH 中的程序 / shell 内建）。
/// Agent 模式据此判断用户输入的第一个词是不是命令：是则直接交给 shell 执行，不再发给大模型。
/// - 本地 Windows：`where`（只认磁盘可执行文件，内建/别名由前端静态命令表覆盖）
/// - 本地 Unix / SSH 远端：`command -v`（含内建命令）
/// 名称经白名单字符校验后才拼进探测命令，避免把用户输入注入 shell；
/// 探测不到（不存在 / 会话不支持 / 命令报错）统一返回 false，由前端启发式兜底。
#[tauri::command]
pub async fn is_shell_command(
    dto: CommandProbeDTO,
    manager: State<'_, TerminalSessionManager>,
) -> AppResult<bool> {
    let name = dto.name.trim();
    let safe = !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('-')
        && name.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | '\\' | '+' | '~')
        });
    if !safe {
        return Ok(false);
    }
    let ssh = {
        let lock = manager.sessions.lock().unwrap();
        lock.get(&dto.session_id).and_then(|e| e.ssh.clone())
    };
    let ssh = ssh.ok_or_else(|| AppError::Ssh("会话未连接".into()))?;
    let probe = if cfg!(windows) && ssh.is_local() {
        format!("where {name}")
    } else {
        format!("command -v '{name}' >/dev/null 2>&1")
    };
    Ok(ssh.run_shell_checked(&probe).await.is_ok())
}

/// 创建本地终端会话（本地计算机真实 Shell，进入家目录；shell 可选：auto/powershell/cmd/bash/zsh）
#[tauri::command]
pub async fn create_local_shell(
    cols: u16,
    rows: u16,
    app: AppHandle,
    manager: State<'_, TerminalSessionManager>,
    shell: Option<String>,
) -> AppResult<TerminalSession> {
    #[cfg(desktop)]
    {
        use chrono::Local;
        let id = crate::common::security::gen_session_id();
        let session = TerminalSession {
            id: id.clone(),
            host_id: None,
            title: "本地终端".to_string(),
            state: TerminalSessionState::Connected,
            cols,
            rows,
            created_at: Local::now(),
            error: None,
        };
        let handle = crate::services::local_pty::create_local_shell(&app, &id, cols, rows, shell.as_deref())?;
        manager.sessions.lock().unwrap().insert(id.clone(), SessionEntry {
            info: session.clone(),
            ssh: Some(Arc::from(handle)),
            is_local: true,
        });
        let _ = app.emit("ssh-status", SshStatusPayload {
            session_id: id,
            event: "connected".into(),
            message: None,
        });
        Ok(session)
    }
    #[cfg(mobile)]
    {
        let _ = (cols, rows, app, manager, shell);
        Err(AppError::Internal("移动端不支持本地终端".into()))
    }
}

/// 打开独立终端窗口（Tab 拖出）
/// x/y：可选的窗口左上角屏幕物理坐标（拖拽落点），缺省时由系统默认摆放
#[tauri::command]
pub async fn open_terminal_window(
    label: String,
    title: String,
    url: String,
    x: Option<f64>,
    y: Option<f64>,
    app: AppHandle,
) -> AppResult<()> {
    // 移动端兜底：独立终端窗口在 Android 上会永久置顶盖住主窗口（所有窗口
    // 渲染进同一 Activity 视图栈）。移动端终端会话只在主窗口内以 Tab 承载。
    if cfg!(mobile) {
        let _ = (&label, &title, &url, x, y, &app);
        return Err(AppError::Internal("移动端不支持独立终端窗口".into()));
    }
    // 同名窗口已存在则聚焦
    if let Some(win) = app.get_webview_window(&label) {
        let _ = win.set_focus();
        return Ok(());
    }
    let mut builder = tauri::WebviewWindowBuilder::new(&app, &label, tauri::WebviewUrl::App(url.into()))
        .title(title)
        .inner_size(960.0, 640.0)
        .min_inner_size(480.0, 320.0)
        .background_color(tauri::window::Color(31, 31, 35, 255))
        .devtools(false);
    // 标题栏与主窗口一致：macOS 保留原生三圆点（Overlay 标题栏），Windows / Linux 走自绘无边框。
    #[cfg(target_os = "macos")]
    {
        builder = builder
            .decorations(true)
            .title_bar_style(tauri::TitleBarStyle::Overlay)
            .hidden_title(true);
    }
    // 非 macOS 桌面端（Windows / Linux）走自绘无边框 + 阴影。
    // 注意：必须用 `desktop` 而非 `not(macos)`——移动端 Android/iOS 根本没有 decorations/shadow 等
    // 原生窗框 API（Tauri 2 中这些方法被 #[cfg(desktop)] 关闭），否则移动端编译失败。
    #[cfg(all(desktop, not(target_os = "macos")))]
    {
        builder = builder.decorations(false).shadow(true);
    }
    if let (Some(x), Some(y)) = (x, y) {
        builder = builder.position(x, y);
    }
    // ⚠️ 所有窗口属性（可见性/尺寸/位置/装饰）都必须在 build() 前经 builder 配置。
    // build() 只是把 Message::CreateWindow 投递到主线程消息队列后立即返回
    // （见 tauri-runtime-wry 的 Context::create_window），窗口此刻尚未登记进主线程窗口表；
    // 若在 build() 之后立刻调 win.show()/set_position() 等，消息会先于窗口登记被处理并被丢弃，
    // 结果是窗口以隐藏态创建后永远不显示（表现为"拖出后标签消失、新窗口不出现"）。
    let _win = builder
        .build()
        .map_err(|e| AppError::Internal(format!("创建独立终端窗口失败: {e}")))?;
    // Windows: debug 构建下 devtools:false 会被忽略，需经原生 WebView2 接口禁用（与 lib.rs 主窗口一致）。
    // 该函数要遍历全部窗口逐个 with_webview，放到后台线程延迟执行，不占用窗口创建的热路径。
    #[cfg(target_os = "windows")]
    {
        let h = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            crate::disable_devtools_all(&h);
        });
    }
    Ok(())
}

/// 文件上传/下载入参（本地路径可为文件或目录）
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTransferDTO {
    pub session_id: String,
    pub transfer_id: String,
    pub local_path: String,
    pub remote_path: String,
}

// ===== 传输取消注册表（前端取消按钮 → 置位标志，传输循环逐块检查） =====
static TRANSFER_CANCELS: Lazy<std::sync::Mutex<HashMap<String, Arc<AtomicBool>>>> =
    Lazy::new(|| std::sync::Mutex::new(HashMap::new()));

/// 传输开始：登记取消标志（结束时须 end_transfer 清理）
fn begin_transfer(transfer_id: &str) -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    TRANSFER_CANCELS.lock().unwrap().insert(transfer_id.to_string(), flag.clone());
    flag
}

fn end_transfer(transfer_id: &str) {
    TRANSFER_CANCELS.lock().unwrap().remove(transfer_id);
}

/// 取消进行中的传输（后端在下一个数据块边界响应，清理残缺文件并发送完成事件）
#[tauri::command]
pub fn cancel_transfer(transfer_id: String) -> AppResult<bool> {
    if let Some(flag) = TRANSFER_CANCELS.lock().unwrap().get(&transfer_id) {
        flag.store(true, Ordering::Relaxed);
        tracing::info!(%transfer_id, "收到传输取消请求");
        Ok(true)
    } else {
        Ok(false)
    }
}

/// 上传本地文件/目录到远程（目录递归上传，进度按总字节聚合，可取消）
#[tauri::command]
pub async fn upload_file(
    dto: FileTransferDTO,
    app: AppHandle,
    manager: State<'_, TerminalSessionManager>,
) -> AppResult<()> {
    let ssh_handle = {
        let lock = manager.sessions.lock().unwrap();
        lock.get(&dto.session_id).and_then(|e| e.ssh.clone())
    };
    let ssh = ssh_handle.ok_or_else(|| AppError::Ssh("会话未连接".into()))?;
    let cancel = begin_transfer(&dto.transfer_id);
    let result = ssh.upload_path(&app, &dto.transfer_id, &dto.local_path, &dto.remote_path, cancel).await;
    end_transfer(&dto.transfer_id);
    result
}

/// 手机端上传入参：WebView 文件选择器拿不到真实路径，前端把文件字节 base64 编码
/// 走 IPC 传来，这里落临时文件后复用 upload_path 全链路
/// （SFTP 流式上传 / transfer-progress 进度 / 可取消 / exec 兜底）。
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadBytesDTO {
    pub session_id: String,
    pub transfer_id: String,
    pub remote_path: String,
    #[serde(default)]
    pub file_name: Option<String>,
    pub data_base64: String,
}

/// 手机端上传：把前端传来的文件字节写入远程（带进度，可取消）
#[tauri::command]
pub async fn upload_file_bytes(
    dto: UploadBytesDTO,
    app: AppHandle,
    manager: State<'_, TerminalSessionManager>,
) -> AppResult<()> {
    use base64::Engine;
    let data = base64::engine::general_purpose::STANDARD
        .decode(dto.data_base64.as_bytes())
        .map_err(|e| AppError::Ssh(format!("文件数据 base64 解码失败: {e}")))?;

    // 文件名只留最后一段并去掉路径分隔符，避免污染临时文件路径
    let name = dto
        .file_name
        .unwrap_or_else(|| "file".into())
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("file")
        .to_string();
    let tmp = std::env::temp_dir().join(format!("jc-upload-{}-{}", dto.transfer_id, name));
    tokio::fs::write(&tmp, &data)
        .await
        .map_err(|e| AppError::Ssh(format!("写入临时文件失败 {}: {e}", tmp.display())))?;

    let ssh_handle = {
        let lock = manager.sessions.lock().unwrap();
        lock.get(&dto.session_id).and_then(|e| e.ssh.clone())
    };
    let ssh = ssh_handle.ok_or_else(|| AppError::Ssh("会话未连接".into()))?;
    let cancel = begin_transfer(&dto.transfer_id);
    let result = ssh
        .upload_path(&app, &dto.transfer_id, tmp.to_string_lossy().as_ref(), &dto.remote_path, cancel)
        .await;
    end_transfer(&dto.transfer_id);
    let _ = tokio::fs::remove_file(&tmp).await;
    result
}

/// 下载远程文件/目录到本地（可取消）
#[tauri::command]
pub async fn download_file(
    dto: FileTransferDTO,
    app: AppHandle,
    manager: State<'_, TerminalSessionManager>,
) -> AppResult<()> {
    let ssh_handle = {
        let lock = manager.sessions.lock().unwrap();
        lock.get(&dto.session_id).and_then(|e| e.ssh.clone())
    };
    let ssh = ssh_handle.ok_or_else(|| AppError::Ssh("会话未连接".into()))?;
    let cancel = begin_transfer(&dto.transfer_id);
    let result = ssh.download_path(&app, &dto.transfer_id, &dto.remote_path, &dto.local_path, cancel).await;
    end_transfer(&dto.transfer_id);
    result
}

/// 跨会话传输入参（支持文件与目录；src_type 缺省按文件处理）
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteTransferDTO {
    pub src_session_id: String,
    pub dst_session_id: String,
    pub src_path: String,
    pub dst_path: String,
    pub transfer_id: String,
    #[serde(default)]
    pub src_type: Option<String>,
    /// 源/目标端类型：客户端显式传入 local / ssh。为空时回退按会话表里登记的类型判断。
    /// 用于 FTP 本地窗格这类无会话记录的占位 id（session_id='local'），
    /// 否则本地端在会话表中查不到 SSH 句柄，无法与远程互传。
    #[serde(default)]
    pub src_kind: Option<String>,
    #[serde(default)]
    pub dst_kind: Option<String>,
}

/// 跨会话文件/目录传输：按两端会话类型（本地 PTY / SSH）选择最优通道，可取消
/// - ssh  → ssh  ：下载到本地临时路径再上传（中转；目录用临时目录）
/// - local→ ssh  ：本地路径直接经目标会话上传（目录递归）
/// - ssh  → local：源会话直接下载到本地目标路径（目录递归）
/// - local→ local：本机流式复制（目录递归，无需中转）
#[tauri::command]
pub async fn transfer_remote_file(
    dto: RemoteTransferDTO,
    app: AppHandle,
    manager: State<'_, TerminalSessionManager>,
) -> AppResult<()> {
    let (src_ssh, dst_ssh, src_local, dst_local) = {
        let lock = manager.sessions.lock().unwrap();
        let s = lock.get(&dto.src_session_id);
        let d = lock.get(&dto.dst_session_id);
        // 优先取客户端显式声明的端类型；未声明时回退按会话表登记类型判断
        let src_local = match &dto.src_kind { Some(k) => k == "local", None => s.map(|e| e.is_local).unwrap_or(false) };
        let dst_local = match &dto.dst_kind { Some(k) => k == "local", None => d.map(|e| e.is_local).unwrap_or(false) };
        (
            if src_local { None } else { s.and_then(|e| e.ssh.clone()) },
            if dst_local { None } else { d.and_then(|e| e.ssh.clone()) },
            src_local,
            dst_local,
        )
    };
    let is_dir = dto.src_type.as_deref() == Some("dir");
    let cancel = begin_transfer(&dto.transfer_id);
    let result = async {
        match (src_local, dst_local) {
            (true, true) => {
                copy_path_local(&app, &dto.transfer_id, &dto.src_path, &dto.dst_path, &cancel).await
            }
            (true, false) => {
                let dst = dst_ssh.ok_or_else(|| AppError::Ssh("目标会话未连接".into()))?;
                dst.upload_path(&app, &dto.transfer_id, &dto.src_path, &dto.dst_path, cancel.clone()).await
            }
            (false, true) => {
                let src = src_ssh.ok_or_else(|| AppError::Ssh("源会话未连接".into()))?;
                src.download_path(&app, &dto.transfer_id, &dto.src_path, &dto.dst_path, cancel.clone()).await
            }
            (false, false) => {
                // ssh→ssh 中转：目录用临时目录，文件用临时文件；两段共用同一进度条目（后段覆盖前段进度）
                let src = src_ssh.ok_or_else(|| AppError::Ssh("源会话未连接".into()))?;
                let dst = dst_ssh.ok_or_else(|| AppError::Ssh("目标会话未连接".into()))?;
                let tmp = std::env::temp_dir().join(format!(
                    "jc_transfer_{}{}",
                    crate::common::security::gen_session_id(),
                    if is_dir { "_dir" } else { "" }
                ));
                let tmp_str = tmp.to_string_lossy().to_string();
                let result = async {
                    src.download_path(&app, &dto.transfer_id, &dto.src_path, &tmp_str, cancel.clone()).await?;
                    dst.upload_path(&app, &dto.transfer_id, &tmp_str, &dto.dst_path, cancel.clone()).await
                }
                .await;
                if is_dir {
                    let _ = std::fs::remove_dir_all(&tmp);
                } else {
                    let _ = std::fs::remove_file(&tmp);
                }
                result
            }
        }
    }
    .await;
    end_transfer(&dto.transfer_id);
    result
}

/// 传输进度事件载荷（与 russh_impl 保持一致，前端共用同一事件）
#[derive(serde::Serialize, Clone)]
struct TransferProgressPayload {
    transfer_id: String,
    transferred: u64,
    total: u64,
    done: bool,
    error: Option<String>,
}

fn emit_transfer_progress(app: &AppHandle, transfer_id: &str, transferred: u64, total: u64, done: bool, error: Option<String>) {
    let _ = app.emit("transfer-progress", TransferProgressPayload {
        transfer_id: transfer_id.to_string(),
        transferred,
        total,
        done,
        error,
    });
}

/// 本地递归遍历目录，返回 (绝对路径, 相对路径, 字节数)
fn walk_local_dir(root: &std::path::Path) -> AppResult<Vec<(std::path::PathBuf, String, u64)>> {
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
                let rel = p.strip_prefix(root).map(|r| r.to_string_lossy().to_string()).unwrap_or_default();
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                if !rel.is_empty() {
                    out.push((p, rel, size));
                }
            }
        }
    }
    Ok(out)
}

/// 本机路径复制（文件或目录，带聚合进度与取消），供 local→local 传输与本地拖入复制共用。
/// 目标语义与 cp/mv 一致：dst 已存在为目录时放进去，否则作为目标全路径。
async fn copy_path_local(
    app: &AppHandle,
    transfer_id: &str,
    src: &str,
    dst: &str,
    cancel: &Arc<AtomicBool>,
) -> AppResult<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let src_path = std::path::Path::new(src);
    if !src_path.exists() {
        return Err(AppError::NotFound(format!("源路径不存在: {src}")));
    }
    // 目标是已存在目录 → 放进去（与拖拽语义一致）
    let dst_buf = if std::path::Path::new(dst).is_dir() {
        let name = src_path.file_name().ok_or_else(|| AppError::Validation(format!("无效源路径: {src}")))?;
        std::path::Path::new(dst).join(name)
    } else {
        std::path::PathBuf::from(dst)
    };

    let is_dir = src_path.is_dir();
    let files: Vec<(std::path::PathBuf, String, u64)> = if is_dir {
        walk_local_dir(src_path)?
    } else {
        let size = std::fs::metadata(src_path).map(|m| m.len()).unwrap_or(0);
        vec![(src_path.to_path_buf(), String::new(), size)]
    };
    let total: u64 = files.iter().map(|(_, _, s)| s).sum();

    let result: AppResult<()> = async {
        let mut transferred: u64 = 0;
        let mut last_emit = tokio::time::Instant::now();
        for (from_path, rel, _) in &files {
            if cancel.load(Ordering::Relaxed) {
                return Err(AppError::Ssh("传输已取消".into()));
            }
            let to_path = if rel.is_empty() { dst_buf.clone() } else { dst_buf.join(rel) };
            if let Some(parent) = to_path.parent() {
                tokio::fs::create_dir_all(parent).await.ok();
            }
            let mut from = tokio::fs::File::open(from_path)
                .await
                .map_err(|e| AppError::Io(std::io::Error::new(e.kind(), format!("打开源文件失败 {}: {e}", from_path.display()))))?;
            let mut to = tokio::fs::File::create(&to_path)
                .await
                .map_err(|e| AppError::Io(std::io::Error::new(e.kind(), format!("创建目标文件失败 {}: {e}", to_path.display()))))?;
            let mut buf = vec![0u8; 256 * 1024];
            loop {
                if cancel.load(Ordering::Relaxed) {
                    return Err(AppError::Ssh("传输已取消".into()));
                }
                let n = from
                    .read(&mut buf)
                    .await
                    .map_err(|e| AppError::Io(std::io::Error::new(e.kind(), format!("读取源文件失败: {e}"))))?;
                if n == 0 {
                    break;
                }
                to.write_all(&buf[..n])
                    .await
                    .map_err(|e| AppError::Io(std::io::Error::new(e.kind(), format!("写入目标文件失败: {e}"))))?;
                transferred += n as u64;
                // 本地复制可达 GB/s，逐块 emit 会洪水式冲击前端；节流 200ms
                if last_emit.elapsed() >= std::time::Duration::from_millis(200) {
                    last_emit = tokio::time::Instant::now();
                    emit_transfer_progress(app, transfer_id, transferred, total, false, None);
                }
            }
            to.flush().await.ok();
        }
        Ok(())
    }
    .await;

    if matches!(&result, Err(AppError::Ssh(m)) if m == "传输已取消") {
        // 取消：清理已复制的残留（目录删目标根，文件删目标文件）
        if is_dir {
            let _ = std::fs::remove_dir_all(&dst_buf);
        } else {
            let _ = std::fs::remove_file(&dst_buf);
        }
        emit_transfer_progress(app, transfer_id, 0, total, true, Some("传输已取消".into()));
    } else {
        match &result {
            Ok(_) => emit_transfer_progress(app, transfer_id, total, total, true, None),
            Err(e) => {
                if is_dir {
                    let _ = std::fs::remove_dir_all(&dst_buf);
                } else {
                    let _ = std::fs::remove_file(&dst_buf);
                }
                emit_transfer_progress(app, transfer_id, 0, total, true, Some(e.to_string()));
            }
        }
    }
    result
}

/// 本机路径移动（同会话内拖拽整理；支持文件与目录；跨盘文件退化为复制+删除）
#[tauri::command]
pub async fn move_local_file(src: String, dst: String) -> AppResult<String> {
    let s = std::path::Path::new(&src);
    if !s.exists() {
        return Err(AppError::NotFound(format!("源文件不存在: {src}")));
    }
    let is_dir = s.is_dir();
    if std::path::Path::new(&dst).exists() {
        if is_dir {
            return Err(AppError::Validation(format!("目标已存在: {dst}")));
        }
        std::fs::remove_file(&dst)
            .map_err(|e| AppError::Io(std::io::Error::new(e.kind(), format!("覆盖目标文件失败 {dst}: {e}"))))?;
    }
    match std::fs::rename(&src, &dst) {
        Ok(_) => Ok(dst),
        Err(e) => {
            if is_dir {
                return Err(AppError::Io(std::io::Error::new(e.kind(), format!("目录移动失败（可能跨磁盘，请先压缩后传输）: {e}"))));
            }
            // 跨盘 rename 失败：复制后删除源（目录不做隐式深拷贝，避免长时间阻塞）
            std::fs::copy(&src, &dst)
                .map_err(|e| AppError::Io(std::io::Error::new(e.kind(), format!("移动文件失败: {e}"))))?;
            std::fs::remove_file(&src).ok();
            Ok(dst)
        }
    }
}

/// 获取本地计算机的家目录（用于本地终端默认打开位置）
#[tauri::command]
pub fn get_local_home_dir() -> AppResult<String> {
    // Windows: %USERPROFILE%；Unix: $HOME
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .map_err(|e| crate::common::AppError::Ssh(format!("无法获取家目录: {e}")))
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME")
            .map_err(|e| crate::common::AppError::Ssh(format!("无法获取家目录: {e}")))
    }
}

/// 本地目录条目（本地终端会话的文件系统浏览用，替代 SSH 的 ls -la）
#[derive(serde::Serialize)]
pub struct LocalFileEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub file_type: String,
    pub size: u64,
    pub modified: String,
    pub permissions: String,
    pub owner: String,
}

/// 列出本地目录内容（本地终端会话无法走 SSH exec，直接读本机文件系统）
///
/// 注意：目录可能是大目录（每项一次 metadata 系统调用）或网络盘，
/// 必须放阻塞线程池执行 —— 同步命令会跑在 UI 主线程上，把整个应用卡住。
#[tauri::command]
pub async fn list_local_dir(path: String) -> AppResult<Vec<LocalFileEntry>> {
    crate::common::blocking::run(move || list_local_dir_inner(path)).await
}

fn list_local_dir_inner(path: String) -> AppResult<Vec<LocalFileEntry>> {
    let dir = std::path::PathBuf::from(&path);
    let read = std::fs::read_dir(&dir)
        .map_err(|e| AppError::Io(std::io::Error::new(e.kind(), format!("读取本地目录失败 {path}: {e}"))))?;
    let owner = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_default();
    let mut list = Vec::new();
    for entry in read.flatten() {
        let ft = entry.file_type().ok();
        let file_type = if ft.as_ref().map(|t| t.is_symlink()).unwrap_or(false) {
            "symlink"
        } else if ft.as_ref().map(|t| t.is_dir()).unwrap_or(false) {
            "dir"
        } else {
            "file"
        };
        let meta = entry.metadata().ok();
        let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        // 修改时间格式化为本地时间 yyyy-MM-dd HH:mm
        let modified = meta
            .as_ref()
            .and_then(|m| m.modified().ok())
            .map(|t| chrono::DateTime::<chrono::Local>::from(t).format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_default();
        let permissions = local_permissions(meta.as_ref());
        list.push(LocalFileEntry {
            name: entry.file_name().to_string_lossy().to_string(),
            file_type: file_type.to_string(),
            size,
            modified,
            permissions,
            owner: owner.clone(),
        });
    }
    Ok(list)
}

/// 获取本地逻辑根（Windows = 可用驱动器列表；Unix = 仅包含 /）
///
/// 驱动器枚举在存在离线映射盘/U 盘时会阻塞，放阻塞线程池执行。
#[tauri::command]
pub async fn list_local_roots() -> AppResult<Vec<String>> {
    crate::common::blocking::run(list_local_roots_inner).await
}

fn list_local_roots_inner() -> AppResult<Vec<String>> {
    #[cfg(windows)]
    {
        extern "system" {
            fn GetLogicalDriveStringsW(cch_buffer: u32, lp_buffer: *mut u16) -> u32;
        }
        unsafe {
            let need = GetLogicalDriveStringsW(0, std::ptr::null_mut());
            if need == 0 {
                return Ok(vec!["C:\\".to_string()]);
            }
            let mut buf: Vec<u16> = vec![0u16; need as usize];
            let wrote = GetLogicalDriveStringsW(need, buf.as_mut_ptr());
            if wrote == 0 || wrote > need {
                return Ok(vec!["C:\\".to_string()]);
            }
            let mut drives = Vec::new();
            let mut start = 0usize;
            for i in 0..(wrote as usize) {
                if buf[i] == 0 {
                    if i > start {
                        let s = String::from_utf16_lossy(&buf[start..i]);
                        let p = s.trim().to_string();
                        if !p.is_empty() { drives.push(p); }
                    }
                    start = i + 1;
                }
            }
            if drives.is_empty() { drives.push("C:\\".to_string()); }
            Ok(drives)
        }
    }
    #[cfg(not(windows))]
    {
        Ok(vec!["/".to_string()])
    }
}

/// 权限字符串：Unix 用 mode 生成 rwx 串；Windows 仅区分只读
fn local_permissions(meta: Option<&std::fs::Metadata>) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.map(|m| {
            let mode = m.permissions().mode();
            let mut s = String::with_capacity(9);
            for shift in (0..3).rev() {
                let bits = (mode >> (shift * 3)) & 0b111;
                s.push(if bits & 0b100 != 0 { 'r' } else { '-' });
                s.push(if bits & 0b010 != 0 { 'w' } else { '-' });
                s.push(if bits & 0b001 != 0 { 'x' } else { '-' });
            }
            s
        })
        .unwrap_or_else(|| "---------".to_string())
    }
    #[cfg(not(unix))]
    {
        meta.map(|m| if m.permissions().readonly() { "r--".to_string() } else { "rw-".to_string() })
            .unwrap_or_else(|| "---".to_string())
    }
}

/// 终端缓冲区暂存表（Tab 拖出/拖回跨窗口迁移时延续 scrollback，内存态不落盘）
static TERM_BUFFERS: once_cell::sync::Lazy<std::sync::Mutex<HashMap<String, String>>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(HashMap::new()));

/// 暂存终端缓冲区（源窗口拖出 Tab 前调用）
#[tauri::command]
pub fn stash_term_buffer(tab_id: String, data: String) -> AppResult<()> {
    TERM_BUFFERS.lock().unwrap().insert(tab_id, data);
    Ok(())
}

/// 取出并移除暂存的终端缓冲区（接管窗口还原时调用）
#[tauri::command]
pub fn take_term_buffer(tab_id: String) -> AppResult<Option<String>> {
    Ok(TERM_BUFFERS.lock().unwrap().remove(&tab_id))
}

/// 本地文件/目录复制（外部拖入本地终端；支持目录递归复制；带进度与取消）
#[tauri::command]
pub async fn copy_local_file(app: AppHandle, src: String, dst_dir: String, transfer_id: String) -> AppResult<String> {
    let src_path = std::path::Path::new(&src);
    if !src_path.exists() {
        return Err(AppError::NotFound(format!("源文件不存在: {src}")));
    }
    let file_name = src_path
        .file_name()
        .ok_or_else(|| AppError::Validation(format!("无效源文件路径: {src}")))?;
    let dst = std::path::Path::new(&dst_dir).join(file_name);
    let dst_str = dst.to_string_lossy().to_string();
    let cancel = begin_transfer(&transfer_id);
    let result = copy_path_local(&app, &transfer_id, &src, &dst_str, &cancel).await;
    end_transfer(&transfer_id);
    result?;
    tracing::info!(%src, dst = %dst.display(), "本地复制完成");
    Ok(dst_str)
}

/// 删除本地文件或目录（文件系统右键删除；目录递归删除，前端需二次确认）
///
/// 递归删除大目录可能耗时数秒到数分钟，必须离开 UI 主线程。
#[tauri::command]
pub async fn delete_local_path(path: String) -> AppResult<()> {
    crate::common::blocking::run(move || delete_local_path_inner(path)).await
}

fn delete_local_path_inner(path: String) -> AppResult<()> {
    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err(AppError::NotFound(format!("路径不存在: {path}")));
    }
    if p.is_dir() {
        std::fs::remove_dir_all(p)
            .map_err(|e| AppError::Io(std::io::Error::new(e.kind(), format!("删除目录失败 {path}: {e}"))).into())
    } else {
        std::fs::remove_file(p)
            .map_err(|e| AppError::Io(std::io::Error::new(e.kind(), format!("删除文件失败 {path}: {e}"))).into())
    }
}

/// 弹出系统文件夹选择对话框（目录下载选本地落点；取消返回 null）
#[tauri::command]
pub fn pick_folder_path(title: String) -> AppResult<Option<String>> {
    #[cfg(desktop)]
    {
        let dir = rfd::FileDialog::new().set_title(&title).pick_folder();
        Ok(dir.map(|p| p.to_string_lossy().to_string()))
    }
    #[cfg(mobile)]
    {
        let _ = title;
        Ok(None)
    }
}

/// 在系统文件管理器中打开本地路径（高亮文件）；跨平台：Windows=explorer /select，
/// macOS=open -R，Linux=xdg-open 父目录（多数 Linux 文件管理器不提供 reveal-in-dir）
#[tauri::command]
pub async fn open_in_file_manager(path: String) -> AppResult<()> {
    crate::common::blocking::run(move || open_in_file_manager_inner(path)).await
}

fn open_in_file_manager_inner(path: String) -> AppResult<()> {
    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err(AppError::NotFound(format!("路径不存在: {path}")));
    }
    #[cfg(target_os = "windows")]
    {
        // explorer /select,"path" 打开父目录并高亮目标文件；路径必须加引号，
        // 否则含空格/中文时 explorer 无法解析参数，表现为点击无反应
        std::process::Command::new("explorer")
            .arg(format!("/select,\"{}\"", path))
            .spawn()
            .map_err(|e| AppError::Io(std::io::Error::new(e.kind(), format!("打开文件管理器失败: {e}"))))?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&path)
            .spawn()
            .map_err(|e| AppError::Io(std::io::Error::new(e.kind(), format!("打开文件管理器失败: {e}"))))?;
        Ok(())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // Linux：xdg-open 父目录
        let parent = p.parent().unwrap_or(p);
        std::process::Command::new("xdg-open")
            .arg(parent)
            .spawn()
            .map_err(|e| AppError::Io(std::io::Error::new(e.kind(), format!("打开文件管理器失败: {e}"))))?;
        Ok(())
    }
}

/// 读取会话内文件原始字节（文本编辑器用；本地/SSH 均可；超过 max_bytes 报错）
#[tauri::command]
pub async fn read_remote_file(
    session_id: String,
    path: String,
    max_bytes: Option<u64>,
    manager: State<'_, TerminalSessionManager>,
) -> AppResult<Vec<u8>> {
    let ssh_handle = {
        let lock = manager.sessions.lock().unwrap();
        lock.get(&session_id).and_then(|e| e.ssh.clone())
    };
    let ssh = ssh_handle.ok_or_else(|| AppError::Ssh("会话未连接".into()))?;
    let max = max_bytes.unwrap_or(16 * 1024 * 1024);
    if ssh.is_local() {
        // 本地会话先查大小，避免大文件先读后拒
        let len = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        if len > max {
            return Err(AppError::Validation(format!("文件过大（{len} 字节），超出编辑器上限")));
        }
    }
    ssh.read_file_bytes(&path, max).await
}

/// 以指定编码把文本写回会话内文件（本地/SSH 均可；encoding 为 WHATWG 标签如 utf-8/gbk）
#[tauri::command]
pub async fn write_remote_file(
    session_id: String,
    path: String,
    content: String,
    encoding: String,
    manager: State<'_, TerminalSessionManager>,
) -> AppResult<()> {
    let ssh_handle = {
        let lock = manager.sessions.lock().unwrap();
        lock.get(&session_id).and_then(|e| e.ssh.clone())
    };
    let ssh = ssh_handle.ok_or_else(|| AppError::Ssh("会话未连接".into()))?;
    ssh.write_file_text(&path, &content, &encoding).await
}

// ===== 压缩/解压（依赖目标机器上的常见工具：7z/7za/zip/unzip/tar/xz/gzip/bzip2/unrar；按检测结果选通道） =====

/// POSIX 单引号转义（SSH / Unix 本地）
fn sh_q(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Windows cmd 双引号转义（本地 Windows 会话）
fn win_q(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// 检测目标机器可用的压缩/解压工具（前端据此展示可选格式）
#[tauri::command]
pub async fn detect_archive_tools(
    session_id: String,
    manager: State<'_, TerminalSessionManager>,
) -> AppResult<Vec<String>> {
    let ssh = {
        let lock = manager.sessions.lock().unwrap();
        lock.get(&session_id).and_then(|e| e.ssh.clone())
    };
    let ssh = ssh.ok_or_else(|| AppError::Ssh("会话未连接".into()))?;
    let out = if ssh.is_local() && cfg!(windows) {
        let mut out = ssh.run_shell_checked("for %t in (7z 7za tar zip unzip unrar rar zstd) do @where %t >nul 2>nul && echo %t").await.unwrap_or_default();
        // 兜底探测 7-Zip 默认安装目录（已安装但未加入 PATH 时 where 找不到）
        for p in ["C:\\Program Files\\7-Zip\\7z.exe", "C:\\Program Files (x86)\\7-Zip\\7z.exe"] {
            if !out.contains("7z") && std::path::Path::new(p).exists() {
                out.push('\n');
                out.push_str(p);
            }
        }
        out
    } else {
        ssh.run_shell_checked(
            "for t in 7z 7za zip unzip tar xz gzip bzip2 zstd rar unrar unar bsdtar; do command -v $t >/dev/null 2>&1 && echo $t; done",
        )
        .await
        .unwrap_or_default()
    };
    Ok(out.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
}

/// 取命令列表（检测输出可能含路径）
fn tool_cmd<'a>(tools: &'a [String], name: &str) -> Option<&'a str> {
    tools.iter().find(|t| *t == name || t.ends_with(&format!("/{name}")) || t.ends_with(&format!("\\{name}")) || t.ends_with(&format!("\\{name}.exe"))).map(|t| t.as_str())
}

/// 列出远程单个目录（文件浏览器用）。
/// 走 SFTP read_dir，不经过 shell —— 与终端 exec 通道解耦，
/// 因此跑部署脚本 / 大文件上传等占满 exec 通道的场景下也不会周期性失败。
#[tauri::command]
pub async fn list_remote_dir(
    session_id: String,
    path: String,
    manager: State<'_, TerminalSessionManager>,
) -> AppResult<Vec<crate::services::ssh::transfer::RemoteDirEntry>> {
    let ssh = {
        let lock = manager.sessions.lock().unwrap();
        lock.get(&session_id).and_then(|e| e.ssh.clone())
    };
    let ssh = ssh.ok_or_else(|| AppError::Ssh("会话未连接".into()))?;
    ssh.list_dir(&path).await
}

/// 压缩：把 sources（文件/目录）打包为 output（完整路径）；format: tar/tar.gz/tar.bz2/tar.xz/tar.zst/zip/7z/rar
#[tauri::command]
pub async fn archive_compress(
    session_id: String,
    sources: Vec<String>,
    output: String,
    format: String,
    manager: State<'_, TerminalSessionManager>,
) -> AppResult<()> {
    let ssh = {
        let lock = manager.sessions.lock().unwrap();
        lock.get(&session_id).and_then(|e| e.ssh.clone())
    };
    let ssh = ssh.ok_or_else(|| AppError::Ssh("会话未连接".into()))?;
    if sources.is_empty() {
        return Err(AppError::Validation("未选择要压缩的文件".into()));
    }
    let win = ssh.is_local() && cfg!(windows);
    let q: fn(&str) -> String = if win { win_q } else { sh_q };
    let tools = detect_archive_tools(session_id.clone(), manager).await?;

    // 相对化：把源路径拆为（父目录, 名字），包内不带绝对路径前缀（行为与资源管理器/右键压缩一致）
    let mut parents = Vec::new();
    let mut names = Vec::new();
    for s in &sources {
        let norm = s.trim_end_matches(['/', '\\']);
        let idx = norm.rfind(['/', '\\']).unwrap_or(0);
        let (parent, name) = if idx == 0 { ("/".to_string(), norm.to_string()) } else { (norm[..idx].to_string(), norm[idx + 1..].to_string()) };
        if parents.iter().all(|p: &String| *p != parent) {
            parents.push(parent);
        }
        names.push(name);
    }
    if parents.len() > 1 {
        return Err(AppError::Validation("多选压缩要求所选项目位于同一目录，请逐个压缩".into()));
    }
    let parent = parents.into_iter().next().unwrap_or_else(|| "/".to_string());
    let cwd_arg = if win { format!("cd /d {} && ", win_q(&parent)) } else { format!("cd {} && ", sh_q(&parent)) };
    let srcs = names.iter().map(|n| q(n)).collect::<Vec<_>>().join(" ");

    let fmt = format.to_ascii_lowercase();
    let cmd = match fmt.as_str() {
        "tar" => format!("{cwd_arg}tar cf {} {srcs}", q(&output)),
        "tar.gz" | "tgz" => format!("{cwd_arg}tar czf {} {srcs}", q(&output)),
        "tar.bz2" | "tbz2" => format!("{cwd_arg}tar cjf {} {srcs}", q(&output)),
        "tar.xz" | "txz" => format!("{cwd_arg}tar cJf {} {srcs}", q(&output)),
        "tar.zst" | "tzst" => {
            if win {
                // Win10+ 内置 bsdtar（libarchive 含 zstd 支持）
                format!("{cwd_arg}tar --zstd -cf {} {srcs}", q(&output))
            } else if let Some(zstd) = tool_cmd(&tools, "zstd") {
                // 管道形式兼容不识别 --zstd 选项的老 tar
                format!("{cwd_arg}tar cf - {srcs} | {zstd} -q -o {} -", q(&output))
            } else {
                return Err(AppError::Ssh("目标机器缺少 zstd 工具，无法创建 tar.zst（apt install zstd）".into()));
            }
        }
        "zip" => {
            if let Some(zip) = tool_cmd(&tools, "zip") {
                format!("{cwd_arg}{zip} -r -q {} {srcs}", q(&output))
            } else if let Some(z) = tool_cmd(&tools, "7z").or_else(|| tool_cmd(&tools, "7za")) {
                format!("{cwd_arg}{z} a -tzip {} {srcs}", q(&output))
            } else if win {
                // Windows 本地无 zip CLI 时用 PowerShell 内置 Compress-Archive 兜底
                let ps_srcs = names.iter().map(|n| format!("'{}'", n.replace('\'', "''"))).collect::<Vec<_>>().join(",");
                format!("{cwd_arg}powershell -NoProfile -Command \"Compress-Archive -Path {ps_srcs} -DestinationPath '{}' -Force\"", output.replace('\'', "''"))
            } else {
                return Err(AppError::Ssh("目标机器缺少 zip/7z 工具，无法创建 zip（可改装 7zip 或改用 tar.gz）".into()));
            }
        }
        "7z" => {
            if let Some(z) = tool_cmd(&tools, "7z").or_else(|| tool_cmd(&tools, "7za")) {
                format!("{cwd_arg}{z} a {} {srcs}", q(&output))
            } else {
                return Err(AppError::Ssh("目标机器未安装 7z/7za，无法创建 7z（apt install p7zip-full 或改用 tar.gz）".into()));
            }
        }
        "rar" => {
            if let Some(rar) = tool_cmd(&tools, "rar") {
                format!("{cwd_arg}{rar} a -r {} {srcs}", q(&output))
            } else {
                return Err(AppError::Ssh("目标机器未安装 rar，无法创建 rar 压缩包（rar 为商业软件，仅支持解压；可改用 zip/7z）".into()));
            }
        }
        _ => return Err(AppError::Validation(format!("不支持的压缩格式: {format}"))),
    };
    ssh.run_shell_checked(&cmd).await?;
    tracing::info!(%output, %format, "压缩完成");
    Ok(())
}

/// 解压：把 archive 解到 dest_dir（需已存在；前端负责选目录/建目录）
/// password：加密压缩包（zip/7z/rar）的解压密码，None 表示无密码
#[tauri::command]
pub async fn archive_extract(
    session_id: String,
    archive: String,
    dest_dir: String,
    password: Option<String>,
    manager: State<'_, TerminalSessionManager>,
) -> AppResult<()> {
    let ssh = {
        let lock = manager.sessions.lock().unwrap();
        lock.get(&session_id).and_then(|e| e.ssh.clone())
    };
    let ssh = ssh.ok_or_else(|| AppError::Ssh("会话未连接".into()))?;
    let win = ssh.is_local() && cfg!(windows);
    let q: fn(&str) -> String = if win { win_q } else { sh_q };
    let tools = detect_archive_tools(session_id.clone(), manager).await?;
    let lower = archive.to_ascii_lowercase();
    let a = q(&archive);
    let d = q(&dest_dir);
    // 密码参数：unzip 用 -P <pwd>（可引用），7z/unrar 用 -p<pwd>（不可有空格）
    let pw = password.unwrap_or_default();
    let unzip_pw = if pw.is_empty() { String::new() } else { format!("-P {} ", q(&pw)) };
    let z7_pw = if pw.is_empty() { String::new() } else { format!("-p{}", pw.replace([' ', '\t'], "")) };
    let rar_pw = if pw.is_empty() { String::new() } else { format!("-p{} ", sh_q(&pw)) };

    let cmd = if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        format!("tar xzf {a} -C {d}")
    } else if lower.ends_with(".tar.bz2") || lower.ends_with(".tbz2") {
        format!("tar xjf {a} -C {d}")
    } else if lower.ends_with(".tar.xz") || lower.ends_with(".txz") {
        format!("tar xJf {a} -C {d}")
    } else if lower.ends_with(".tar.zst") || lower.ends_with(".tzst") {
        if win {
            format!("tar --zstd -xf {a} -C {d}")
        } else if let Some(zstd) = tool_cmd(&tools, "zstd") {
            format!("{zstd} -dc {a} | tar xf - -C {d}")
        } else {
            return Err(AppError::Ssh("目标机器缺少 zstd 工具，无法解压 tar.zst".into()));
        }
    } else if lower.ends_with(".tar") {
        format!("tar xf {a} -C {d}")
    } else if lower.ends_with(".zip")
        || lower.ends_with(".jar")
        || lower.ends_with(".war")
        || lower.ends_with(".ear")
        || lower.ends_with(".apk")
    {
        // jar/war/ear/apk 均为 ZIP 格式
        if win {
            // Windows 本地：优先 7z；否则用 Windows 10+ 内置 bsdtar（System32\tar.exe，
            // 基于 libarchive，原生支持 zip 系格式），避免未装 unzip/7z 时报"不支持"
            if let Some(z) = tool_cmd(&tools, "7z").or_else(|| tool_cmd(&tools, "7za")) {
                format!("{z} x {z7_pw} {a} -o{d} -y")
            } else {
                format!("tar -xf {a} -C {d}")
            }
        } else if let Some(unzip) = tool_cmd(&tools, "unzip") {
            format!("{unzip} -o -q {unzip_pw}{a} -d {d}")
        } else if let Some(z) = tool_cmd(&tools, "7z").or_else(|| tool_cmd(&tools, "7za")) {
            format!("{z} x {z7_pw} {a} -o{d} -y")
        } else {
            return Err(AppError::Ssh("目标机器缺少 unzip/7z 工具，无法解压 zip/jar/war/ear/apk".into()));
        }
    } else if lower.ends_with(".7z") {
        if let Some(z) = tool_cmd(&tools, "7z").or_else(|| tool_cmd(&tools, "7za")) {
            format!("{z} x {z7_pw} {a} -o{d} -y")
        } else {
            return Err(AppError::Ssh("目标机器未安装 7z/7za，无法解压 7z".into()));
        }
    } else if lower.ends_with(".rar") {
        if let Some(unrar) = tool_cmd(&tools, "unrar") {
            format!("{unrar} x -y {rar_pw}{a} {d}/")
        } else if let Some(z) = tool_cmd(&tools, "7z").or_else(|| tool_cmd(&tools, "7za")) {
            format!("{z} x {z7_pw} {a} -o{d} -y")
        } else {
            return Err(AppError::Ssh("目标机器缺少 unrar/7z 工具，无法解压 rar".into()));
        }
    } else if lower.ends_with(".gz") {
        if win {
            return Err(AppError::Ssh("本地 Windows 不支持单文件 gz 解压，请用 7z 打开".into()));
        }
        let stem = archive.trim_end_matches(".gz");
        format!("gzip -dc {a} > {}", q(stem))
    } else if lower.ends_with(".xz") {
        if win {
            return Err(AppError::Ssh("本地 Windows 不支持单文件 xz 解压，请用 7z 打开".into()));
        }
        let stem = archive.trim_end_matches(".xz");
        format!("xz -dc {a} > {}", q(stem))
    } else if lower.ends_with(".bz2") {
        if win {
            return Err(AppError::Ssh("本地 Windows 不支持单文件 bz2 解压，请用 7z 打开".into()));
        }
        let stem = archive.trim_end_matches(".bz2");
        format!("bzip2 -dc {a} > {}", q(stem))
    } else if lower.ends_with(".zst") {
        if win {
            return Err(AppError::Ssh("本地 Windows 不支持单文件 zst 解压，请用 7z 打开".into()));
        }
        if tool_cmd(&tools, "zstd").is_none() {
            return Err(AppError::Ssh("目标机器缺少 zstd 工具，无法解压 .zst".into()));
        }
        let stem = archive.trim_end_matches(".zst");
        format!("zstd -dc {a} > {}", q(stem))
    } else {
        return Err(AppError::Validation("无法识别的压缩格式（支持 tar/tar.gz/tar.bz2/tar.xz/tar.zst/zip/jar/war/ear/apk/7z/rar/gz/xz/bz2/zst）".into()));
    };
    ssh.run_shell_checked(&cmd).await?;
    tracing::info!(%archive, %dest_dir, "解压完成");
    Ok(())
}

/// 在会话内创建目录（递归创建；本地/SSH 均可；解压到 xxx 文件夹用）
#[tauri::command]
pub async fn make_remote_dir(
    session_id: String,
    path: String,
    manager: State<'_, TerminalSessionManager>,
) -> AppResult<()> {
    let ssh = {
        let lock = manager.sessions.lock().unwrap();
        lock.get(&session_id).and_then(|e| e.ssh.clone())
    };
    let ssh = ssh.ok_or_else(|| AppError::Ssh("会话未连接".into()))?;
    if ssh.is_local() {
        std::fs::create_dir_all(&path)
            .map_err(|e| AppError::Io(std::io::Error::new(e.kind(), format!("创建目录失败 {path}: {e}"))).into())
    } else {
        ssh.run_shell_checked(&format!("mkdir -p {}", sh_q(&path))).await.map(|_| ())
    }
}

/// 在会话内创建空文件（本地/SSH 均可；文件已存在则报错，不覆盖）
#[tauri::command]
pub async fn create_remote_file(
    session_id: String,
    path: String,
    manager: State<'_, TerminalSessionManager>,
) -> AppResult<()> {
    let ssh = {
        let lock = manager.sessions.lock().unwrap();
        lock.get(&session_id).and_then(|e| e.ssh.clone())
    };
    let ssh = ssh.ok_or_else(|| AppError::Ssh("会话未连接".into()))?;
    if ssh.is_local() {
        // create_new：文件已存在则报错，避免覆盖
        if std::fs::metadata(&path).is_ok() {
            return Err(AppError::Validation(format!("文件已存在: {path}")));
        }
        std::fs::File::create(&path)
            .map_err(|e| AppError::Io(std::io::Error::new(e.kind(), format!("创建文件失败 {path}: {e}"))))?;
        Ok(())
    } else {
        // SSH：先判存在性，避免 touch 仅更新时间戳的副作用
        let cmd = format!("[ ! -e {} ] && touch {}", sh_q(&path), sh_q(&path));
        ssh.run_shell_checked(&cmd).await.map(|_| ())
    }
}

/// 修改会话内文件/目录权限（权限编辑对话框用；mode 为 3~4 位八进制如 "755"/"0644"）
#[tauri::command]
pub async fn set_file_permissions(
    session_id: String,
    path: String,
    mode: String,
    recursive: Option<bool>,
    manager: State<'_, TerminalSessionManager>,
) -> AppResult<()> {
    let m = mode.trim().trim_start_matches('0');
    if m.is_empty() || m.len() > 4 || !m.chars().all(|c| ('0'..='7').contains(&c)) {
        return Err(AppError::Validation(format!("无效的权限模式: {mode}（应为 3~4 位八进制，如 755）")));
    }
    let ssh = {
        let lock = manager.sessions.lock().unwrap();
        lock.get(&session_id).and_then(|e| e.ssh.clone())
    };
    let ssh = ssh.ok_or_else(|| AppError::Ssh("会话未连接".into()))?;
    let rec = recursive.unwrap_or(false);
    if ssh.is_local() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let bits = u32::from_str_radix(m, 8)
                .map_err(|_| AppError::Validation(format!("无效的权限模式: {mode}")))?;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(bits))
                .map_err(|e| AppError::Io(std::io::Error::new(e.kind(), format!("修改权限失败 {path}: {e}"))))?;
            if rec {
                // 本地递归走 shell（Windows 无 chmod，下方 cfg 分支已拦截）
                ssh.run_shell_checked(&format!("chmod -R {} {}", m, sh_q(&path))).await?;
            }
            Ok(())
        }
        #[cfg(not(unix))]
        {
            let _ = rec;
            Err(AppError::Validation("本地 Windows 不支持 chmod 权限编辑".into()))
        }
    } else {
        let flag = if rec { "-R " } else { "" };
        ssh.run_shell_checked(&format!("chmod {flag}{} {}", m, sh_q(&path))).await.map(|_| ())
    }
}

// ===== 本机系统信息（本地终端的设备信息面板用，跨平台 sysinfo，不依赖 SSH） =====
//
// 采样全部交给 services::local_metrics 的后台线程，这里只做内存快照读取。
// 硬性约束：这些 command 必须「零阻塞」—— 同步 command 跑在 Tauri 的 UI 主线程上，
// 同一进程所有窗口共用这条线程，一旦在里面 sleep / 读盘 / 枚举磁盘，
// 整个应用（主窗口 + 全部独立窗口）都会一起卡住。

/// 系统基本信息
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalSystemInfo {
    pub hostname: String,
    pub os: String,
    pub kernel: String,
    pub uptime: String,
    pub cpu_model: String,
    pub cpu_cores: usize,
    pub ip: String,
}

#[tauri::command]
pub fn get_local_system_info() -> AppResult<LocalSystemInfo> {
    #[cfg(desktop)]
    {
        let s = crate::services::local_metrics::snapshot();
        Ok(LocalSystemInfo {
            hostname: s.sys.hostname,
            os: s.sys.os,
            kernel: s.sys.kernel,
            uptime: s.sys.uptime,
            cpu_model: s.sys.cpu_model,
            cpu_cores: s.sys.cpu_cores,
            ip: s.sys.ip,
        })
    }
    #[cfg(mobile)]
    {
        Err(AppError::Internal("移动端不支持本机系统信息".into()))
    }
}

/// CPU 使用率（取后台 1s 采样快照，无需在此等待采样窗口）
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalCpuInfo {
    pub usage: f64,
    pub per_core: Vec<f64>,
    pub load_avg: [f64; 3],
}

#[tauri::command]
pub fn get_local_cpu_info() -> AppResult<LocalCpuInfo> {
    #[cfg(desktop)]
    {
        let s = crate::services::local_metrics::snapshot();
        Ok(LocalCpuInfo {
            usage: s.cpu_usage,
            per_core: s.per_core,
            load_avg: s.load_avg,
        })
    }
    #[cfg(mobile)]
    {
        Err(AppError::Internal("移动端不支持本机系统信息".into()))
    }
}

/// 内存（字节）
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalMemInfo {
    pub total: u64,
    pub used: u64,
    pub available: u64,
    pub cached: u64,
    pub swap_total: u64,
    pub swap_used: u64,
}

#[tauri::command]
pub fn get_local_mem_info() -> AppResult<LocalMemInfo> {
    #[cfg(desktop)]
    {
        let s = crate::services::local_metrics::snapshot();
        Ok(LocalMemInfo {
            total: s.mem_total,
            used: s.mem_used,
            available: s.mem_available,
            cached: 0,
            swap_total: s.swap_total,
            swap_used: s.swap_used,
        })
    }
    #[cfg(mobile)]
    {
        Err(AppError::Internal("移动端不支持本机系统信息".into()))
    }
}

/// 磁盘分区
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalDiskPartition {
    pub mount: String,
    pub total: u64,
    pub used: u64,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalDiskInfo {
    pub total: u64,
    pub used: u64,
    pub partitions: Vec<LocalDiskPartition>,
}

#[tauri::command]
pub fn get_local_disk_info() -> AppResult<LocalDiskInfo> {
    #[cfg(desktop)]
    {
        let s = crate::services::local_metrics::snapshot();
        Ok(LocalDiskInfo {
            total: s.disk_total,
            used: s.disk_used,
            partitions: s
                .disks
                .into_iter()
                .map(|d| LocalDiskPartition {
                    mount: d.mount,
                    total: d.total,
                    used: d.used,
                })
                .collect(),
        })
    }
    #[cfg(mobile)]
    {
        Err(AppError::Internal("移动端不支持本机系统信息".into()))
    }
}

/// 网络速率（字节/秒，取后台采样的相邻两次差值）
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalNetInfo {
    pub rx_rate: u64,
    pub tx_rate: u64,
}

#[tauri::command]
pub fn get_local_net_rate() -> AppResult<LocalNetInfo> {
    #[cfg(desktop)]
    {
        let s = crate::services::local_metrics::snapshot();
        Ok(LocalNetInfo {
            rx_rate: s.net_rx_rate,
            tx_rate: s.net_tx_rate,
        })
    }
    #[cfg(mobile)]
    {
        Err(AppError::Internal("移动端不支持本机系统信息".into()))
    }
}
