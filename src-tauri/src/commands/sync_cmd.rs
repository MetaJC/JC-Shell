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

//! 桌面端配置云同步 Commands（仅登录用户可用）
//! 作者：JC
//!
//! 安全模型:
//! - 请求携带 OAuth access_token (Bearer); 推送额外带客户端签名头 (X-JC-Sign)
//! - HTTP 全部在 Rust 侧发起: 渲染进程拿不到 token, 也避开跨域
//! - 服务端只当"带修订号的文档仓", 合并算法在前端 syncMerge.ts, 这里不做任何配置语义解析

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::common::{AppError, AppResult};
use crate::commands::ai_agent_cmd::{base_and_token, http_client, sign_headers};
use crate::services::auth::OAuthClient;

const STATES_PATH: &str = "/api/openjc/client/sync/states";
const DOC_PATH: &str = "/api/openjc/client/sync/doc";
const PUSH_PATH: &str = "/api/openjc/client/sync/doc/push";

/// 云端文档(服务端实体原样映射; payload 为空表示状态行, 云端不带正文)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncDoc {
    pub doc_key: String,
    pub rev: i64,
    #[serde(default)]
    pub schema_ver: Option<i32>,
    #[serde(default)]
    pub entry_count: Option<i32>,
    #[serde(default)]
    pub payload: Option<String>,
    #[serde(default)]
    pub digest: Option<String>,
    #[serde(default)]
    pub byte_size: Option<i32>,
    #[serde(default)]
    pub source_device: Option<String>,
    /// 云端受理时间(服务端权威时钟, 字符串形如 2026-08-30 12:03:11)
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// 推送入参（base_rev = 客户端读云端时的修订号，服务端据此判定有无并发写入）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncPushDTO {
    pub doc_key: String,
    pub base_rev: Option<i64>,
    pub schema_ver: Option<i32>,
    pub entry_count: Option<i32>,
    pub payload: String,
    pub device_id: Option<String>,
}

/// 推送结果：冲突不是错误（HTTP 200 + accepted=false），前端要拿 doc 做三方合并后重试
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncPushResult {
    pub accepted: bool,
    #[serde(default)]
    pub rev: Option<i64>,
    #[serde(default)]
    pub digest: Option<String>,
    /// 服务端时钟毫秒值，客户端用它记录"上次同步时间"，不依赖本机时间
    #[serde(default)]
    pub server_time: Option<i64>,
    /// 仅冲突时下发：云端最新文档
    #[serde(default)]
    pub doc: Option<SyncDoc>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiR<T> {
    code: i32,
    message: Option<String>,
    data: Option<T>,
}

/// 拉取该用户全部文档的状态行（不含 payload，用于增量判断）
#[tauri::command]
pub async fn sync_pull_states(
    oauth: State<'_, Arc<OAuthClient>>,
) -> AppResult<Vec<SyncDoc>> {
    get_json(oauth.inner().clone(), STATES_PATH).await
}

/// 拉取单份文档正文；云端不存在返回 null（客户端按"首次同步"处理，不做任何破坏性操作）
#[tauri::command]
pub async fn sync_pull_doc(
    doc_key: String,
    oauth: State<'_, Arc<OAuthClient>>,
) -> AppResult<Option<SyncDoc>> {
    let path = format!("{DOC_PATH}?docKey={}", urlencoding(&doc_key));
    get_json_opt(oauth.inner().clone(), &path).await
}

/// 推送单份文档
#[tauri::command]
pub async fn sync_push_doc(
    dto: SyncPushDTO,
    oauth: State<'_, Arc<OAuthClient>>,
) -> AppResult<SyncPushResult> {
    let body = serde_json::to_string(&dto)
        .map_err(|e| AppError::Internal(format!("序列化同步请求失败: {e}")))?;
    post_signed(oauth.inner().clone(), PUSH_PATH, body).await
}

// ---------------------------------------------------------------------------
// HTTP 细节（与 ai_agent_cmd 共用签名与网络层，这里只补"允许 data 为 null"的分支）
// ---------------------------------------------------------------------------

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
    let parsed = exchange(req).await?;
    parsed
        .data
        .ok_or_else(|| AppError::Internal("服务未返回数据".into()))
}

async fn get_json<T: serde::de::DeserializeOwned>(
    oauth: Arc<OAuthClient>,
    path: &str,
) -> AppResult<T> {
    let opt = get_json_opt::<T>(oauth, path).await?;
    opt.ok_or_else(|| AppError::Internal("服务未返回数据".into()))
}

/// data 可为 null 的 GET（首次同步时云端确实没有文档，这不是错误）
async fn get_json_opt<T: serde::de::DeserializeOwned>(
    oauth: Arc<OAuthClient>,
    path: &str,
) -> AppResult<Option<T>> {
    let (base, token) = base_and_token(oauth).await?;
    let url = format!("{base}{path}");
    let req = http_client()?.get(&url).bearer_auth(&token);
    Ok(exchange(req).await?.data)
}

async fn exchange<T: serde::de::DeserializeOwned>(
    req: reqwest::RequestBuilder,
) -> AppResult<ApiR<T>> {
    let resp = req
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("网络请求失败: {e}")))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    let parsed: ApiR<T> = serde_json::from_str(&text).map_err(|_| {
        AppError::Internal(format!("服务响应异常 HTTP {status}: {}", abbreviate(&text)))
    })?;
    if parsed.code != 0 {
        return Err(AppError::Internal(
            parsed
                .message
                .unwrap_or_else(|| format!("请求失败 code={}", parsed.code)),
        ));
    }
    Ok(parsed)
}

/// 文档键只可能是 hosts/prefs/layout，做一次最小转义即可（不引额外依赖）
fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => out.push(c),
            _ => out.push_str(&format!("%{:02X}", c as u32)),
        }
    }
    out
}

fn abbreviate(s: &str) -> String {
    if s.len() > 200 {
        s.chars().take(200).collect()
    } else {
        s.to_string()
    }
}
