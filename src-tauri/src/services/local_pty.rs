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

//! 本地 PTY 会话 —— 在本地计算机启动真实 Shell（Windows: PowerShell；Unix: bash）
//! 作者：JC
//! 输出复用 ssh-output / ssh-status 事件，前端无需区分本地/远程会话。
#![cfg(desktop)]

use std::io::{Read, Write};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::sync::Mutex;

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use tauri::{AppHandle, Emitter};

use crate::common::{AppError, AppResult};
use crate::services::ssh::SshSessionHandle;

/// SSH 输出事件载荷（与 russh_impl 保持一致）
#[derive(serde::Serialize, Clone)]
struct LocalOutputPayload {
    session_id: String,
    data: String,
}

/// SSH 状态事件载荷（与 russh_impl 保持一致）
#[derive(serde::Serialize, Clone)]
struct LocalStatusPayload {
    session_id: String,
    event: String,
    message: Option<String>,
}

/// 本地 PTY 会话句柄
pub struct LocalPtyHandle {
    writer: Mutex<Box<dyn Write + Send>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
    /// Shell 子进程 PID（目录跟踪：读取进程 cwd 实现终端→文件系统联动）
    pid: u32,
}

#[async_trait::async_trait]
impl SshSessionHandle for LocalPtyHandle {
    async fn write(&self, data: &[u8]) -> AppResult<()> {
        let mut writer = self.writer.lock().unwrap();
        writer
            .write_all(data)
            .and_then(|_| writer.flush())
            .map_err(|e| AppError::Ssh(format!("写入本地终端失败: {e}")))
    }

    async fn resize(&self, cols: u16, rows: u16) -> AppResult<()> {
        let master = self.master.lock().unwrap();
        master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| AppError::Ssh(format!("调整本地终端大小失败: {e}")))
    }

    async fn close(&self) -> AppResult<()> {
        let mut child = self.child.lock().unwrap();
        let _ = child.kill();
        Ok(())
    }

    async fn exec_command(&self, _command: &str) -> AppResult<String> {
        Err(AppError::Ssh("本地终端不支持远程命令".into()))
    }

    async fn shell_cwd(&self) -> AppResult<String> {
        if self.pid == 0 {
            return Err(AppError::Ssh("本地 Shell PID 未知".into()));
        }
        // 通过 sysinfo 读取 Shell 进程 cwd（PEB CurrentDirectory）；
        // cmd / Git Bash 的 cd 会直接改进程 cwd，PowerShell 不会 —— 依赖 create_local_shell
        // 注入的 prompt 包装在每次出提示符前把进程 cwd 同步为当前 PS Location；
        // 注意必须显式请求 cwd 字段，默认刷新不采集 cwd
        let pid = sysinfo::Pid::from_u32(self.pid);
        let mut sys = sysinfo::System::new();
        sys.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::Some(&[pid]),
            sysinfo::ProcessRefreshKind::new().with_cwd(sysinfo::UpdateKind::Always),
        );
        let process = sys
            .process(pid)
            .ok_or_else(|| AppError::Ssh("本地 Shell 进程已退出".into()))?;
        let cwd = process
            .cwd()
            .ok_or_else(|| AppError::Ssh("无法读取本地 Shell 当前目录".into()))?;
        let s = cwd.to_string_lossy().to_string();
        if s.is_empty() {
            return Err(AppError::Ssh("本地 Shell 当前目录为空".into()));
        }
        Ok(s)
    }

    async fn upload_file(&self, _app: &AppHandle, _transfer_id: &str, _local_path: &str, _remote_path: &str, _cancel: Arc<AtomicBool>) -> AppResult<()> {
        Err(AppError::Ssh("本地终端不支持文件上传".into()))
    }

    async fn download_file(&self, _app: &AppHandle, _transfer_id: &str, _remote_path: &str, _local_path: &str, _cancel: Arc<AtomicBool>) -> AppResult<()> {
        Err(AppError::Ssh("本地终端不支持文件下载".into()))
    }

    async fn upload_path(&self, _app: &AppHandle, _transfer_id: &str, _local_path: &str, _remote_path: &str, _cancel: Arc<AtomicBool>) -> AppResult<()> {
        Err(AppError::Ssh("本地终端不支持文件上传".into()))
    }

    async fn download_path(&self, _app: &AppHandle, _transfer_id: &str, _remote_path: &str, _local_path: &str, _cancel: Arc<AtomicBool>) -> AppResult<()> {
        Err(AppError::Ssh("本地终端不支持文件下载".into()))
    }

    async fn list_dir(&self, _path: &str) -> AppResult<Vec<crate::services::ssh::transfer::RemoteDirEntry>> {
        // 本地会话的文件列表走 `list_local_dir` 命令（前端 fetchChildrenLocal），不经过这里
        Err(AppError::Ssh("本地终端不支持远程目录列出".into()))
    }

    async fn read_file_bytes(&self, path: &str, max_bytes: u64) -> AppResult<Vec<u8>> {
        let meta = std::fs::metadata(path)
            .map_err(|e| AppError::Io(std::io::Error::new(e.kind(), format!("读取文件失败 {path}: {e}"))))?;
        if meta.len() > max_bytes {
            return Err(AppError::Validation(format!("文件过大（{} 字节），超出编辑器上限", meta.len())));
        }
        std::fs::read(path)
            .map_err(|e| AppError::Io(std::io::Error::new(e.kind(), format!("读取文件失败 {path}: {e}"))).into())
    }

    async fn write_file_text(&self, path: &str, content: &str, encoding: &str) -> AppResult<()> {
        let bytes = crate::services::ssh::transfer::encode_text(content, encoding);
        std::fs::write(path, bytes)
            .map_err(|e| AppError::Io(std::io::Error::new(e.kind(), format!("写入文件失败 {path}: {e}"))).into())
    }

    async fn run_shell_checked(&self, command: &str) -> AppResult<String> {
        // 本地会话：Windows 走 cmd /C，Unix 走 sh -c（压缩/解压等命令按平台拼）
        let output = run_local_shell(command).await?;
        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr);
            let detail = if detail.trim().is_empty() { String::from_utf8_lossy(&output.stdout).to_string() } else { detail.trim().to_string() };
            let code = output.status.code().unwrap_or(-1);
            let detail = if detail.is_empty() { format!("exit {code}") } else { detail };
            return Err(AppError::Ssh(detail));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn is_local(&self) -> bool { true }
}

/// 在本机执行 shell 命令（不检查退出码，由调用方判定）
async fn run_local_shell(command: &str) -> AppResult<std::process::Output> {
    let output = if cfg!(windows) {
        tokio::process::Command::new("cmd").args(["/C", command]).output().await
    } else {
        tokio::process::Command::new("sh").args(["-c", command]).output().await
    };
    output.map_err(|e| AppError::Ssh(format!("执行本地命令失败: {e}")).into())
}

/// PowerShell 的 `Set-Location`(cd) 只改会话位置，不会改 Win32 进程工作目录
/// （PEB CurrentDirectory 始终停在启动目录），导致 `shell_cwd` 读到的目录永不变化，
/// 终端→文件系统联动与 Agent 模式 prompt 全部失效（cmd / Git Bash 无此问题）。
/// 参照 Windows Terminal 的 shell integration：以 `-NoExit -Command` 注入一段启动脚本，
/// 包装全局 prompt 函数，在每次出提示符前把进程 cwd 同步为当前 PS Location；
/// 原始 prompt（含用户 profile 自定义的）先捕获再转调，提示符外观不变。
/// 脚本内只用单引号，避开 Win32 命令行参数转义。
#[cfg(windows)]
const PS_SYNC_CWD_SCRIPT: &str = concat!(
    "$__jcPrevPrompt = if (Get-Command prompt -CommandType Function -ErrorAction SilentlyContinue) ",
    "{ (Get-Command prompt -CommandType Function).ScriptBlock }; ",
    "function global:prompt { ",
    "try { [System.IO.Directory]::SetCurrentDirectory((Get-Location -PSProvider FileSystem).ProviderPath) } ",
    "catch {}; ",
    "if ($__jcPrevPrompt) { & $__jcPrevPrompt } else { 'PS ' + (Get-Location).Path + '> ' } }",
);

/// PowerShell 启动参数（Windows PowerShell / pwsh 通用）
#[cfg(windows)]
fn powershell_args() -> Vec<String> {
    vec![
        "-NoExit".to_string(),
        "-Command".to_string(),
        PS_SYNC_CWD_SCRIPT.to_string(),
    ]
}

/// 根据设置项解析本地 Shell 程序与参数（auto：Windows→PowerShell，Unix→$SHELL）
fn resolve_shell(name: Option<&str>) -> (String, Vec<String>) {
    let name = name.unwrap_or("auto");
    #[cfg(windows)]
    {
        match name {
            "cmd" => ("cmd.exe".to_string(), vec![]),
            "bash" => {
                // 常见 Git for Windows 安装路径；未找到时回退 PowerShell
                let mut candidates = Vec::new();
                if let Ok(pf) = std::env::var("ProgramFiles") {
                    candidates.push(format!("{pf}\\Git\\bin\\bash.exe"));
                }
                if let Ok(pf86) = std::env::var("ProgramFiles(x86)") {
                    candidates.push(format!("{pf86}\\Git\\bin\\bash.exe"));
                }
                for c in &candidates {
                    if std::path::Path::new(c).exists() {
                        return (c.clone(), vec!["-l".to_string()]);
                    }
                }
                tracing::warn!("未找到 Git Bash，回退 PowerShell");
                ("powershell.exe".to_string(), powershell_args())
            }
            "zsh" => ("wsl.exe".to_string(), vec!["zsh".to_string(), "-l".to_string()]),
            // auto / powershell / 未知值：默认 PowerShell
            _ => ("powershell.exe".to_string(), powershell_args()),
        }
    }
    #[cfg(not(windows))]
    {
        match name {
            "zsh" => ("/bin/zsh".to_string(), vec![]),
            "bash" => ("/bin/bash".to_string(), vec![]),
            // auto：用户默认 Shell；cmd/powershell 在 Unix 上无意义，同样回退默认
            _ => (
                std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string()),
                vec![],
            ),
        }
    }
}

/// 启动本地 Shell 会话，返回句柄（shell 由设置项指定）
pub fn create_local_shell(
    app: &AppHandle,
    session_id: &str,
    cols: u16,
    rows: u16,
    shell: Option<&str>,
) -> AppResult<Box<dyn SshSessionHandle>> {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| AppError::Ssh(format!("创建本地 PTY 失败: {e}")))?;

    // 选择本地 shell（按设置项；auto：Windows → PowerShell，Unix → $SHELL 或 bash）
    let (shell, shell_args) = resolve_shell(shell);

    let mut cmd = CommandBuilder::new(&shell);
    for a in &shell_args {
        cmd.arg(a);
    }
    // 进入本地计算机家目录
    if let Some(home) = dirs::home_dir() {
        cmd.cwd(home);
    }
    cmd.env("TERM", "xterm-256color");

    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| AppError::Ssh(format!("启动本地 Shell 失败: {e}")))?;
    drop(pair.slave);
    let pid = child.process_id().unwrap_or(0);

    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| AppError::Ssh(format!("克隆 PTY 读取端失败: {e}")))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|e| AppError::Ssh(format!("获取 PTY 写入端失败: {e}")))?;

    // 读取线程：PTY 输出 → 前端
    let reader_app = app.clone();
    let reader_sid = session_id.to_string();
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let text = String::from_utf8_lossy(&buf[..n]).to_string();
                    let _ = reader_app.emit("ssh-output", LocalOutputPayload {
                        session_id: reader_sid.clone(),
                        data: text,
                    });
                }
                Err(_) => break,
            }
        }
        let _ = reader_app.emit("ssh-status", LocalStatusPayload {
            session_id: reader_sid,
            event: "disconnected".into(),
            message: Some("本地 Shell 已退出".into()),
        });
    });

    tracing::info!(%session_id, %shell, "本地 Shell 会话已启动");
    Ok(Box::new(LocalPtyHandle {
        writer: Mutex::new(writer),
        master: Mutex::new(pair.master),
        child: Mutex::new(child),
        pid,
    }))
}
