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

//! 客户端启动弹窗 Tauri Command
//! 作者：JC

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::common::http;
use crate::common::{AppError, AppResult, OAUTH_DEFAULT_SERVER_BASE_URL};
use crate::services::auth::OAuthClient;

/// 客户端启动弹窗（与后端 LaunchDialogVO 字段保持一致）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LaunchDialogInfo {
    pub id: Option<i64>,
    pub title: Option<String>,
    /// 渲染后 HTML（后端由管理端 markdown-it 渲染结果落库），客户端直接展示
    #[serde(default)]
    pub content_html: Option<String>,
    /// OFF=不弹窗 EVERY_STARTUP=每次启动 FIRST_ONLY=仅首次启动
    #[serde(default)]
    pub frequency: Option<String>,
    /// LARGE=大弹窗 SMALL=小弹窗(头像上方)
    #[serde(default)]
    pub dialog_size: Option<String>,
    /// 内容更新时间（字符串），客户端据此判断是否为新弹窗
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// 拉取当前生效的启动弹窗；未启用 / 无内容时返回 None
#[tauri::command]
pub async fn get_launch_dialog(
    oauth: State<'_, Arc<OAuthClient>>,
) -> AppResult<Option<LaunchDialogInfo>> {
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
        "{}/api/openjc/client/launch-dialog",
        base.trim_end_matches('/')
    );
    tracing::info!("[launch-dialog] url={}", url);

    // 复用共享 HTTP 客户端（tcp_keepalive + 网络错误自动重试一次），与 QQ 群拉取保持一致
    let resp = http::send_with_retry(|| http::client().get(&url).send()).await?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(AppError::Internal(format!(
            "拉取启动弹窗失败 HTTP {status}: {text}"
        )));
    }

    // 后端统一响应体 R<T>：{ code, data, message }
    #[derive(Deserialize)]
    struct R<T> {
        code: i32,
        #[serde(default)]
        data: Option<T>,
        #[serde(default)]
        message: Option<String>,
    }
    let parsed: R<LaunchDialogInfo> = serde_json::from_str(&text)
        .map_err(|e| AppError::Internal(format!("启动弹窗响应解析失败: {e}")))?;
    if parsed.code != 0 {
        return Err(AppError::Internal(
            parsed.message.unwrap_or_else(|| format!("业务失败 code={}", parsed.code)),
        ));
    }
    Ok(parsed.data)
}
