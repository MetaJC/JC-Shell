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

//! AI Agent Commands（自然语言 → 命令建议，经服务端网关计费转发）
//! 作者：JC
//!
//! 安全模型:
//! - 请求必须携带 OAuth access_token (Bearer) + 客户端签名头 (X-JC-Sign)
//! - 服务端返回的 risk_level 与本地 agent_guard 引擎取更严者
//! - 计费细节(成本/服务费)为服务端内部数据, 客户端只接收最新积分余额

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::State;

use crate::common::{AppError, AppResult};
use crate::services::agent_guard::{GuardLevel, GuardVerdict};
use crate::services::auth::OAuthClient;

const AGENT_PATH: &str = "/api/openjc/client/ai/agent";
const BALANCE_PATH: &str = "/api/openjc/client/ai/asset/balance";
const TXNS_PATH: &str = "/api/openjc/client/ai/asset/txns";
const USAGE_DAILY_PATH: &str = "/api/openjc/client/ai/usage/daily";

// ---------------------------------------------------------------------------
// 在途请求取消注册表
// ---------------------------------------------------------------------------

/// clientRequestId → 取消标志。前端 Ctrl+C 时调用 ai_abort，置位后 select 立即放弃在途 HTTP 请求。
fn abort_registry() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    static REG: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 取消指定客户端请求；返回是否命中在途请求
pub(crate) fn abort_agent_request(id: &str) -> bool {
    if id.is_empty() {
        return false;
    }
    let reg = abort_registry();
    let guard = reg.lock().unwrap();
    match guard.get(id) {
        Some(flag) => {
            flag.store(true, Ordering::Relaxed);
            true
        }
        None => false,
    }
}

/// 轮询等待取消标志置位（150ms 粒度，足够灵敏且开销可忽略）
async fn wait_abort(flag: Option<Arc<AtomicBool>>) {
    match flag {
        Some(f) => loop {
            if f.load(Ordering::Relaxed) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        },
        None => std::future::pending::<()>().await,
    }
}

// ---------------------------------------------------------------------------
// DTO
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentQueryDTO {
    pub prompt: String,
    pub shell_type: Option<String>,
    pub os: Option<String>,
    pub cwd: Option<String>,
    pub target: Option<String>,
    pub recent_output: Option<String>,
    pub conversation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCommandVO {
    pub cmd: String,
    pub purpose: Option<String>,
    pub consequence: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentResponseVO {
    pub request_id: Option<String>,
    pub explanation: Option<String>,
    pub risk_level: Option<String>,
    pub commands: Option<Vec<AgentCommandVO>>,
    pub need_more_info: Option<bool>,
    pub question: Option<String>,
    pub balance_remaining: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentResultVO {
    #[serde(flatten)]
    pub resp: AgentResponseVO,
    /// 本地引擎与服务端取更严后的最终级别: safe/caution/danger/blocked
    pub final_risk_level: String,
    /// 本地引擎命中原因(有则为用户可读提示)
    pub guard_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetGrantVO {
    pub points: Option<i64>,
    pub remaining: Option<i64>,
    pub source: Option<String>,
    /// 来源中文名(服务端下发, 如"月卡"/"邀请返利"), 客户端直接展示
    pub source_label: Option<String>,
    pub expire_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetBalanceVO {
    /// 可消耗总积分(永久 + 限时), 与扣费校验口径一致
    pub balance: Option<i64>,
    /// 永久有效积分合计 —— 卡片「积分余额」展示口径
    pub permanent_balance: Option<i64>,
    /// 限时积分合计
    pub expiring_balance: Option<i64>,
    /// 最早的限时积分到期时间
    pub nearest_expire_at: Option<String>,
    /// 订阅是否有效
    pub subscription_active: Option<bool>,
    /// 订阅套餐: FREE/BASIC/PRO
    pub plan_code: Option<String>,
    pub subscription_expire_at: Option<String>,
    /// 限时批次明细(永久批次不逐条下发), 已按到期更早优先排序
    pub grants: Option<Vec<AssetGrantVO>>,
}

/// 服务端统一返回包装
#[derive(Debug, Deserialize)]
struct ApiR<T> {
    code: i32,
    message: Option<String>,
    data: Option<T>,
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Agent 请求: 自然语言 → 命令建议(含作用/后果/风险)。
/// 携带 conversationId 时登记取消标志，前端 Ctrl+C 可真正中止在途请求（不再后台跑完+计费）。
#[tauri::command]
pub async fn ai_agent_query(
    dto: AgentQueryDTO,
    oauth: State<'_, Arc<OAuthClient>>,
) -> AppResult<AgentResultVO> {
    if dto.prompt.trim().is_empty() {
        return Err(AppError::Internal("指令不能为空".into()));
    }
    let client_id = dto.conversation_id.clone().unwrap_or_default();
    let abort_flag: Option<Arc<AtomicBool>> = if client_id.is_empty() {
        None
    } else {
        let flag = Arc::new(AtomicBool::new(false));
        abort_registry()
            .lock()
            .unwrap()
            .insert(client_id.clone(), flag.clone());
        Some(flag)
    };

    let body = serde_json::to_string(&dto)?;
    let oauth_arc = oauth.inner().clone();
    let query = async move {
        let resp: AgentResponseVO = post_signed(oauth_arc, AGENT_PATH, body).await?;

        // 合并风险等级: 服务端 risk_level 与本地引擎取更严者
        let cmds: Vec<String> = resp
            .commands
            .as_ref()
            .map(|v| v.iter().map(|c| c.cmd.clone()).collect())
            .unwrap_or_default();
        let local = crate::services::agent_guard::check_commands(&cmds);
        let server_level = parse_level(resp.risk_level.as_deref());
        let final_level = if local.level > server_level { local.level } else { server_level };
        let final_str = match final_level {
            GuardLevel::Safe => "safe",
            GuardLevel::Caution => "caution",
            GuardLevel::Danger => "danger",
            GuardLevel::Blocked => "blocked",
        }
        .to_string();
        Ok::<AgentResultVO, AppError>(AgentResultVO {
            resp,
            final_risk_level: final_str,
            guard_reason: local.reason,
        })
    };

    let result = tokio::select! {
        r = query => r,
        _ = wait_abort(abort_flag) => Err(AppError::Internal("请求已终止".into())),
    };

    if !client_id.is_empty() {
        abort_registry().lock().unwrap().remove(&client_id);
    }
    result
}

/// 本地危险命令评估(前端在展示确认卡片时逐条调用)
#[tauri::command]
pub async fn ai_guard_check(command: String) -> AppResult<GuardVerdict> {
    Ok(crate::services::agent_guard::check_command(&command))
}

/// 我的积分余额
#[tauri::command]
pub async fn ai_asset_balance(oauth: State<'_, Arc<OAuthClient>>) -> AppResult<AssetBalanceVO> {
    get_json(oauth.inner().clone(), BALANCE_PATH).await
}

/// 我的积分流水(分页)
#[tauri::command]
pub async fn ai_asset_txns(
    page_num: Option<u32>,
    page_size: Option<u32>,
    oauth: State<'_, Arc<OAuthClient>>,
) -> AppResult<serde_json::Value> {
    let path = format!(
        "{}?pageNum={}&pageSize={}",
        TXNS_PATH,
        page_num.unwrap_or(1),
        page_size.unwrap_or(20)
    );
    get_json(oauth.inner().clone(), &path).await
}

/// 我的用量(按日, 默认近30天)
#[tauri::command]
pub async fn ai_usage_daily(
    start_date: Option<String>,
    end_date: Option<String>,
    oauth: State<'_, Arc<OAuthClient>>,
) -> AppResult<serde_json::Value> {
    let mut path = USAGE_DAILY_PATH.to_string();
    let mut qs: Vec<String> = vec![];
    if let Some(s) = start_date.filter(|v| !v.is_empty()) {
        qs.push(format!("startDate={s}"));
    }
    if let Some(e) = end_date.filter(|v| !v.is_empty()) {
        qs.push(format!("endDate={e}"));
    }
    if !qs.is_empty() {
        path = format!("{}?{}", path, qs.join("&"));
    }
    get_json(oauth.inner().clone(), &path).await
}

// ---------------------------------------------------------------------------
// HTTP helpers
// ---------------------------------------------------------------------------

fn parse_level(s: Option<&str>) -> GuardLevel {
    match s {
        Some("caution") => GuardLevel::Caution,
        Some("danger") => GuardLevel::Danger,
        Some("blocked") => GuardLevel::Blocked,
        _ => GuardLevel::Safe,
    }
}

pub(crate) async fn base_and_token(oauth: Arc<OAuthClient>) -> AppResult<(String, String)> {
    let cfg = oauth.cfg_snapshot().await;
    if cfg.server_base_url.trim().is_empty() {
        return Err(AppError::Internal("后端地址未配置".into()));
    }
    let token = oauth.ensure_access_token().await?;
    Ok((cfg.server_base_url.trim_end_matches('/').to_string(), token))
}

pub(crate) fn http_client() -> AppResult<reqwest::Client> {
    crate::common::http::apply_client_defaults(reqwest::Client::builder())
        .timeout(Duration::from_secs(75))
        .build()
        .map_err(|e| AppError::Internal(format!("HTTP 客户端初始化失败: {e}")))
}

/// 计算签名头: HMAC-SHA256(secret, ts + "\n" + nonce + "\n" + sha256hex(body))
pub(crate) fn sign_headers(body: &str) -> Option<(String, String, String)> {
    let secret = crate::common::AI_CLIENT_SIGN_SECRET;
    if secret.is_empty() {
        return None;
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis().to_string())
        .unwrap_or_else(|_| "0".into());
    let mut nonce_bytes = [0u8; 12];
    use rand::RngCore;
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = hex_encode(&nonce_bytes);
    let payload = format!("{}\n{}\n{}", ts, nonce, sha256_hex(body.as_bytes()));
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret.as_bytes())
        .expect("HMAC 初始化失败");
    mac.update(payload.as_bytes());
    let sign = hex_encode(&mac.finalize().into_bytes());
    Some((ts, nonce, sign))
}

async fn post_signed<T: serde::de::DeserializeOwned>(
    oauth: Arc<OAuthClient>,
    path: &str,
    body: String,
) -> AppResult<T> {
    let (base, token) = base_and_token(oauth).await?;
    let url = format!("{base}{path}");
    let mut req = http_client()?
        .post(&url)
        .bearer_auth(&token)
        .header("Content-Type", "application/json")
        .body(body.clone());
    if let Some((ts, nonce, sign)) = sign_headers(&body) {
        req = req
            .header("X-JC-Timestamp", ts)
            .header("X-JC-Nonce", nonce)
            .header("X-JC-Sign", sign);
    }
    send(req).await
}

async fn get_json<T: serde::de::DeserializeOwned>(
    oauth: Arc<OAuthClient>,
    path: &str,
) -> AppResult<T> {
    let (base, token) = base_and_token(oauth).await?;
    let url = format!("{base}{path}");
    let req = http_client()?.get(&url).bearer_auth(&token);
    send(req).await
}

async fn send<T: serde::de::DeserializeOwned>(req: reqwest::RequestBuilder) -> AppResult<T> {
    let resp = req
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("网络请求失败: {e}")))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    let parsed: ApiR<T> = serde_json::from_str(&text)
        .map_err(|_| AppError::Internal(format!("服务响应异常 HTTP {status}: {}", abbreviate(&text))))?;
    if parsed.code != 0 {
        return Err(AppError::Internal(
            parsed.message.unwrap_or_else(|| format!("请求失败 code={}", parsed.code)),
        ));
    }
    parsed
        .data
        .ok_or_else(|| AppError::Internal("服务未返回数据".into()))
}

fn sha256_hex(data: &[u8]) -> String {
    hex_encode(&Sha256::digest(data))
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn abbreviate(s: &str) -> String {
    if s.len() > 200 {
        s.chars().take(200).collect()
    } else {
        s.to_string()
    }
}
