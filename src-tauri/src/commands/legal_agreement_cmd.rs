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

//! 法律协议（用户协议 / 隐私政策）Tauri Command
//! 作者：JC
//!
//! - get_legal_agreement：公开接口，按本应用所属项目（PROJECT_CODE）拉取协议链接与版本。
//! - submit_agreement：需登录，将当前会员已同意的版本上报后端落库。

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::common::http;
use crate::common::{AppError, AppResult, OAUTH_DEFAULT_SERVER_BASE_URL};
use crate::services::auth::OAuthClient;
use crate::services::telemetry::PROJECT_CODE;

/// 为「APP 内打开的网页」追加防护放行参数。
///
/// WebView/iframe 无法自定义请求头，带不上 X-JC-Pass 放行头；
/// 这里把等价的放行令牌拼成查询参数（jc_device / jc_pass），
/// 服务端 SecurityGuardFilter 对两者等价校验 —— 保证 APP 内嵌协议页永不 403。
///
/// 另追加 pure=1：文档站（openjc-doc）据此在文档加载前就进入纯净阅读模式，
/// 避免顶栏/面包屑/加载图标先闪现一帧（PC 端走系统浏览器打开原链接，不受影响）。
#[tauri::command]
pub fn legal_page_url(url: String) -> AppResult<String> {
    let sep = if url.contains('?') { '&' } else { '?' };
    let mut extra = String::from("pure=1");
    if let Some(q) = http::guard_pass_query() {
        if !q.is_empty() {
            extra = format!("{extra}&{q}");
        }
    }
    Ok(format!("{url}{sep}{extra}"))
}

/// 与后端 LegalAgreementVO 字段保持一致（协议以链接形式提供）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LegalAgreementConfig {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub user_agreement_url: Option<String>,
    #[serde(default)]
    pub user_agreement_version: Option<String>,
    #[serde(default)]
    pub user_agreement_required_version: Option<String>,
    #[serde(default)]
    pub privacy_policy_url: Option<String>,
    #[serde(default)]
    pub privacy_policy_version: Option<String>,
    #[serde(default)]
    pub privacy_policy_required_version: Option<String>,
}

/// 拉取当前生效的法律协议配置；门禁关闭 / 无配置时返回 None
#[tauri::command]
pub async fn get_legal_agreement(
    oauth: State<'_, Arc<OAuthClient>>,
) -> AppResult<Option<LegalAgreementConfig>> {
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
    let url = format!(
        "{}/api/openjc/client/legal-agreement?projectCode={}",
        base.trim_end_matches('/'),
        PROJECT_CODE
    );
    tracing::info!("[legal-agreement] url={}", url);

    let resp = http::send_with_retry(|| http::client().get(&url).send()).await?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(AppError::Internal(format!(
            "拉取法律协议配置失败 HTTP {status}: {text}"
        )));
    }

    #[derive(Deserialize)]
    struct R<T> {
        code: i32,
        #[serde(default)]
        data: Option<T>,
        #[serde(default)]
        message: Option<String>,
    }
    let parsed: R<LegalAgreementConfig> = serde_json::from_str(&text)
        .map_err(|e| AppError::Internal(format!("法律协议响应解析失败: {e}")))?;
    if parsed.code != 0 {
        return Err(AppError::Internal(
            parsed.message.unwrap_or_else(|| format!("业务失败 code={}", parsed.code)),
        ));
    }
    Ok(parsed.data)
}

/// 客户端提交同意：将当前会员已同意的版本上报后端（需携带登录态 Bearer Token）
#[tauri::command]
pub async fn submit_agreement(
    oauth: State<'_, Arc<OAuthClient>>,
    user_agreement_version: String,
    privacy_policy_version: String,
) -> AppResult<()> {
    let token = oauth.ensure_access_token().await?;
    let cfg = oauth.cfg_snapshot().await;
    let base = if cfg.server_base_url.trim().is_empty() {
        OAUTH_DEFAULT_SERVER_BASE_URL.to_string()
    } else {
        cfg.server_base_url
    };
    let url = format!(
        "{}/api/openjc/client/agree",
        base.trim_end_matches('/')
    );

    let body = serde_json::json!({
        "userAgreementVersion": user_agreement_version,
        "privacyPolicyVersion": privacy_policy_version,
    });

    let resp = http::send_with_retry(|| {
        http::client()
            .post(&url)
            .bearer_auth(&token)
            .json(&body)
            .send()
    })
    .await?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(AppError::Internal(format!(
            "提交同意记录失败 HTTP {status}: {text}"
        )));
    }
    Ok(())
}
