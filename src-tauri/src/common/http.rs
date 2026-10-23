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

//! 共享 HTTP 客户端 + 网络错误自动重试
//! 作者：JC
//!
//! 根因背景：原 qq_group_cmd.rs 等每次 `reqwest::Client::builder().build()` 新建客户端，
//! 即使 Client drop，hyper 内部 socket 仍可能被复用；服务端 keep-alive 超时（60-300s 常见）
//! 先关掉对端连接后，客户端再发请求会拿到 `error sending request`（socket RST/EOF）。
//! 用户反馈"长时间不打开软件再打开，第一次点 QQ 群偶发 INTERNAL_ERROR" 即是此原因。
//!
//! 修复：
//! 1. 进程级共享 reqwest::Client（OnceLock 单例），启用 TCP keep-alive
//! 2. 配置 pool_idle_timeout < 服务端 keep-alive，主动让客户端把空闲连接关掉，杜绝 reuse-after-close
//! 3. `send_with_retry` 工具：网络层错误（连接 RST / 超时）时自动重试一次，
//!    hyper 内部会把坏连接剔出连接池，重试会用全新 TCP 连接。

use std::future::Future;
use std::sync::OnceLock;
use std::time::Duration;

use hmac::{Hmac, Mac};
use reqwest::header::{HeaderMap, HeaderValue};
use sha2::{Digest, Sha256};
use tokio::time::sleep;

use crate::common::{AppError, AppResult, AI_CLIENT_SIGN_SECRET};

static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

/// 品牌标识：JC Shell 社区版（OpenJC）。
/// 出现在所有出站请求的 User-Agent 中，是本仓库「出处署名」的功能性落点之一。
/// 以常量形式被代码引用，移除会直接导致编译失败，从而提高二开时抹除出处的成本。
const JC_SHELL_USER_AGENT_BRAND: &str = "JC Shell";
/// 官方站点，作为 User-Agent 的 `+(官网)` 后缀携带。
const JC_SHELL_OFFICIAL_SITE: &str = "https://www.openjc.cc";

/// 客户端 User-Agent。
///
/// 关键：reqwest 默认【不发送】User-Agent，而站点 WAF 的「空UA拦截」会拦截无 UA 请求
/// 并自动拉黑来源 IP —— APP 每次心跳/请求都在给自家 IP "攒" 封禁，
/// 表现就是「用户浏览器访问官网也被 403、反复解封又反复被限制」。
/// 所以所有出站请求必须带真实 UA。
fn user_agent() -> String {
    format!(
        "JCShell/{} ({}) {} (+{})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        JC_SHELL_USER_AGENT_BRAND,
        JC_SHELL_OFFICIAL_SITE
    )
}

/// 机器级设备标识（供服务端识别/审计来源）。
/// 取主机名+用户名做摘要；移动端取不到时回退应用级常量（放行信任锚是签名密钥，不影响放行）。
fn device_id() -> String {
    let host = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_default();
    let user = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_default();
    let digest = Sha256::digest(format!("{host}|{user}|jc-shell").as_bytes());
    hex_encode(&digest)[..32].to_string()
}

/// 防护放行令牌（机器级白名单）：
///   X-JC-Pass = HMAC-SHA256(secret, "jc-guard-pass\n" + device_id)
/// 服务端 SecurityGuardFilter 校验通过后跳过一切拦截（黑名单/限流/扫描/空UA/反爬）。
/// 密钥与服务端 openjc.ai.client-sign-secret 同源（构建期 OPENJC_AI_SIGN_SECRET 注入）；
/// secret 未配置（空）时不携带该头，防护策略照常生效。
fn guard_pass_token(secret: &str, device: &str) -> String {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret.as_bytes())
        .expect("HMAC 初始化失败");
    mac.update(format!("jc-guard-pass\n{device}").as_bytes());
    hex_encode(&mac.finalize().into_bytes())
}

/// 为发往自有后端的 HTTP 客户端统一注入 UA 与防护放行默认头。
/// 所有后向 HTTP 客户端（共享客户端 / OAuth / AI / 更新下载）都必须经过这里，
/// 保证「JC Shell 发出的请求任何情况下不被自家 WAF 拦截」。
pub fn apply_client_defaults(builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    let builder = builder.user_agent(user_agent());
    if AI_CLIENT_SIGN_SECRET.is_empty() {
        return builder;
    }
    let device = device_id();
    let mut headers = HeaderMap::new();
    if let Ok(v) = HeaderValue::from_str(&device) {
        headers.insert("X-JC-Device", v);
    }
    if let Ok(v) = HeaderValue::from_str(&guard_pass_token(AI_CLIENT_SIGN_SECRET, &device)) {
        headers.insert("X-JC-Pass", v);
    }
    builder.default_headers(headers)
}

/// 生成「APP 内打开网页」所需的放行查询参数（WebView/iframe 无法自定义请求头）。
/// 返回形如 `jc_device=xxx&jc_pass=xxx`；secret 未配置时返回 None（不拼接）。
pub fn guard_pass_query() -> Option<String> {
    if AI_CLIENT_SIGN_SECRET.is_empty() {
        return None;
    }
    let device = device_id();
    Some(format!(
        "jc_device={device}&jc_pass={}",
        guard_pass_token(AI_CLIENT_SIGN_SECRET, &device)
    ))
}

fn hex_encode(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02x}")).collect()
}

/// 进程级共享 HTTP 客户端（懒初始化）
pub fn client() -> &'static reqwest::Client {
    CLIENT.get_or_init(|| {
        apply_client_defaults(
            reqwest::Client::builder()
                // 连接阶段超时：5s 内必须完成 TCP 握手（含 TLS 握手）
                .connect_timeout(Duration::from_secs(5))
                // 整体请求超时（含读 body）：15s，避免被服务端拖死
                .timeout(Duration::from_secs(15))
                // TCP 层 keep-alive：每 60s 发一次心跳包，对端 NAT/负载均衡不会过早断流
                .tcp_keepalive(Duration::from_secs(60))
                // 客户端连接池空闲连接最大存活 90s（短于常见服务端 keep-alive 60-300s），
                // 主动让客户端把空闲连接关闭，避免"服务端已关、客户端复用"导致的偶发失败
                .pool_idle_timeout(Duration::from_secs(90))
                .pool_max_idle_per_host(4),
        )
        .build()
        .expect("[http] init reqwest::Client failed (构建全局 HTTP 客户端失败)")
    })
}

/// 发送请求并在网络层错误时自动重试一次
///
/// - 只对 `reqwest::Error`（连接失败 / 读 body 失败 / 超时等）做重试
/// - 非 2xx 的 HTTP 状态码不会触发重试（业务错误由调用方自行处理）
/// - 重试前退避 200ms，期间 hyper 会把坏连接剔出连接池
pub async fn send_with_retry<F, Fut>(mut send: F) -> AppResult<reqwest::Response>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<reqwest::Response, reqwest::Error>>,
{
    match send().await {
        Ok(r) => Ok(r),
        Err(e) => {
            tracing::warn!("[http] 首次 send 失败，自动重试一次: {e}");
            sleep(Duration::from_millis(200)).await;
            send().await.map_err(|e2| {
                AppError::Internal(format!(
                    "HTTP 请求失败（重试后仍失败）: {e2}"
                ))
            })
        }
    }
}