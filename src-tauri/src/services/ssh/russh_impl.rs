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

//! russh SSH 实现 —— 真实 SSH 连接、PTY、I/O
//! 作者：JC

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use russh::client;
use russh::keys::ssh_key::PublicKey;
use russh::ChannelMsg;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

use crate::common::{AppError, AppResult};
use crate::services::ssh::SshSessionHandle;
use super::transfer;

/// SSH 输出事件
#[derive(serde::Serialize, Clone)]
struct SshOutputPayload {
    session_id: String,
    data: String,
}

/// SSH 状态事件?
#[derive(serde::Serialize, Clone)]
struct SshStatusPayload {
    session_id: String,
    event: String,
    message: Option<String>,
}

/// russh 客户端 Handler
struct ClientHandler;

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _key: &PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

/// 保存 SSH 连接参数用于 exec 命令（传输模块共用）
#[derive(Clone)]
pub(crate) struct SshConfig {
    host: String,
    port: u16,
    username: String,
    auth_type: String,
    password: Option<String>,
    private_key_path: Option<String>,
    passphrase: Option<String>,
    proxy: Option<ProxyInfo>,
}

// ===== 代理隧道（SOCKS5 / HTTP CONNECT，由「通用-网络代理」设置驱动） =====

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum ProxyKind {
    Socks5,
    Http,
}

/// 代理参数（前端解析设置后传入；空 = 直连）?
#[derive(Clone)]
pub(crate) struct ProxyInfo {
    kind: ProxyKind,
    /// 代理监听地址 host:port
    addr: String,
}

pub(crate) fn parse_proxy(addr: Option<&str>, kind: Option<&str>) -> Option<ProxyInfo> {
    let addr = addr?.trim();
    if addr.is_empty() {
        return None;
    }
    let is_socks5 = kind.map(|k| k.eq_ignore_ascii_case("socks5")).unwrap_or(false);
    Some(ProxyInfo {
        kind: if is_socks5 { ProxyKind::Socks5 } else { ProxyKind::Http },
        addr: addr.to_string(),
    })
}

/// SOCKS5 无认证握手，成功后返回已建立隧道的流（目标为域名直传，由代理侧解析）
async fn socks5_tunnel(
    mut stream: tokio::net::TcpStream,
    host: &str,
    port: u16,
) -> AppResult<tokio::net::TcpStream> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    stream
        .write_all(&[0x05, 0x01, 0x00])
        .await
        .map_err(|e| AppError::Ssh(format!("SOCKS5 握手失败: {e}")))?;
    let mut resp = [0u8; 2];
    stream
        .read_exact(&mut resp)
        .await
        .map_err(|e| AppError::Ssh(format!("SOCKS5 握手失败: {e}")))?;
    if resp[0] != 0x05 || resp[1] != 0x00 {
        return Err(AppError::Ssh("SOCKS5 代理拒绝无认证方式".into()));
    }
    // CONNECT 请求：ATYP=0x03（域名）
    let host_bytes = host.as_bytes();
    let mut req = vec![0x05, 0x01, 0x00, 0x03, host_bytes.len() as u8];
    req.extend_from_slice(host_bytes);
    req.extend_from_slice(&port.to_be_bytes());
    stream
        .write_all(&req)
        .await
        .map_err(|e| AppError::Ssh(format!("SOCKS5 连接请求发送败: {e}")))?;
    let mut hdr = [0u8; 4];
    stream
        .read_exact(&mut hdr)
        .await
        .map_err(|e| AppError::Ssh(format!("SOCKS5 应答读取失败: {e}")))?;
    if hdr[1] != 0x00 {
        return Err(AppError::Ssh(format!("SOCKS5 隧道建立失败（应答码 {}）", hdr[1])));
    }
    // 跳过 BND.ADDR/BND.PORT 段?
    let skip: AppResult<()> = async {
        match hdr[3] {
            0x01 => {
                let mut b = [0u8; 6];
                stream.read_exact(&mut b).await?;
            }
            0x04 => {
                let mut b = [0u8; 18];
                stream.read_exact(&mut b).await?;
            }
            0x03 => {
                let mut lb = [0u8; 1];
                stream.read_exact(&mut lb).await?;
                let mut rest = vec![0u8; lb[0] as usize + 2];
                stream.read_exact(&mut rest).await?;
            }
            _ => return Err(AppError::Ssh("SOCKS5 应答地址类型非法".into())),
        }
        Ok(())
    }
    .await;
    skip.map_err(|e| AppError::Ssh(format!("SOCKS5 应答读取失败: {e}")))?;
    Ok(stream)
}

/// HTTP CONNECT 隧道；逐字节读到响应头结束，避免误吞后续 SSH 协议字节
async fn http_tunnel(
    mut stream: tokio::net::TcpStream,
    host: &str,
    port: u16,
) -> AppResult<tokio::net::TcpStream> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let req = format!("CONNECT {host}:{port} HTTP/1.1\r\nHost: {host}:{port}\r\n\r\n");
    stream
        .write_all(req.as_bytes())
        .await
        .map_err(|e| AppError::Ssh(format!("HTTP 代理请求发送败: {e}")))?;
    let mut buf: Vec<u8> = Vec::with_capacity(256);
    let mut b = [0u8; 1];
    loop {
        let n = stream
            .read(&mut b)
            .await
            .map_err(|e| AppError::Ssh(format!("HTTP 代理应答读取失败: {e}")))?;
        if n == 0 {
            return Err(AppError::Ssh("HTTP 代理已断开连接".into()));
        }
        buf.push(b[0]);
        if buf.len() >= 4 && buf.ends_with(b"\r\n\r\n") {
            break;
        }
        if buf.len() > 8192 {
            return Err(AppError::Ssh("HTTP 代理响应头过大".into()));
        }
    }
    let head = String::from_utf8_lossy(&buf);
    let status_line = head.lines().next().unwrap_or("");
    if !status_line.contains(" 200") {
        return Err(AppError::Ssh(format!("HTTP 代理隧道建立失败: {status_line}")));
    }
    Ok(stream)
}

/// 建立 russh 客户端连接（直连或经 SOCKS5/HTTP 代理；传输模块共用）
pub(crate) async fn establish_client(
    config: Arc<client::Config>,
    host: &str,
    port: u16,
    proxy: Option<&ProxyInfo>,
    timeout_ms: u64,
) -> AppResult<client::Handle<ClientHandler>> {
    establish_client_h(config, host, port, proxy, timeout_ms, || ClientHandler).await
}

/// [`establish_client`] 的泛型版本：允许调用方自带 Handler
/// （端口转发的 `ForwardHandler` 需要 `server_channel_open_forwarded_tcpip` 回调，
/// 代理/直连/超时逻辑必须与终端完全一致，否则「终端能连、隧道连不上」）。
pub(crate) async fn establish_client_h<H, F>(
    config: Arc<client::Config>,
    host: &str,
    port: u16,
    proxy: Option<&ProxyInfo>,
    timeout_ms: u64,
    make_handler: F,
) -> AppResult<client::Handle<H>>
where
    H: client::Handler<Error = russh::Error> + Send + 'static,
    F: FnOnce() -> H,
{
    let addr = format!("{host}:{port}");
    let fut = async {
        match proxy {
            None => {
                let sock_addr = tokio::net::lookup_host(&addr)
                    .await
                    .map_err(|e| AppError::Ssh(format!("DNS 解析失败 {addr}: {e}")))?
                    .next()
                    .ok_or_else(|| AppError::Ssh(format!("无法解析地址 {addr}")))?;
                client::connect(config, sock_addr, make_handler())
                    .await
                    .map_err(|e| AppError::Ssh(format!("连接失败: {addr}: {e}")))
            }
            Some(p) => {
                tracing::info!(proxy = %p.addr, "经代理连接 SSH");
                let stream = tokio::net::TcpStream::connect(&p.addr)
                    .await
                    .map_err(|e| AppError::Ssh(format!("连接代理 {} 失败: {e}", p.addr)))?;
                let stream = match p.kind {
                    ProxyKind::Socks5 => socks5_tunnel(stream, host, port).await?,
                    ProxyKind::Http => http_tunnel(stream, host, port).await?,
                };
                client::connect_stream(config, stream, make_handler())
                    .await
                    .map_err(|e| AppError::Ssh(format!("连接失败（经代理）: {addr}: {e}")))
            }
        }
    };
    tokio::time::timeout(Duration::from_millis(timeout_ms), fut)
        .await
        .map_err(|_| AppError::Ssh(format!("连接超时: {addr}")))?
}

/// russh 会话句柄
pub struct RusshSessionHandle {
    config: SshConfig,
    /// SFTP 会话：**直接复用主 SSH 连接**（connect_ssh 在把 session 交给驱动任务前先开好子系统），
    /// 因此列目录/读文件无需重新拨号 + 重认证（原来每次都会 open_sftp_session 新建连接，很慢）。
    /// 元组第二项为「独立驱动句柄」：复用主连接的 SFTP 为 None（由主连接驱动任务复用），
    /// 仅当主连接未提供 SFTP、退回 open_sftp_session 重拨时才有 Some（需自行 abort）。
    sftp: tokio::sync::Mutex<Option<(russh_sftp::client::SftpSession, Option<tokio::task::JoinHandle<()>>)>>,
    input_tx: mpsc::UnboundedSender<Vec<u8>>,
    close_tx: mpsc::UnboundedSender<()>,
    /// 窗口尺寸变更通知（I/O 循环据此向远端发送 window-change 请求）。
    /// 必须真正下发，否则远端 readline 按旧尺寸(默认 80 列)建模、本地按实际列宽换行，
    /// 长命令翻历史时重绘会错位（提示符丢失/光标跳行）。
    resize_tx: mpsc::UnboundedSender<(u16, u16)>,
    /// 交互式 Shell 的远程 PID（0 = 未知）
    shell_pid: Arc<AtomicU64>,
}

#[async_trait::async_trait]
impl SshSessionHandle for RusshSessionHandle {
    async fn write(&self, data: &[u8]) -> AppResult<()> {
        self.input_tx
            .send(data.to_vec())
            .map_err(|e| AppError::Ssh(format!("发送数据败: {e}")))
    }

    async fn resize(&self, cols: u16, rows: u16) -> AppResult<()> {
        self.resize_tx
            .send((cols, rows))
            .map_err(|e| AppError::Ssh(format!("发送窗口尺寸失败: {e}")))
    }

    async fn close(&self) -> AppResult<()> {
        let _ = self.close_tx.send(());
        // 释放缓存的 SFTP 会话；若为「重拨」建立的独立连接，一并终止其驱动任务避免泄漏
        let mut guard = self.sftp.lock().await;
        if let Some((_s, drive)) = guard.take() {
            if let Some(d) = drive {
                d.abort();
            }
        }
        Ok(())
    }

    async fn exec_command(&self, command: &str) -> AppResult<String> {
        exec_remote_command(&self.config, command).await
    }

    async fn list_dir(&self, path: &str) -> AppResult<Vec<crate::services::ssh::transfer::RemoteDirEntry>> {
        // 复用主连接上预先开好的 SFTP 会话（连接时已建立），列目录毫秒级；
        // 仅当主连接未提供 SFTP 时才退回 open_sftp_session 重拨（并缓存）。
        let mut guard = self.sftp.lock().await;
        if guard.is_none() {
            let sftp_opt = match tokio::time::timeout(Duration::from_secs(20), open_sftp_session(&self.config)).await {
                Ok(r) => r?,
                Err(_) => {
                    tracing::warn!(path = %path, "SFTP 会话建立超时(20s)，目录列出回退 exec 通道");
                    return Err(AppError::Ssh("SFTP 会话建立超时".into()));
                }
            };
            let (s, d) = sftp_opt.ok_or_else(|| AppError::Ssh("sftp 子系统不可用".into()))?;
            *guard = Some((s, Some(d)));
        }
        let (sftp, _drive) = guard.as_mut().unwrap();
        // 读取并映射（不在此关闭 SFTP 通道，供后续目录复用）；失败则复位，下次重新建立
        let result = transfer::sftp_read_and_map(sftp, path).await;
        if result.is_err() {
            if let Some((_s, drive)) = guard.take() {
                if let Some(d) = drive {
                    d.abort();
                }
            }
        }
        result
    }

    async fn upload_file(&self, app: &AppHandle, transfer_id: &str, local_path: &str, remote_path: &str, cancel: Arc<AtomicBool>) -> AppResult<()> {
        transfer::upload_file(&self.config, app, transfer_id, local_path, remote_path, cancel).await
    }

    async fn download_file(&self, app: &AppHandle, transfer_id: &str, remote_path: &str, local_path: &str, cancel: Arc<AtomicBool>) -> AppResult<()> {
        transfer::download_file(&self.config, app, transfer_id, remote_path, local_path, cancel).await
    }

    async fn upload_path(&self, app: &AppHandle, transfer_id: &str, local_path: &str, remote_path: &str, cancel: Arc<AtomicBool>) -> AppResult<()> {
        transfer::upload_path(&self.config, app, transfer_id, local_path, remote_path, cancel).await
    }

    async fn download_path(&self, app: &AppHandle, transfer_id: &str, remote_path: &str, local_path: &str, cancel: Arc<AtomicBool>) -> AppResult<()> {
        transfer::download_path(&self.config, app, transfer_id, remote_path, local_path, cancel).await
    }

    async fn read_file_bytes(&self, path: &str, max_bytes: u64) -> AppResult<Vec<u8>> {
        // SFTP 直读；不可用时回退 `base64 <path>` 输出解码（绕过输出大小限制）。
        // open_sftp_session 内部步骤（认证/请求子系统/SftpSession 握手）无外层超时，
        // 服务器不支持 SFTP 或响应缓慢时会整体挂起，表现为前端"一直卡 loading"。
        // 这里给建连加 20s 超时，超时回退 base64 通道（其 exec 自带超时，必然有结果）。
        let sftp_opt = match tokio::time::timeout(
            Duration::from_secs(20),
            open_sftp_session(&self.config),
        )
        .await
        {
            Ok(r) => r?,
            Err(_) => {
                tracing::warn!(path = %path, "SFTP 会话建立超时(20s)，回退 base64 通道");
                None
            }
        };
        if let Some((sftp, drive)) = sftp_opt {
            // SFTP 读取/关闭整体加 20s 外层超时：服务器死锁、网络半开等
            // 场景下避免永久挂起；超时后 drive.abort() 强制中断 russh 子任务。
            let result: AppResult<Vec<u8>> = match tokio::time::timeout(
                Duration::from_secs(20),
                async {
                    if let Ok(m) = sftp.metadata(path).await {
                        if m.len() > max_bytes {
                            return Err(AppError::Validation(format!("文件过大（{} 字节），超出编辑器上限", m.len())));
                        }
                    }
                    sftp.read(path.to_string())
                        .await
                        .map_err(|e| AppError::Ssh(format!("读取远程文件失败 {path}: {e}")))
                },
            )
            .await
            {
                Ok(r) => r,
                Err(_) => Err(AppError::Ssh(format!("SFTP 读取超时(20s): {path}"))),
            };
            // close 也加 2s 兜底，避免通道半死时 close 自身挂起
            let _ = tokio::time::timeout(Duration::from_secs(2), sftp.close()).await;
            drive.abort();
            let data = result?;
            if data.len() as u64 > max_bytes {
                return Err(AppError::Validation(format!("文件过大（{} 字节），超出编辑器上限", data.len())));
            }
            return Ok(data);
        }
        use base64::Engine;
        let out = exec_remote_command(&self.config, &format!("base64 {}", shell_quote(path))).await?;
        let clean: Vec<u8> = out.bytes().filter(|b| !b"\r\n \t".contains(b)).collect();
        let data = base64::engine::general_purpose::STANDARD
            .decode(&clean)
            .map_err(|e| AppError::Ssh(format!("Base64 解码失败: {e}")))?;
        if data.len() as u64 > max_bytes {
            return Err(AppError::Validation(format!("文件过大（{} 字节），超出编辑器上限", data.len())));
        }
        Ok(data)
    }

    async fn write_file_text(&self, path: &str, content: &str, encoding: &str) -> AppResult<()> {
        let bytes = transfer::encode_text(content, encoding);
        if let Some((sftp, drive)) = open_sftp_session(&self.config).await? {
            use tokio::io::AsyncWriteExt;
            let result: AppResult<()> = async {
                let mut f = sftp
                    .create(path)
                    .await
                    .map_err(|e| AppError::Ssh(format!("创建远程文件失败 {path}: {e}")))?;
                f.write_all(&bytes)
                    .await
                    .map_err(|e| AppError::Ssh(format!("写入远程文件失败 {path}: {e}")))?;
                f.flush().await.map_err(|e| AppError::Ssh(format!("远端写入失败: {e}")))
            }
            .await;
            let _ = sftp.close().await;
            drive.abort();
            return result;
        }
        // 回退：`base64 -d > path` exec 通道
        use base64::Engine;
        let cmd = format!("base64 -d > {}", shell_quote(path));
        let (mut channel, drive_handle) = open_exec_stream(&self.config, &cmd).await?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
        channel
            .data(encoded.as_bytes())
            .await
            .map_err(|e| AppError::Ssh(format!("发送数据败: {e}")))?;
        let _ = channel.eof().await;
        while let Some(msg) = channel.wait().await {
            if matches!(msg, ChannelMsg::ExitStatus { .. } | ChannelMsg::Eof | ChannelMsg::Close) {
                break;
            }
        }
        let _ = channel.close().await;
        drive_handle.abort();
        Ok(())
    }

    async fn run_shell_checked(&self, command: &str) -> AppResult<String> {
        exec_remote_checked(&self.config, command).await
    }

    async fn shell_cwd(&self) -> AppResult<String> {
        let pid = self.shell_pid.load(Ordering::Relaxed);
        if pid == 0 {
            return Err(AppError::Ssh("交互式 Shell PID 未知".into()));
        }
        let out = self
            .exec_command(&format!("readlink /proc/{pid}/cwd"))
            .await?;
        let cwd = out.trim().to_string();
        if cwd.is_empty() || cwd.starts_with("readlink") {
            return Err(AppError::Ssh("无法获取 Shell 当前目录".into()));
        }
        Ok(cwd)
    }
}

/// 从输出前缀中提取交互式 Shell 的 PID 标记，返回 (pid, 已消费字节数)
fn extract_shell_pid(buf: &[u8]) -> Option<(u64, usize)> {
    let marker = b"JC_SHELL_PID:";
    let pos = buf.windows(marker.len()).position(|w| w == marker)?;
    let num_start = pos + marker.len();
    let mut num_end = num_start;
    while num_end < buf.len() && buf[num_end].is_ascii_digit() {
        num_end += 1;
    }
    // 数字尚未接收完整（未遇到行尾）时等待更多数据
    if num_end == num_start || num_end >= buf.len() {
        return None;
    }
    let pid: u64 = String::from_utf8_lossy(&buf[num_start..num_end])
        .parse()
        .ok()?;
    // 消费到行尾（兼容 \n 与 \r\n）
    let mut end = num_end;
    if buf[end] == b'\r' {
        end += 1;
    }
    if end < buf.len() && buf[end] == b'\n' {
        end += 1;
    }
    Some((pid, end))
}

/// 打开一个 exec channel（新建立 SSH 连接），返回 channel 与 session 驱动句柄（传输模块共用）
pub(crate) async fn open_exec_stream(
    config: &SshConfig,
    command: &str,
) -> AppResult<(russh::Channel<client::Msg>, tokio::task::JoinHandle<()>)> {
    let ssh_config = Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(300)),
        ..Default::default()
    });

    let mut session = establish_client(ssh_config, &config.host, config.port, config.proxy.as_ref(), 10_000).await?;
    authenticate_session(&mut session, config).await?;

    let channel = session
        .channel_open_session()
        .await
        .map_err(|e| AppError::Ssh(format!("打开通道失败: {e}")))?;

    channel
        .exec(true, command.as_bytes())
        .await
        .map_err(|e| AppError::Ssh(format!("执行命令失败: {e}")))?;

    let drive_handle = tokio::spawn(async move {
        let _ = session.await;
    });

    Ok((channel, drive_handle))
}

/// 会话认证（密码 / 私钥，exec 与 sftp 通道共用）
pub(crate) async fn authenticate_session(
    session: &mut client::Handle<ClientHandler>,
    config: &SshConfig,
) -> AppResult<()> {
    if config.auth_type == "private_key" {
        let key_path = config.private_key_path
            .as_deref()
            .ok_or_else(|| AppError::Validation("密钥路径不能为空".into()))?;
        let key_pair = russh::keys::load_secret_key(key_path, config.passphrase.as_deref())
            .map_err(|e| AppError::Ssh(format!("加载密钥失败: {e}")))?;
        // 与主连接保持一致：RSA 必须协商出 SHA-256/512，否则现代服务器会拒签。
        let hash_alg = session
            .best_supported_rsa_hash()
            .await
            .map_err(|e| AppError::Ssh(format!("协商 RSA 哈希算法失败: {e}")))?
            .flatten();
        let key_with_alg = russh::keys::PrivateKeyWithHashAlg::new(
            Arc::new(key_pair),
            hash_alg,
        );
        session
            .authenticate_publickey(&config.username, key_with_alg)
            .await
            .map_err(|e| AppError::Ssh(format!("密钥认证失败: {e}")))?;
    } else {
        let pwd = config.password
            .as_deref()
            .ok_or_else(|| AppError::Validation("密码不能为空".into()))?;
        session
            .authenticate_password(&config.username, pwd)
            .await
            .map_err(|e| AppError::Ssh(format!("密码认证失败: {e}")))?;
    }
    Ok(())
}

/// 打开 SFTP 会话（新建立 SSH 连接 + sftp 子系统；传输模块共用）。
/// 连接/认证失败返回 Err；子系统不可用返回 Ok(None)（调用方回退 base64 通道）
pub(crate) async fn open_sftp_session(
    config: &SshConfig,
) -> AppResult<Option<(russh_sftp::client::SftpSession, tokio::task::JoinHandle<()>)>> {
    let ssh_config = Arc::new(client::Config {
        // 传输专用：空闲容忍度放宽到 10 分钟，避免大文件慢速传输被断开
        inactivity_timeout: Some(Duration::from_secs(600)),
        // 传输吞吐优化：russh 默认首选 chacha20-poly1305（纯 Rust 实现、无硬件加速，
        // 吞吐受限尤其 debug 构建下仅数 MB/s）。改为优先协商 AES-GCM / AES-CTR：
        // AES 走 AES-NI 硬件指令、GCM 的 GHASH 走 CLMUL 加速，且免去 HMAC 纯软哈希开销。
        // 兼容旧服务端：chacha20-poly1305 保留在末位兜底
        preferred: russh::Preferred {
            cipher: std::borrow::Cow::Owned(vec![
                russh::cipher::AES_256_GCM,
                russh::cipher::AES_256_CTR,
                russh::cipher::AES_192_CTR,
                russh::cipher::AES_128_CTR,
                russh::cipher::CHACHA20_POLY1305,
            ]),
            ..Default::default()
        },
        ..Default::default()
    });
    let mut session = establish_client(ssh_config, &config.host, config.port, config.proxy.as_ref(), 10_000).await?;
    authenticate_session(&mut session, config).await?;

    let channel = match session.channel_open_session().await {
        Ok(c) => c,
        Err(_) => return Ok(None),
    };
    if channel.request_subsystem(true, "sftp").await.is_err() {
        return Ok(None);
    }
    let drive_handle = tokio::spawn(async move {
        let _ = session.await;
    });
    // Channel 本身不实现 AsyncRead/AsyncWrite，需转为 ChannelStream 提供给 SFTP 会话
    let stream = channel.into_stream();
    match russh_sftp::client::SftpSession::new(stream).await {
        Ok(sftp) => Ok(Some((sftp, drive_handle))),
        Err(e) => {
            tracing::warn!(err = %e, "SFTP 协议初始化失败，将回退 base64 通道");
            drive_handle.abort();
            Ok(None)
        }
    }
}

/// 在【已建立且已认证】的连接上开 SFTP 子系统（复用同一条 SSH 连接，不重拨）。
/// 失败返回 None，不阻断主流程；调用方负责超时控制。
async fn open_sftp_subsystem_on(
    session: &client::Handle<ClientHandler>,
) -> Option<russh_sftp::client::SftpSession> {
    let channel = session.channel_open_session().await.ok()?;
    if channel.request_subsystem(true, "sftp").await.is_err() {
        tracing::warn!("远端不支持 sftp 子系统，文件浏览将回退重拨/exec 通道");
        return None;
    }
    match russh_sftp::client::SftpSession::new(channel.into_stream()).await {
        Ok(sftp) => Some(sftp),
        Err(e) => {
            tracing::warn!(err = %e, "SFTP 协议初始化失败（复用主连接）");
            None
        }
    }
}

/// 通过新建 SSH 连接执行远程命令并返回输出（不检查退出码）?
pub(crate) async fn exec_remote_command(config: &SshConfig, command: &str) -> AppResult<String> {
    let (mut channel, drive_handle) = open_exec_stream(config, command).await?;

    let mut output = String::new();
    let mut stderr = String::new();

    // 命令执行整体加 30s 兜底：服务器执行完毕但退出信号丢失（半死连接）
    // 时不再永久挂起；超时后 drive.abort() 强制中断 russh 子任务。
    let exec = async {
        while let Some(msg) = channel.wait().await {
            match msg {
                ChannelMsg::Data { data } => {
                    output.push_str(&String::from_utf8_lossy(data.as_ref()));
                }
                ChannelMsg::ExtendedData { data, .. } => {
                    stderr.push_str(&String::from_utf8_lossy(data.as_ref()));
                }
                ChannelMsg::ExitStatus { .. } => {
                    break;
                }
                ChannelMsg::Eof | ChannelMsg::Close => {
                    break;
                }
                _ => {}
            }
        }
        Ok::<_, AppError>(())
    };
    if tokio::time::timeout(Duration::from_secs(30), exec).await.is_err() {
        tracing::warn!(command = %command, "远程命令执行超时(30s)，强制中断");
    }

    let _ = channel.close().await;
    drive_handle.abort();

    if !stderr.is_empty() {
        if !output.is_empty() {
            output.push_str("\n");
        }
        output.push_str(&stderr);
    }

    Ok(output)
}

/// Shell 单引号转义（POSIX 风格，压缩/解压等命令拼接用；传输模块共用）
pub(crate) fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// 执行远程命令并检查退出码（非 0 返回输出作为错误；压缩/解压/删除等需成败判定的操作用）
async fn exec_remote_checked(config: &SshConfig, command: &str) -> AppResult<String> {
    let (mut channel, drive_handle) = open_exec_stream(config, command).await?;
    let mut output = String::new();
    let mut stderr = String::new();
    let mut exit: Option<u32> = None;
    while let Some(msg) = channel.wait().await {
        match msg {
            ChannelMsg::Data { data } => output.push_str(&String::from_utf8_lossy(data.as_ref())),
            ChannelMsg::ExtendedData { data, .. } => stderr.push_str(&String::from_utf8_lossy(data.as_ref())),
            ChannelMsg::ExitStatus { exit_status } => exit = Some(exit_status),
            ChannelMsg::Eof | ChannelMsg::Close => break,
            _ => {}
        }
    }
    let _ = channel.close().await;
    drive_handle.abort();
    if exit.map(|s| s != 0).unwrap_or(false) {
        let detail = if stderr.trim().is_empty() { output.trim().to_string() } else { stderr.trim().to_string() };
        let detail = if detail.is_empty() { format!("exit {}", exit.unwrap_or(0)) } else { detail };
        return Err(AppError::Ssh(detail));
    }
    Ok(output)
}

/// 建立 russh SSH 连接（超时/保活由设置项传入）
pub async fn connect_ssh(
    app: &AppHandle,
    session_id: &str,
    host: &str,
    port: u16,
    username: &str,
    auth_type: &str,
    password: Option<&str>,
    private_key_path: Option<&str>,
    passphrase: Option<&str>,
    cols: u16,
    rows: u16,
    connect_timeout_secs: Option<u64>,
    keep_alive_secs: Option<u64>,
    proxy_addr: Option<&str>,
    proxy_kind: Option<&str>,
) -> AppResult<Box<dyn SshSessionHandle>> {
    // SSH 保活：russh 中真正负责发保活包的是 `keepalive_interval`
    // （空闲 N 秒后发送 keepalive@openssh.com），而 `inactivity_timeout`
    // 只是「空闲 N 秒未收到数据就回收连接」，两者语义完全不同。
    //
    // 修复前只设置了 inactivity_timeout，且开启保活时反而把它置为 None，
    // 导致 keepalive_interval 始终是默认的 None —— 也就是说从未真正发过保活包。
    // 大文件上传时终端连接长时间空转（上传走的是另一条独立的 SFTP 连接），
    // 服务端 / NAT 防火墙的空闲超时会直接掐断 TCP，
    // 表现为「传输过程中会话突然断开，随后自动重连」。
    let keepalive = keep_alive_secs.filter(|s| *s > 0).map(Duration::from_secs);
    let config = Arc::new(client::Config {
        // 开启保活后由 keepalive 维持链路，不再用 inactivity_timeout 回收；
        // 未开启时保留 1 小时空闲回收作为兜底。
        inactivity_timeout: if keepalive.is_some() {
            None
        } else {
            Some(Duration::from_secs(3600))
        },
        keepalive_interval: keepalive,
        ..Default::default()
    });

    let addr = format!("{}:{}", host, port);
    tracing::info!(%addr, %username, "SSH 开始连接");
    let proxy = parse_proxy(proxy_addr, proxy_kind);

    let timeout_ms = connect_timeout_secs
        .filter(|s| *s > 0)
        .map(|s| s * 1000)
        .unwrap_or(crate::common::SSH_CONNECT_TIMEOUT_MS);
    let mut session = establish_client(config, host, port, proxy.as_ref(), timeout_ms).await?;
    tracing::info!("SSH 握手完成");

    // 认证
    tracing::info!(%auth_type, "开始 SSH 认证");
    let auth_result = if auth_type == "private_key" {
        let key_path = private_key_path
            .ok_or_else(|| AppError::Validation("密钥路径不能为空".into()))?;
        let key_pair = russh::keys::load_secret_key(key_path, passphrase)
            .map_err(|e| AppError::Ssh(format!("加载密钥失败: {e}")))?;
        // 与 russh 官方示例一致：RSA 必须协商出 SHA-256/512 哈希算法，
        // 否则签名退化为 SHA-1，会被现代 OpenSSH 服务器拒绝（表现为「认证失败」）。
        let hash_alg = session
            .best_supported_rsa_hash()
            .await
            .map_err(|e| AppError::Ssh(format!("协商 RSA 哈希算法失败: {e}")))?
            .flatten();
        let key_with_alg = russh::keys::PrivateKeyWithHashAlg::new(
            Arc::new(key_pair),
            hash_alg,
        );
        session
            .authenticate_publickey(username, key_with_alg)
            .await
            .map_err(|e| AppError::Ssh(format!("密钥认证失败: {e}")))?
    } else {
        let pwd = password
            .ok_or_else(|| AppError::Validation("密码不能为空".into()))?;
        session
            .authenticate_password(username, pwd)
            .await
            .map_err(|e| AppError::Ssh(format!("密码认证失败: {e}")))?
    };

    if !auth_result.success() {
        tracing::warn!("SSH 认证失败");
        let msg = if auth_type == "private_key" {
            "认证失败：用户名或密钥不匹配（请检查私钥、口令或用户名权限）"
        } else {
            "认证失败：用户名或密码错误"
        };
        return Err(AppError::Ssh(msg.into()));
    }
    tracing::info!("SSH 认证成功");

    // 保存连接参数用于后续 exec 命令（含代理，保证文件传输与命令执行同链路）
    let ssh_config = SshConfig {
        host: host.to_string(),
        port,
        username: username.to_string(),
        auth_type: auth_type.to_string(),
        password: password.map(|s| s.to_string()),
        private_key_path: private_key_path.map(|s| s.to_string()),
        passphrase: passphrase.map(|s| s.to_string()),
        proxy,
    };

    // 打开 shell channel（用于交互式终端）?
    tracing::info!("打开 SSH 通道");
    let mut channel = session
        .channel_open_session()
        .await
        .map_err(|e| AppError::Ssh(format!("打开通道失败: {e}")))?;

    // 请求 PTY + 交互式 shell
    tracing::info!("请求 PTY {}x{}", cols, rows);
    channel
        .request_pty(true, "xterm-256color", cols as u32, rows as u32, 0, 0, &[])
        .await
        .map_err(|e| AppError::Ssh(format!("请求 PTY 失败: {e}")))?;

    // 使用 exec 启动交互式 bash -i，确保 Tab 补全与 readline 功能正常
    // 先由外层 shell 打印自身 PID（exec 替换进程，PID 不变），用于后续 /proc/<pid>/cwd 跟踪目录
    // 该标记行会在 I/O 循环中被识别并剥离，不会显示到终端?
    channel
        .exec(true, "printf 'JC_SHELL_PID:%s\\n' $$; exec bash -i".as_bytes())
        .await
        .map_err(|e| AppError::Ssh(format!("启动交互式 shell 失败: {e}")))?;
    tracing::info!("交互式 shell 已启动");

    // mpsc 通道
    let (input_tx, mut input_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let (close_tx, mut close_rx) = mpsc::unbounded_channel::<()>();
    let (resize_tx, mut resize_rx) = mpsc::unbounded_channel::<(u16, u16)>();

    let app_clone = app.clone();
    let sid = session_id.to_string();

    // 在把 session 交给驱动任务之前，先在【同一条连接】上开好 SFTP 子系统并缓存：
    // 文件浏览列目录直接复用，无需再新建第二条 SSH 连接 + 重认证（这是此前"连接/浏览很慢"的主因）。
    // 失败（服务器不支持 SFTP / 请求超时）不阻断连接，list_dir 会退回 open_sftp_session 重拨。
    let sftp_on_conn = match tokio::time::timeout(
        Duration::from_secs(8),
        open_sftp_subsystem_on(&session),
    )
    .await
    {
        Ok(s) => s,
        Err(_) => {
            tracing::warn!("SFTP 子系统请求超时(8s)，文件浏览将回退重拨");
            None
        }
    };

    // 关键：单独 spawn 一个 task 驱动 session（russh 需要持续 poll Handle）
    let session_drive_handle = tokio::spawn(async move {
        let result = session.await;
        if let Err(e) = result {
            tracing::warn!(sid, %e, "SSH session 驱动结束");
            let _ = app_clone.emit("ssh-status", SshStatusPayload {
                session_id: sid.clone(),
                event: "disconnected".into(),
                message: Some(e.to_string()),
            });
        } else {
            tracing::info!(sid, "SSH session 驱动正常结束");
        }
    });

    // I/O 任务：读取 channel 数据 + 写入前端输入
    let io_app = app.clone();
    let io_sid = session_id.to_string();
    let shell_pid = Arc::new(AtomicU64::new(0));
    let shell_pid_io = shell_pid.clone();
    tokio::spawn(async move {
        // 启动初期从输出流中捕获并剥离 PID 标记行?
        let mut pid_buf: Vec<u8> = Vec::new();
        let mut pid_done = false;
        loop {
            tokio::select! {
                msg = channel.wait() => {
                    match msg {
                        Some(ChannelMsg::Data { data }) => {
                            if pid_done {
                                let text = String::from_utf8_lossy(data.as_ref()).to_string();
                                let _ = io_app.emit("ssh-output", SshOutputPayload {
                                    session_id: io_sid.clone(),
                                    data: text,
                                });
                            } else {
                                pid_buf.extend_from_slice(data.as_ref());
                                match extract_shell_pid(&pid_buf) {
                                    Some((pid, consumed)) => {
                                        tracing::info!(io_sid, pid, "捕获交互式 Shell PID");
                                        shell_pid_io.store(pid, Ordering::Relaxed);
                                        pid_done = true;
                                        let rest = pid_buf.split_off(consumed);
                                        pid_buf.clear();
                                        if !rest.is_empty() {
                                            let text = String::from_utf8_lossy(&rest).to_string();
                                            let _ = io_app.emit("ssh-output", SshOutputPayload {
                                                session_id: io_sid.clone(),
                                                data: text,
                                            });
                                        }
                                    }
                                    None => {
                                        // 兜底：前缀过长仍未见标记则原样转发
                                        if pid_buf.len() > 8192 {
                                            let text = String::from_utf8_lossy(&pid_buf).to_string();
                                            let _ = io_app.emit("ssh-output", SshOutputPayload {
                                                session_id: io_sid.clone(),
                                                data: text,
                                            });
                                            pid_buf.clear();
                                            pid_done = true;
                                        }
                                    }
                                }
                            }
                        }
                        Some(ChannelMsg::ExtendedData { data, .. }) => {
                            let text = String::from_utf8_lossy(data.as_ref()).to_string();
                            let _ = io_app.emit("ssh-output", SshOutputPayload {
                                session_id: io_sid.clone(),
                                data: text,
                            });
                        }
                        None | Some(ChannelMsg::Eof) |
                        Some(ChannelMsg::Close) |
                        Some(ChannelMsg::ExitStatus { .. }) => {
                            tracing::info!(io_sid, "SSH channel 关闭");
                            let _ = io_app.emit("ssh-status", SshStatusPayload {
                                session_id: io_sid.clone(),
                                event: "disconnected".into(),
                                message: None,
                            });
                            break;
                        }
                        _ => {}
                    }
                }
                Some(data) = input_rx.recv() => {
                    match channel.data(data.as_slice()).await {
                        Ok(_) => {}
                        Err(e) => {
                            tracing::warn!(io_sid, %e, "SSH 写入失败");
                            break;
                        }
                    }
                }
                Some((c, r)) = resize_rx.recv() => {
                    // 向远端 PTY 下发 window-change，保证远端 readline 的列宽与本地一致
                    match channel.window_change(c as u32, r as u32, 0, 0).await {
                        Ok(_) => tracing::debug!(io_sid, cols = c, rows = r, "SSH 窗口尺寸已同步"),
                        Err(e) => tracing::warn!(io_sid, %e, "SSH 发送窗口尺寸变更失败"),
                    }
                }
                _ = close_rx.recv() => {
                    tracing::info!(io_sid, "SSH 收到关闭信号");
                    let _ = channel.close().await;
                    break;
                }
            }
        }
        tracing::info!(io_sid, "SSH I/O 循环结束");
        session_drive_handle.abort();
    });

    tracing::info!(%session_id, "SSH 会话建立完成，返回 handle");
    Ok(Box::new(RusshSessionHandle {
        config: ssh_config,
        // 复用主连接的 SFTP（驱动句柄为 None，由主连接驱动任务复用）
        sftp: tokio::sync::Mutex::new(sftp_on_conn.map(|s| (s, None))),
        input_tx,
        close_tx,
        resize_tx,
        shell_pid,
    }))
}
