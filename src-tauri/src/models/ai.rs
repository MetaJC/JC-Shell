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

//! AI 模型配置与消息模型
//! 作者：JC

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

/// 模型提供方
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AiProvider {
    Ollama,
    OpenAICompat,
}

impl Default for AiProvider {
    fn default() -> Self {
        AiProvider::Ollama
    }
}

/// 模型配置（持久化）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiModelConfig {
    pub id: String,
    pub name: String,
    pub provider: AiProvider,
    pub base_url: String,
    pub model: String,
    /// api_key 的 KeyChain 引用；真实值不落盘
    pub api_key_ref: Option<String>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub is_default: bool,
    pub created_at: DateTime<Local>,
    pub updated_at: DateTime<Local>,
}

/// 新增/修改模型配置
#[derive(Debug, Clone, Deserialize)]
pub struct AiModelConfigUpsertDTO {
    pub id: Option<String>,
    pub name: String,
    pub provider: AiProvider,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub is_default: bool,
}

/// 聊天消息角色
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AiChatRole {
    System,
    User,
    Assistant,
}

/// 单条消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiChatMessage {
    pub id: String,
    pub conversation_id: String,
    pub role: AiChatRole,
    pub content: String,
    pub created_at: DateTime<Local>,
}

/// 发起聊天请求入参
#[derive(Debug, Clone, Deserialize)]
pub struct AiChatRequestDTO {
    pub model_config_id: Option<String>,
    pub conversation_id: Option<String>,
    pub user_input: String,
    /// 附加上下文（如当前选中终端的最近 N 行输出，AI 据此解释错误）
    pub context: Option<Vec<String>>,
    pub stream: bool,
}
