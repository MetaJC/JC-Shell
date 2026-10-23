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

//! AI Service Trait —— 可切换 Ollama / OpenAI 兼容实现
//! 作者：JC

use crate::common::AppResult;
use crate::models::ai::{AiChatRequestDTO, AiModelConfig};

/// 流式输出事件
#[derive(Debug, Clone)]
pub enum AiStreamEvent {
    Chunk(String),
    Done(String),
    Error(String),
}

/// AI 服务抽象
#[async_trait::async_trait]
pub trait AiService: Send + Sync {
    /// 非流式对话
    async fn chat(&self, config: &AiModelConfig, req: &AiChatRequestDTO) -> AppResult<String>;

    /// 流式对话（回调推 chunk）
    async fn chat_stream<F>(
        &self,
        config: &AiModelConfig,
        req: &AiChatRequestDTO,
        on_event: F,
    ) -> AppResult<()>
    where
        F: Fn(AiStreamEvent) + Send + 'static;

    /// 连通性测试（短超时，发个最小请求）
    async fn test(&self, config: &AiModelConfig) -> AppResult<bool>;
}
