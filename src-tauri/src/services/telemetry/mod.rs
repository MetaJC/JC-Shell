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

//! 客户端使用遥测（启动 / 心跳 / 退出）
//! 作者：JC
//!
//! 后台接口（公开，无需登录）：
//!   POST /api/openjc/client/telemetry/session/start      —— 软件启动，建会话，返回 sessionToken
//!   POST /api/openjc/client/telemetry/session/heartbeat  —— 运行中心跳（默认 60s），累加使用时长
//!   POST /api/openjc/client/telemetry/session/end        —— 软件退出，结束会话
//!
//! 设计约束：
//! 1. 全程旁路 —— 任何失败只 warn，绝不向调用方抛错、不阻断启动/退出流程；
//! 2. 未配置后端地址（自建/离线部署）时直接跳过，不发任何请求；
//! 3. 登录态只在本地取 token 字符串上传，不做任何用户内容采集。

use std::sync::Arc;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;
use tokio::sync::Mutex;

use crate::common::{http, AppResult, OAUTH_DEFAULT_SERVER_BASE_URL};
use crate::services::auth::OAuthClient;

/// 项目 code（与后台 openjc_project.code 对齐）
pub const PROJECT_CODE: &str = "jc-shell";
/// 前端心跳默认间隔（后端会在 start 响应里下发建议值，这里仅作兜底）
pub const DEFAULT_HEARTBEAT_SECS: u32 = 60;

// ===== 设备型号检测 =====
// 手机取产品型号（getprop），电脑取整机型号（BIOS / hw.model / DMI）。
// 取不到（权限/环境异常）返回 None，上报体里直接省略该字段，不阻断遥测。

/// 清洗检测结果：去空白、过滤 BIOS 默认占位值
fn clean_model(raw: String) -> Option<String> {
    let s = raw.trim().to_string();
    if s.is_empty() {
        return None;
    }
    // DIY 主机/老机器 BIOS 常见占位符，无辨识价值
    const PLACEHOLDERS: [&str; 4] = ["System Product Name", "To be filled", "Default string", "None"];
    if PLACEHOLDERS.iter().any(|p| s.eq_ignore_ascii_case(p)) {
        return None;
    }
    Some(s)
}

#[cfg(target_os = "windows")]
fn detect_device_model() -> Option<String> {
    use std::os::windows::process::CommandExt;
    // 读 BIOS 的整机型号（如 "Latitude 5420"），比 wmic（已弃用）稳
    let out = std::process::Command::new("reg")
        .args(["query", r"HKLM\HARDWARE\DESCRIPTION\System\BIOS", "/v", "SystemProductName"])
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW：启动期不闪黑框
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    // 输出形如 "    SystemProductName    REG_SZ    Latitude 5420"
    let model = String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.split("REG_SZ").nth(1))
        .unwrap_or("")
        .trim()
        .to_string();
    clean_model(model)
}

#[cfg(target_os = "macos")]
fn detect_device_model() -> Option<String> {
    let out = std::process::Command::new("sysctl").args(["-n", "hw.model"]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    clean_model(String::from_utf8_lossy(&out.stdout).to_string())
}

#[cfg(target_os = "android")]
fn detect_device_model() -> Option<String> {
    let out = std::process::Command::new("getprop").arg("ro.product.model").output().ok()?;
    if !out.status.success() {
        return None;
    }
    clean_model(String::from_utf8_lossy(&out.stdout).to_string())
}

#[cfg(target_os = "ios")]
fn detect_device_model() -> Option<String> {
    None // iOS 沙盒拿不到设备型号，留空
}

#[cfg(target_os = "linux")]
fn detect_device_model() -> Option<String> {
    let model = std::fs::read_to_string("/sys/class/dmi/id/product_name").unwrap_or_default();
    clean_model(model)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StartReq {
    project_code: String,
    client_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    platform: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    os_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    os_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    app_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    token: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BeatReq {
    session_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    app_version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiResp<T> {
    code: i32,
    #[serde(default)]
    #[allow(dead_code)]
    message: String,
    /// Option 字段在 serde 下缺省即 None，无需 `#[serde(default)]`（加了会要求 T: Default）
    data: Option<T>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartData {
    pub session_token: String,
    #[serde(default)]
    pub heartbeat_interval_seconds: u32,
    #[serde(default)]
    #[allow(dead_code)]
    pub session_timeout_seconds: u32,
}

// ===== 进程级设备信息快照 =====
// 更新上报（report-update）也要带 clientId / osVersion，但它走 UpdateService，
// 与遥测服务不是同一条链；这里用进程级静态量共享一次即可（首次 start 后就有值）。
static CLIENT_ID: OnceLock<Mutex<Option<String>>> = OnceLock::new();
static OS_VERSION: OnceLock<Mutex<Option<String>>> = OnceLock::new();

/// 写入设备信息（遥测 start 时调用）
pub async fn set_client_info(client_id: Option<String>, os_version: Option<String>) {
    if let Some(c) = client_id {
        *CLIENT_ID.get_or_init(|| Mutex::new(None)).lock().await = Some(c);
    }
    if let Some(o) = os_version {
        *OS_VERSION.get_or_init(|| Mutex::new(None)).lock().await = Some(o);
    }
}

/// 读取设备信息（供更新上报复用）
pub async fn get_client_info() -> (Option<String>, Option<String>) {
    let cid = CLIENT_ID
        .get_or_init(|| Mutex::new(None))
        .lock()
        .await
        .clone();
    let osv = OS_VERSION
        .get_or_init(|| Mutex::new(None))
        .lock()
        .await
        .clone();
    (cid, osv)
}

/// 遥测服务（进程内全局单例，由 lib.rs setup 注册为 Tauri State）
pub struct TelemetryService {
    oauth: Arc<OAuthClient>,
    /// 当前会话凭证（start 成功后写入，end 后清空）
    session_token: Mutex<Option<String>>,
    /// 设备标识（前端传入并缓存，心跳/退出复用）
    client_id: Mutex<Option<String>>,
    app_version: Mutex<Option<String>>,
}

impl TelemetryService {
    pub fn new(oauth: Arc<OAuthClient>) -> Self {
        Self {
            oauth,
            session_token: Mutex::new(None),
            client_id: Mutex::new(None),
            app_version: Mutex::new(None),
        }
    }

    /// 后端地址（与 OAuth / 更新服务同优先级：运行时配置 > 编译期常量）
    async fn base_url(&self) -> Option<String> {
        let cfg = self.oauth.cfg_snapshot().await;
        let base = if cfg.server_base_url.trim().is_empty() {
            OAUTH_DEFAULT_SERVER_BASE_URL.trim().to_string()
        } else {
            cfg.server_base_url
        };
        if base.is_empty() {
            None
        } else {
            Some(base.trim_end_matches('/').to_string())
        }
    }

    fn token(&self) -> Option<String> {
        self.oauth.access_token().ok().flatten()
    }

    /// 软件启动：建会话并保存 sessionToken
    /// 返回建议心跳间隔（秒）
    pub async fn start(
        &self,
        client_id: Option<String>,
        platform: Option<String>,
        os_family: Option<String>,
        os_version: Option<String>,
        app_version: Option<String>,
    ) -> AppResult<u32> {
        let cid = match client_id {
            Some(c) if !c.trim().is_empty() => c,
            _ => {
                tracing::warn!("[telemetry] 缺少 clientId，跳过会话上报");
                return Ok(DEFAULT_HEARTBEAT_SECS);
            }
        };
        let base = match self.base_url().await {
            Some(b) => b,
            None => {
                tracing::debug!("[telemetry] 后端地址未配置，跳过会话上报");
                return Ok(DEFAULT_HEARTBEAT_SECS);
            }
        };

        {
            let mut g = self.client_id.lock().await;
            *g = Some(cid.clone());
            let mut v = self.app_version.lock().await;
            *v = app_version.clone();
        }
        // 共享给更新上报（report-update）复用同一设备标识
        set_client_info(Some(cid.clone()), os_version.clone()).await;

        let body = StartReq {
            project_code: PROJECT_CODE.to_string(),
            client_id: cid,
            platform,
            os_family,
            device_model: detect_device_model(),
            os_version,
            app_version,
            token: self.token(),
        };
        let url = format!("{base}/api/openjc/client/telemetry/session/start");
        let data: Option<StartData> = match self.post(&url, &body).await {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("[telemetry] 会话开始上报失败（忽略）: {e}");
                None
            }
        };
        match data {
            Some(d) => {
                *self.session_token.lock().await = Some(d.session_token);
                let interval = if d.heartbeat_interval_seconds > 0 {
                    d.heartbeat_interval_seconds
                } else {
                    DEFAULT_HEARTBEAT_SECS
                };
                Ok(interval)
            }
            None => Ok(DEFAULT_HEARTBEAT_SECS),
        }
    }

    /// 心跳：累加使用时长
    pub async fn heartbeat(&self) {
        let _ = self.beat("heartbeat").await;
    }

    /// 退出：结束会话
    pub async fn end(&self) {
        let _ = self.beat("end").await;
        *self.session_token.lock().await = None;
    }

    async fn beat(&self, action: &str) {
        let token = { self.session_token.lock().await.clone() };
        let Some(token) = token else { return };
        let base = match self.base_url().await {
            Some(b) => b,
            None => return,
        };
        let body = BeatReq {
            session_token: token,
            token: self.token(),
            app_version: self.app_version.lock().await.clone(),
        };
        let url = format!("{base}/api/openjc/client/telemetry/session/{action}");
        if let Err(e) = self.post::<BeatReq, serde_json::Value>(&url, &body).await {
            tracing::warn!("[telemetry] 会话{action}上报失败（忽略）: {e}");
        }
    }

    /// 统一 POST：非 2xx / 网络错误都返回 Err，由调用方决定是否忽略
    async fn post<B: Serialize, T: DeserializeOwned>(&self, url: &str, body: &B) -> AppResult<Option<T>> {
        let resp = http::send_with_retry(|| http::client().post(url).json(body).send()).await?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(crate::common::AppError::Internal(format!(
                "HTTP {status}: {text}"
            )));
        }
        let parsed: ApiResp<T> = resp.json().await.map_err(|e| {
            crate::common::AppError::Internal(format!("解析响应失败: {e}"))
        })?;
        if parsed.code != 0 {
            return Err(crate::common::AppError::Internal(format!(
                "业务码 {}",
                parsed.code
            )));
        }
        Ok(parsed.data)
    }
}
