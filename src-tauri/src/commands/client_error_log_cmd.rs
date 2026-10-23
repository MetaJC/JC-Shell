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

//! 客户端错误日志 - Tauri Command 转发
//! 作者：JC
//!
//! 前端 `errorReport.ts` 把运行时错误收集成 `Vec<ReportClientErrorReq>`，
//! 调此 command → Rust 端通过共享 HTTP 客户端转发到主站
//! `POST /api/openjc/client/error-logs/report-batch`。
//! 出错不影响主流程（已被调用方 try/catch 兜住），仅记 warn 日志。

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::common::http;
use crate::common::{AppError, AppResult, OAUTH_DEFAULT_SERVER_BASE_URL};
use crate::services::auth::OAuthClient;

/// 与主站 `ReportClientErrorReq` 字段对齐（camelCase）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportClientErrorReq {
    #[serde(default)]
    pub client_type: Option<String>,
    #[serde(default)]
    pub client_version: Option<String>,
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub os_version: Option<String>,
    #[serde(default)]
    pub user_id: Option<i64>,
    #[serde(default)]
    pub username: Option<String>,
    /// 必填
    pub error_type: String,
    #[serde(default)]
    pub error_code: Option<String>,
    #[serde(default)]
    pub error_message: Option<String>,
    #[serde(default)]
    pub error_detail: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub extra: Option<String>,
    #[serde(default)]
    pub client_time: Option<String>,
}

/// 客户端错误日志上报（静默；失败仅 warn，不向调用方抛错）
#[tauri::command]
pub async fn report_client_errors(
    oauth: State<'_, Arc<OAuthClient>>,
    reqs: Vec<ReportClientErrorReq>,
) -> AppResult<()> {
    if reqs.is_empty() {
        return Ok(());
    }
    let cfg = oauth.cfg_snapshot().await;
    let base = if cfg.server_base_url.trim().is_empty() {
        if OAUTH_DEFAULT_SERVER_BASE_URL.trim().is_empty() {
            tracing::warn!("[error-log] baseUrl 未配置，上报丢弃");
            return Ok(());
        }
        OAUTH_DEFAULT_SERVER_BASE_URL.to_string()
    } else {
        cfg.server_base_url
    };
    let url = format!(
        "{}/api/openjc/client/error-logs/report-batch",
        base.trim_end_matches('/')
    );
    let resp = match http::send_with_retry(|| http::client().post(&url).json(&reqs).send()).await {
        Ok(r) => r,
        Err(e) => {
            // 上报失败不能让主流程感知：吞掉错误 + warn
            tracing::warn!("[error-log] 上报失败（已吞掉，不影响主流程）: {e}");
            return Ok(());
        }
    };
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        tracing::warn!("[error-log] 上报失败 HTTP {}: {}", status, text);
        return Ok(());
    }
    tracing::info!("[error-log] 已上报 {} 条客户端错误", reqs.len());
    Ok(())
}

/// 仅用于让编译器不报 unused warning（本模块目前只有一个命令，但保留扩展空间）
#[allow(dead_code)]
fn _assert_types(_a: AppError, _r: ReportClientErrorReq) {}