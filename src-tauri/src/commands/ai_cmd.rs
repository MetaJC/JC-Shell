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

//! AI 能力 Commands（M1 骨架，M3 接入真实模型调用）
//! 作者：JC

use crate::common::AppResult;
use crate::models::ai::{AiChatRequestDTO, AiModelConfig, AiModelConfigUpsertDTO};

/// 发起聊天（支持流式，通过 Tauri event 向前端推送 chunk）
#[tauri::command]
pub async fn ai_chat(
    _dto: AiChatRequestDTO,
    _app: tauri::AppHandle,
) -> AppResult<String> {
    Err(crate::common::AppError::NotFound("M3 里程碑实现 AI 模型调用".to_string()))
}

/// 中止 Agent 在途请求（前端 Ctrl+C 调用）
#[tauri::command]
pub async fn ai_abort(conversation_id: String) -> AppResult<bool> {
    Ok(crate::commands::ai_agent_cmd::abort_agent_request(&conversation_id))
}

/// 列出模型配置
#[tauri::command]
pub async fn list_model_configs() -> AppResult<Vec<AiModelConfig>> {
    Ok(vec![])
}

/// 新增/更新模型配置
#[tauri::command]
pub async fn upsert_model_config(_dto: AiModelConfigUpsertDTO) -> AppResult<AiModelConfig> {
    Err(crate::common::AppError::NotFound("M3 里程碑实现 AI 模型管理".to_string()))
}

/// 删除模型配置
#[tauri::command]
pub async fn delete_model_config(_id: String) -> AppResult<bool> {
    Ok(false)
}

/// 测试指定模型配置的连通性
#[tauri::command]
pub async fn test_model_config(_id: String) -> AppResult<bool> {
    Err(crate::common::AppError::NotFound("M3 里程碑实现 AI 连通性测试".to_string()))
}
