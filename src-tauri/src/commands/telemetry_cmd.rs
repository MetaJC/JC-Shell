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

//! 客户端使用遥测 - Tauri Command 转发
//! 作者：JC
//!
//! 前端在软件启动时调 `telemetry_session_start`，运行期定时调 `telemetry_session_heartbeat`，
//! 退出前调 `telemetry_session_end`。三个命令全部静默：失败只记 warn，不向前端抛错。

use std::sync::Arc;

use tauri::State;

use crate::common::AppResult;
use crate::services::telemetry::{TelemetryService, DEFAULT_HEARTBEAT_SECS};

/// 软件启动：建会话。返回建议心跳间隔（秒）
#[tauri::command]
pub async fn telemetry_session_start(
    telemetry: State<'_, Arc<TelemetryService>>,
    client_id: Option<String>,
    platform: Option<String>,
    os_family: Option<String>,
    os_version: Option<String>,
    app_version: Option<String>,
) -> AppResult<u32> {
    let svc = Arc::clone(&telemetry);
    // 等待 3s 上限：启动路径不能被后端响应拖住
    match tokio::time::timeout(std::time::Duration::from_secs(3), svc.start(client_id, platform, os_family, os_version, app_version)).await {
        Ok(Ok(interval)) => Ok(interval),
        Ok(Err(e)) => {
            tracing::warn!("[telemetry] start 失败（忽略）: {e}");
            Ok(DEFAULT_HEARTBEAT_SECS)
        }
        Err(_) => {
            tracing::warn!("[telemetry] start 超时（忽略）");
            Ok(DEFAULT_HEARTBEAT_SECS)
        }
    }
}

/// 心跳：累加使用时长
#[tauri::command]
pub async fn telemetry_session_heartbeat(telemetry: State<'_, Arc<TelemetryService>>) -> AppResult<()> {
    let svc = Arc::clone(&telemetry);
    svc.heartbeat().await;
    Ok(())
}

/// 软件退出：结束会话（前端在确认退出 / app_exit 前调用）
#[tauri::command]
pub async fn telemetry_session_end(telemetry: State<'_, Arc<TelemetryService>>) -> AppResult<()> {
    let svc = Arc::clone(&telemetry);
    // 退出路径给 2s 窗口，超时直接放行退出
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), svc.end()).await;
    Ok(())
}
