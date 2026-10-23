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

//! 端口转发：连通性探测（真实 TCP 连接）+ 真实隧道建立/拆除
//! 作者：JC
//!
//! 隧道原理（本地模式）：
//!   1. 用 russh 连上 SSH 服务器（中间服务器 / 跳板机）并完成认证；
//!   2. 在本机 bindAddr:localPort 起一个 TCP 监听；
//!   3. 每来一个本地连接，就向 SSH 服务器开一条 direct-tcpip 通道到
//!      目标主机:目标端口，然后在「本地 socket ↔ SSH 通道」之间双向搬运字节。
//!
//! 动态模式在此基础上多一层 SOCKS5 握手（只实现 CONNECT 命令）。

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client;
use russh::keys::ssh_key::PublicKey;
use russh::{Channel, ChannelMsg};
use tauri::State;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::common::{AppError, AppResult};
use crate::services::ssh::russh_impl::{establish_client_h, parse_proxy, ProxyInfo};

/// 默认探测超时（毫秒）
const DEFAULT_TIMEOUT_MS: u64 = 5000;
/// SSH 建连超时（毫秒）
const SSH_CONNECT_TIMEOUT_MS: u64 = 10_000;
/// 单次读写缓冲区大小
const BUF_SIZE: usize = 32 * 1024;

/// TCP 可达性探测：尝试与本机直连的 `host:port` 建立 TCP 连接（带超时）。
///
/// 端口转发场景中，本机能直接触达的只有 SSH 服务器，故用它验证所填 SSH 地址
/// 是否真实可达（而不是仅做「非空」这种假校验）。成功返回 `Ok(())`；
/// 失败返回带原因的 `AppError`，供前端把对应节点/段标记为失败。
#[tauri::command]
pub async fn tcp_probe(host: String, port: u16, timeout_ms: Option<u64>) -> AppResult<()> {
    let host = host.trim().to_string();
    if host.is_empty() {
        return Err(AppError::Validation("主机地址为空".into()));
    }
    if port == 0 {
        return Err(AppError::Validation("端口不合法".into()));
    }

    let ms = timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS).clamp(500, 30_000);
    let addr = format!("{host}:{port}");
    match tokio::time::timeout(
        Duration::from_millis(ms),
        tokio::net::TcpStream::connect(addr.as_str()),
    )
    .await
    {
        Ok(Ok(_stream)) => Ok(()),
        Ok(Err(e)) => Err(AppError::Internal(format!("无法连接 {addr}：{e}"))),
        Err(_) => Err(AppError::Internal(format!("连接 {addr} 超时（{ms} ms）"))),
    }
}

/* ===================== 真实隧道 ===================== */

/// 端口转发规则（前端 ForwardRule 的子集；字段名与前端 JSON 保持一致的下划线风格）
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PortForwardRuleDto {
    pub id: String,
    /// local | remote | dynamic
    pub mode: String,
    #[serde(default)]
    pub bind_addr: String,
    pub local_port: u16,
    pub ssh_host: String,
    pub ssh_port: u16,
    pub ssh_user: String,
    /// password | key
    #[serde(default = "default_auth_type")]
    pub ssh_auth_type: String,
    #[serde(default)]
    pub ssh_password: String,
    #[serde(default)]
    pub ssh_key_path: String,
    #[serde(default)]
    pub ssh_passphrase: String,
    #[serde(default)]
    pub remote_target_host: String,
    #[serde(default)]
    pub remote_target_port: u16,
    /// 远程模式：在 SSH 服务器上开放的端口
    #[serde(default)]
    pub remote_port: u16,
    /// 远程模式：本机侧的目标主机 / 端口
    #[serde(default)]
    pub local_target_host: String,
    #[serde(default)]
    pub local_target_port: u16,
}

fn default_auth_type() -> String {
    "password".to_string()
}

/// 一条运行中的隧道
struct Tunnel {
    /// 发送即触发关闭（监听循环退出、SSH 连接随之释放）
    stop: tokio::sync::oneshot::Sender<()>,
}

/// 端口转发隧道管理器（作为 Tauri 托管状态）
#[derive(Default)]
pub struct PortForwardManager {
    tunnels: Mutex<HashMap<String, Tunnel>>,
}

/// 公钥校验：与终端保持一致，直接接受（不校验 known_hosts）
#[derive(Clone)]
struct ForwardHandler {
    /// 远程模式：服务器把每个入站连接作为 forwarded-tcpip 通道回推，经此交给隧道任务
    incoming: Option<tokio::sync::mpsc::UnboundedSender<Channel<client::Msg>>>,
}

impl client::Handler for ForwardHandler {
    type Error = russh::Error;

    async fn check_server_key(&mut self, _key: &PublicKey) -> Result<bool, Self::Error> {
        Ok(true)
    }

    /// 远程模式下服务器接受了 tcpip-forward 后，每条入站连接都会回调到这里
    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: Channel<client::Msg>,
        _connected_address: &str,
        _connected_port: u32,
        _originator_address: &str,
        _originator_port: u32,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        if let Some(tx) = &self.incoming {
            let _ = tx.send(channel);
        }
        Ok(())
    }
}

/// 建立 SSH 连接并完成认证。
/// 返回裸 Handle（不包 Arc）：`tcpip_forward` 需要 `&mut self`，
/// 调用方按需自行包 Arc（本地 / 动态模式的并发开通道用）。
///
/// `proxy` 来自前端「通用-网络代理」设置，与终端会话共用同一解析——
/// 否则会出现「终端（走代理）能连、端口转发（直连）连不上」的不一致。
async fn connect_and_auth(
    rule: &PortForwardRuleDto,
    incoming: Option<tokio::sync::mpsc::UnboundedSender<Channel<client::Msg>>>,
    proxy: Option<&ProxyInfo>,
) -> AppResult<client::Handle<ForwardHandler>> {
    let config = Arc::new(client::Config {
        // 隧道是长连接，交给 TCP keepalive / 用户关闭，不做空闲超时
        inactivity_timeout: None,
        ..Default::default()
    });

    let host = rule.ssh_host.trim().to_string();
    if host.is_empty() {
        return Err(AppError::Validation("SSH 服务器地址为空".into()));
    }

    let mut session = establish_client_h(
        config,
        &host,
        rule.ssh_port,
        proxy,
        SSH_CONNECT_TIMEOUT_MS,
        || ForwardHandler { incoming },
    )
    .await?;

    if rule.ssh_auth_type == "key" {
        let path = rule.ssh_key_path.trim();
        if path.is_empty() {
            return Err(AppError::Validation("私钥文件不能为空".into()));
        }
        let key_pair = russh::keys::load_secret_key(
            path,
            if rule.ssh_passphrase.is_empty() { None } else { Some(rule.ssh_passphrase.as_str()) },
        )
        .map_err(|e| AppError::Ssh(format!("加载私钥失败：{e}")))?;
        // 与终端一致：RSA 必须协商出 SHA-256/512，否则现代服务器会拒签
        let hash_alg = session
            .best_supported_rsa_hash()
            .await
            .map_err(|e| AppError::Ssh(format!("协商 RSA 哈希算法失败：{e}")))?
            .flatten();
        let key = russh::keys::PrivateKeyWithHashAlg::new(Arc::new(key_pair), hash_alg);
        let res = session
            .authenticate_publickey(&rule.ssh_user, key)
            .await
            .map_err(|e| AppError::Ssh(format!("密钥认证失败：{e}")))?;
        if !res.success() {
            return Err(AppError::Ssh("密钥认证失败：请检查私钥与登录名".into()));
        }
    } else {
        if rule.ssh_password.is_empty() {
            return Err(AppError::Validation("密码不能为空".into()));
        }
        let res = session
            .authenticate_password(&rule.ssh_user, rule.ssh_password.as_str())
            .await
            .map_err(|e| AppError::Ssh(format!("密码认证失败：{e}")))?;
        if !res.success() {
            return Err(AppError::Ssh("认证失败：登录名或密码错误".into()));
        }
    }

    Ok(session)
}

/// 在「本地 socket ↔ 已打开的 SSH 通道」之间双向搬运字节
async fn pump_channel(mut channel: Channel<client::Msg>, local: TcpStream) -> AppResult<()> {
    let (mut local_r, mut local_w) = tokio::io::split(local);
    let mut buf = vec![0u8; BUF_SIZE];

    loop {
        tokio::select! {
            msg = channel.wait() => match msg {
                Some(ChannelMsg::Data { data }) => {
                    if local_w.write_all(data.as_ref()).await.is_err() {
                        break;
                    }
                }
                Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => break,
                _ => {}
            },
            read = local_r.read(&mut buf) => match read {
                Ok(0) => break,
                Ok(n) => {
                    if channel.data(&buf[..n]).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            },
        }
    }

    let _ = local_w.shutdown().await;
    let _ = channel.eof().await;
    let _ = channel.close().await;
    Ok(())
}

/// 本地模式：把一条本地连接直连到目标主机:端口
async fn forward_direct(
    session: Arc<client::Handle<ForwardHandler>>,
    local: TcpStream,
    host: &str,
    port: u16,
    peer: SocketAddr,
) -> AppResult<()> {
    let channel = session
        .channel_open_direct_tcpip(
            host.to_string(),
            port as u32,
            peer.ip().to_string(),
            peer.port() as u32,
        )
        .await
        .map_err(|e| AppError::Ssh(format!("请求转发到 {host}:{port} 失败：{e}")))?;
    pump_channel(channel, local).await
}

/// 动态模式：先做 SOCKS5 协商（仅 CONNECT），再直连到请求的目标
async fn forward_socks5(
    session: Arc<client::Handle<ForwardHandler>>,
    mut local: TcpStream,
    peer: SocketAddr,
) -> AppResult<()> {
    // 1) 方法协商：只接受「无认证」
    let mut head = [0u8; 2];
    local
        .read_exact(&mut head)
        .await
        .map_err(|e| AppError::Internal(format!("SOCKS5 读取失败：{e}")))?;
    if head[0] != 0x05 {
        return Err(AppError::Internal("SOCKS5 版本不支持".into()));
    }
    let mut methods = vec![0u8; head[1] as usize];
    local
        .read_exact(&mut methods)
        .await
        .map_err(|e| AppError::Internal(format!("SOCKS5 读取失败：{e}")))?;
    local
        .write_all(&[0x05, 0x00])
        .await
        .map_err(|e| AppError::Internal(format!("SOCKS5 应答失败：{e}")))?;

    // 2) 请求：VER CMD RSV ATYP
    let mut req = [0u8; 4];
    local
        .read_exact(&mut req)
        .await
        .map_err(|e| AppError::Internal(format!("SOCKS5 读取失败：{e}")))?;
    if req[1] != 0x01 {
        // 只支持 CONNECT，其余回「命令不支持」
        let _ = local.write_all(&[0x05, 0x07, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await;
        return Ok(());
    }
    let host = match req[3] {
        0x01 => {
            let mut a = [0u8; 4];
            local.read_exact(&mut a).await.map_err(|e| AppError::Internal(format!("SOCKS5 读取失败：{e}")))?;
            std::net::Ipv4Addr::from(a).to_string()
        }
        0x03 => {
            let mut l = [0u8; 1];
            local.read_exact(&mut l).await.map_err(|e| AppError::Internal(format!("SOCKS5 读取失败：{e}")))?;
            let mut d = vec![0u8; l[0] as usize];
            local.read_exact(&mut d).await.map_err(|e| AppError::Internal(format!("SOCKS5 读取失败：{e}")))?;
            String::from_utf8_lossy(&d).to_string()
        }
        0x04 => {
            let mut a = [0u8; 16];
            local.read_exact(&mut a).await.map_err(|e| AppError::Internal(format!("SOCKS5 读取失败：{e}")))?;
            std::net::Ipv6Addr::from(a).to_string()
        }
        _ => {
            let _ = local.write_all(&[0x05, 0x08, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await;
            return Ok(());
        }
    };
    let mut p = [0u8; 2];
    local
        .read_exact(&mut p)
        .await
        .map_err(|e| AppError::Internal(format!("SOCKS5 读取失败：{e}")))?;
    let port = u16::from_be_bytes(p);

    // 3) 经 SSH 建通道，成功后才回「成功」应答
    let channel = match session
        .channel_open_direct_tcpip(
            host.clone(),
            port as u32,
            peer.ip().to_string(),
            peer.port() as u32,
        )
        .await
    {
        Ok(c) => c,
        Err(e) => {
            let _ = local.write_all(&[0x05, 0x05, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await;
            return Err(AppError::Ssh(format!("请求转发到 {host}:{port} 失败：{e}")));
        }
    };
    local
        .write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
        .await
        .map_err(|e| AppError::Internal(format!("SOCKS5 应答失败：{e}")))?;

    // 4) 双向搬运
    pump_channel(channel, local).await
}

/// 关闭并移除一条隧道（幂等）
fn stop_tunnel(manager: &PortForwardManager, id: &str) {
    if let Some(t) = manager.tunnels.lock().unwrap().remove(id) {
        let _ = t.stop.send(());
    }
}

/// 取规则里的 SSH 会话（含认证）
fn bind_host(rule: &PortForwardRuleDto) -> String {
    let h = rule.bind_addr.trim();
    if h.is_empty() { "127.0.0.1".to_string() } else { h.to_string() }
}

/// 启动一条转发规则，返回用于访问的端口。
///
/// - 本地模式：本机监听 localPort，转发到服务器侧的目标主机:端口，返回 localPort；
/// - 动态模式：本机监听 localPort 做 SOCKS5，返回 localPort；
/// - 远程模式：请服务器监听 remotePort，入站连接转回本机的本地目标，返回 remotePort。
#[tauri::command]
pub async fn port_forward_start(
    manager: State<'_, PortForwardManager>,
    rule: PortForwardRuleDto,
    proxy_addr: Option<String>,
    proxy_kind: Option<String>,
) -> AppResult<u16> {
    let proxy = parse_proxy(proxy_addr.as_deref(), proxy_kind.as_deref());
    let is_remote = rule.mode == "remote";
    if is_remote {
        if rule.remote_port == 0 {
            return Err(AppError::Validation("服务器监听端口不能为空".into()));
        }
    } else if rule.local_port == 0 {
        return Err(AppError::Validation("监听端口不能为空".into()));
    }

    // 同 id 的旧隧道先停掉，避免端口被占用
    stop_tunnel(manager.inner(), &rule.id);

    let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel::<()>();
    let id = rule.id.clone();
    let port: u16;

    if is_remote {
        /* ===== 远程模式：服务器监听，回连本机目标 ===== */
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let mut session = connect_and_auth(&rule, Some(tx), proxy.as_ref()).await?;

        let host = bind_host(&rule);
        let remote_port = rule.remote_port;
        session
            .tcpip_forward(host.clone(), remote_port as u32)
            .await
            .map_err(|e| {
                AppError::Ssh(format!("请求服务器监听 {host}:{remote_port} 失败：{e}"))
            })?;

        let target_host = rule.local_target_host.clone();
        let target_port = rule.local_target_port;
        let id = id.clone();
        port = remote_port;

        tokio::spawn(async move {
            // 任务结束时需要 &mut 调 cancel_tcpip_forward
            let mut session = session;
            tracing::info!(id, %host, port = remote_port, "远程转发已启动");
            loop {
                tokio::select! {
                    _ = &mut stop_rx => break,
                    ch = rx.recv() => {
                        let Some(channel) = ch else { break };
                        let id = id.clone();
                        let addr = format!("{}:{}", target_host.trim(), target_port);
                        tokio::spawn(async move {
                            match TcpStream::connect(&addr).await {
                                Ok(local) => {
                                    if let Err(e) = pump_channel(channel, local).await {
                                        tracing::warn!(id, %e, "远程转发连接结束");
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(id, addr, %e, "连接本机目标失败");
                                }
                            }
                        });
                    }
                }
            }
            // 撤销服务器上的监听
            let _ = session.cancel_tcpip_forward(host, remote_port as u32).await;
            tracing::info!(id, "远程转发已停止");
        });
    } else {
        /* ===== 本地 / 动态模式：本机监听，经 SSH 直连服务器侧目标 ===== */
        // 用 Arc 共享同一个 SSH 会话：每条本地连接都要去开通道
        let session = Arc::new(connect_and_auth(&rule, None, proxy.as_ref()).await?);

        let host = bind_host(&rule);
        let bind = format!("{host}:{}", rule.local_port);
        let listener = TcpListener::bind(&bind)
            .await
            .map_err(|e| AppError::Internal(format!("监听 {bind} 失败：{e}")))?;
        port = listener.local_addr().map(|a| a.port()).unwrap_or(rule.local_port);

        let dynamic = rule.mode == "dynamic";
        let target_host = rule.remote_target_host.clone();
        let target_port = rule.remote_target_port;
        let id = id.clone();

        tokio::spawn(async move {
            tracing::info!(id, %bind, "端口转发已启动");
            loop {
                tokio::select! {
                    _ = &mut stop_rx => break,
                    accepted = listener.accept() => {
                        let (stream, peer) = match accepted {
                            Ok(v) => v,
                            Err(e) => {
                                tracing::warn!(id, %e, "接受本地连接失败");
                                continue;
                            }
                        };
                        let session = Arc::clone(&session);
                        let id = id.clone();
                        let target_host = target_host.clone();
                        tokio::spawn(async move {
                            let r = if dynamic {
                                forward_socks5(session, stream, peer).await
                            } else {
                                if target_host.trim().is_empty() {
                                    return;
                                }
                                forward_direct(session, stream, &target_host, target_port, peer).await
                            };
                            if let Err(e) = r {
                                tracing::warn!(id, %e, "转发连接结束");
                            }
                        });
                    }
                }
            }
            tracing::info!(id, "端口转发已停止");
        });
    }

    manager.tunnels.lock().unwrap().insert(rule.id, Tunnel { stop: stop_tx });
    Ok(port)
}

/// 端到端探测：建连 → 认证 → 真的向目标主机:端口开一条转发通道（不占用本地端口）。
/// 「测试连通性」用它，能把「SSH 能连上但转发通道被服务器拒绝/目标不可达」这类问题暴露出来。
#[tauri::command]
pub async fn port_forward_probe(
    rule: PortForwardRuleDto,
    proxy_addr: Option<String>,
    proxy_kind: Option<String>,
) -> AppResult<()> {
    let proxy = parse_proxy(proxy_addr.as_deref(), proxy_kind.as_deref());
    let is_remote = rule.mode == "remote";
    let mut session = if is_remote {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        connect_and_auth(&rule, Some(tx), proxy.as_ref()).await?
    } else {
        connect_and_auth(&rule, None, proxy.as_ref()).await?
    };

    // 动态模式：目标是任意外部网络，由 SOCKS5 请求决定；
    // 若测试时填了目标主机/端口，则真实开一条转发通道验证该目标可达（与本地模式一致）
    if rule.mode == "dynamic" {
        let host = rule.remote_target_host.trim();
        if !host.is_empty() && rule.remote_target_port != 0 {
            let channel = session
                .channel_open_direct_tcpip(
                    host.to_string(),
                    rule.remote_target_port as u32,
                    "127.0.0.1".to_string(),
                    0,
                )
                .await
                .map_err(|e| {
                    AppError::Ssh(format!(
                        "请求转发到 {host}:{} 失败：{e}",
                        rule.remote_target_port
                    ))
                })?;
            let _ = channel.close().await;
        }
        return Ok(());
    }

    // 远程模式：请求服务器监听端口（随即撤销），可验证账号是否有 tcpip-forward 权限
    if is_remote {
        if rule.remote_port == 0 {
            return Err(AppError::Validation("服务器监听端口为空".into()));
        }
        let bind = bind_host(&rule);
        session
            .tcpip_forward(bind.clone(), rule.remote_port as u32)
            .await
            .map_err(|e| {
                AppError::Ssh(format!(
                    "请求服务器监听 {bind}:{} 失败：{e}",
                    rule.remote_port
                ))
            })?;
        let _ = session.cancel_tcpip_forward(bind, rule.remote_port as u32).await;
        return Ok(());
    }

    let host = rule.remote_target_host.trim();
    if host.is_empty() {
        return Err(AppError::Validation("目标主机为空".into()));
    }
    if rule.remote_target_port == 0 {
        return Err(AppError::Validation("目标端口为空".into()));
    }

    let channel = session
        .channel_open_direct_tcpip(
            host.to_string(),
            rule.remote_target_port as u32,
            "127.0.0.1".to_string(),
            0,
        )
        .await
        .map_err(|e| {
            AppError::Ssh(format!(
                "请求转发到 {host}:{} 失败：{e}",
                rule.remote_target_port
            ))
        })?;
    let _ = channel.close().await;
    Ok(())
}

/// 远程模式「测试连通性」时，两台远程客户端各自的 SSH 连接信息。
/// 客户端机器本身也是一台可被 SSH 连上的主机，我们从本机 SSH 进它，
/// 再由它向 SSH 服务器监听的 `bind_addr:remote_port` 开一条转发通道，
/// 以验证「这台客户端机器能否真实连上监听端口」。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct RemoteClientDto {
    pub ssh_host: String,
    pub ssh_port: u16,
    #[serde(default = "default_auth_type")]
    pub ssh_auth_type: String,
    pub ssh_user: String,
    pub ssh_password: String,
    pub ssh_key_path: String,
    pub ssh_passphrase: String,
}

/// 单台远程客户端的真实探测结果。
/// 前端据此把对应客户端点亮为绿/红，并区分两类失败：
/// - `ssh_ok = false`：连不上 / 认证失败 —— 这台客户端根本发不出测试请求；
/// - `ssh_ok = true` 但 `target_ok = false`：SSH 没问题，但它连不上监听地址:端口（拓扑/网络不通）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct RemoteClientProbeResult {
    pub ssh_ok: bool,
    pub target_ok: bool,
    pub ssh_err: Option<String>,
    pub target_err: Option<String>,
}

/// 真实探测一台远程客户端：
/// 1) 用客户端 SSH 信息建连 + 认证；失败直接返回 ssh_ok=false（不连目标）。
/// 2) 认证通过后，从客户端机器向 `bind_addr:remote_port` 开 direct-tcpip 通道，
///    验证它能否真实连上 SSH 服务器的监听端口（这一步等价于客户端真正发起了连接请求）。
#[tauri::command]
pub async fn remote_client_probe(
    client: RemoteClientDto,
    bind_addr: String,
    remote_port: u16,
    proxy_addr: Option<String>,
    proxy_kind: Option<String>,
) -> AppResult<RemoteClientProbeResult> {
    let proxy = parse_proxy(proxy_addr.as_deref(), proxy_kind.as_deref());
    let rule = PortForwardRuleDto {
        id: String::new(),
        mode: "remote".into(),
        bind_addr: String::new(),
        local_port: 0,
        ssh_host: client.ssh_host,
        ssh_port: client.ssh_port,
        ssh_user: client.ssh_user,
        ssh_auth_type: client.ssh_auth_type,
        ssh_password: client.ssh_password,
        ssh_key_path: client.ssh_key_path,
        ssh_passphrase: client.ssh_passphrase,
        remote_target_host: String::new(),
        remote_target_port: 0,
        remote_port: 0,
        local_target_host: String::new(),
        local_target_port: 0,
    };

    let mut session = match connect_and_auth(&rule, None, proxy.as_ref()).await {
        Ok(s) => s,
        Err(e) => {
            // 原始报错进日志（前端只展示友好提示），便于对照排查
            tracing::error!("[remote_client_probe] SSH 连接/认证失败: {e}");
            return Ok(RemoteClientProbeResult {
                ssh_ok: false,
                target_ok: false,
                ssh_err: Some(format!("{e}")),
                target_err: None,
            })
        }
    };

    let target = session
        .channel_open_direct_tcpip(bind_addr.clone(), remote_port as u32, "127.0.0.1".to_string(), 0)
        .await;
    match target {
        Ok(ch) => {
            let _ = ch.close().await;
            Ok(RemoteClientProbeResult {
                ssh_ok: true,
                target_ok: true,
                ssh_err: None,
                target_err: None,
            })
        }
        Err(e) => Ok(RemoteClientProbeResult {
            ssh_ok: true,
            target_ok: false,
            ssh_err: None,
            target_err: Some(format!("客户端连不上 {bind_addr}:{remote_port}：{e}")),
        }),
    }
}

/// 停止一条转发规则（幂等）
#[tauri::command]
pub fn port_forward_stop(manager: State<'_, PortForwardManager>, id: String) -> AppResult<()> {
    stop_tunnel(manager.inner(), &id);
    Ok(())
}

/// 当前运行中的隧道 id 列表（前端刷新时用于对齐开关状态）
#[tauri::command]
pub fn port_forward_running(manager: State<'_, PortForwardManager>) -> AppResult<Vec<String>> {
    Ok(manager.tunnels.lock().unwrap().keys().cloned().collect())
}
