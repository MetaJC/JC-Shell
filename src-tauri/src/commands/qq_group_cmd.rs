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

//! QQ群相关 Tauri Commands
//! 作者：JC

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::common::http;
use crate::common::{AppError, AppResult, OAUTH_DEFAULT_SERVER_BASE_URL};
use crate::services::auth::OAuthClient;

/// 默认产品 key（jc-shell 在主站项目 code）
pub const DEFAULT_PRODUCT_CODE: &str = "jc-shell";

/// 客户端展示用的 QQ群（与后端 QqGroupClientVO 字段保持一致）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QqGroupItem {
    pub id: i64,
    pub qq_number: String,
    pub group_name: Option<String>,
    pub sort_order: Option<i32>,
    pub deleted: i32,
    pub project_id: Option<i64>,
    pub project_code: Option<String>,
    pub project_name: Option<String>,
    /// 群二维码图片 URL（后台为每个群单独配置；后端已转绝对 URL）
    #[serde(default)]
    pub qr_code_url: Option<String>,
}

/// 拉取当前产品下的所有QQ群（含已下线 deleted=1，前端据此划线置灰）
#[tauri::command]
pub async fn get_qq_groups(
    oauth: State<'_, Arc<OAuthClient>>,
    product_code: Option<String>,
) -> AppResult<Vec<QqGroupItem>> {
    let code = product_code
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_PRODUCT_CODE.to_string());

    let cfg = oauth.cfg_snapshot().await;
    let base = if cfg.server_base_url.trim().is_empty() {
        if OAUTH_DEFAULT_SERVER_BASE_URL.trim().is_empty() {
            return Err(AppError::Internal(
                "后端地址未配置 (server_base_url 为空). 请检查 .env 是否正确加载, 或在「设置 → 后端」中手动填写.".into(),
            ));
        }
        OAUTH_DEFAULT_SERVER_BASE_URL.to_string()
    } else {
        cfg.server_base_url
    };
    // 把 client 自己的 server_base_url 作为 baseUrl 参数透传给后端，
    // 后端优先用这个值拼二维码绝对 URL；透传失败时回退到 request host。
    let url = format!(
        "{}/api/openjc/client/projects/{}/qq-groups?baseUrl={}",
        base.trim_end_matches('/'),
        code,
        url_encode(&base)
    );
    tracing::info!("[qq-group] base={} url={}", base, url);

    // 复用共享 HTTP 客户端（带 tcp_keepalive + pool_idle_timeout + 网络错误自动重试一次），
    // 修复"长时间空闲后第一次点击 QQ 群偶发 INTERNAL_ERROR" 的根因：
    // 之前每次新建 reqwest::Client，服务端 keep-alive 先关闭旧连接，客户端复用旧 socket 拿到 RST。
    let resp = http::send_with_retry(|| http::client().get(&url).send()).await?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    tracing::info!("[qq-group] http={} body={}", status, text);
    if !status.is_success() {
        return Err(AppError::Internal(format!("拉取QQ群失败 HTTP {status}: {text}")));
    }

    // 兼容后端两种响应形态：R<T> 包 {code, data, message} / 或裸数组
    #[derive(Deserialize)]
    struct R<T> {
        code: i32,
        #[serde(default)]
        data: Option<T>,
        #[serde(default)]
        message: Option<String>,
    }
    let parsed: R<Vec<QqGroupItem>> = serde_json::from_str(&text)
        .map_err(|e| AppError::Internal(format!("QQ群响应解析失败: {e}")))?;
    if parsed.code != 0 {
        return Err(AppError::Internal(
            parsed.message.unwrap_or_else(|| format!("业务失败 code={}", parsed.code)),
        ));
    }
    let mut items = parsed.data.unwrap_or_default();

    // 兜底：后端已根据 request host/client baseUrl 拼成绝对 URL；
    // 万一仍返回相对路径，这里再补拼一次 server_base_url。
    let base_trim = base.trim_end_matches('/');
    for it in items.iter_mut() {
        if let Some(u) = it.qr_code_url.as_ref() {
            let u = u.trim();
            if !u.is_empty() && !u.starts_with("http://") && !u.starts_with("https://") {
                let joined = if u.starts_with('/') {
                    format!("{base_trim}{u}")
                } else {
                    format!("{base_trim}/{u}")
                };
                it.qr_code_url = Some(joined);
            }
        }
    }

    Ok(items)
}

/// 轻量 URL 编码（无需引入 urlencoding crate）
fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}
