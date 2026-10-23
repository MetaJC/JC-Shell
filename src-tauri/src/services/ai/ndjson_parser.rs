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

//! NDJSON 解析器（Ollama 原生 /api/chat 使用）
//! 作者：JC

use serde::Deserialize;

use super::StreamChunk;
use crate::common::{AppError, AppResult};

#[derive(Debug, Default)]
pub struct NdJsonParser {
    buf: Vec<u8>,
}

#[derive(Debug, Deserialize)]
struct OllamaChunk {
    message: Option<OllamaMessage>,
    done: Option<bool>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OllamaMessage {
    content: Option<String>,
}

impl super::StreamParser for NdJsonParser {
    fn name(&self) -> &'static str {
        "ndjson"
    }

    fn feed(&mut self, bytes: &[u8]) -> AppResult<Vec<StreamChunk>> {
        self.buf.extend_from_slice(bytes);
        let mut out = Vec::new();
        loop {
            let Some(pos) = self.buf.iter().position(|&b| b == b'\n') else {
                break;
            };
            let line = self.buf.drain(..pos + 1).collect::<Vec<_>>();
            let trimmed: Vec<u8> = line.into_iter().take_while(|&b| b != b'\r' && b != b'\n').collect();
            if trimmed.is_empty() {
                continue;
            }
            let s = String::from_utf8_lossy(&trimmed);
            let chunk: OllamaChunk = serde_json::from_str(&s).map_err(|e| {
                AppError::Ai(format!("解析 NDJSON chunk 失败: {e}; 内容={s}"))
            })?;
            if let Some(err) = chunk.error {
                return Err(AppError::Ai(format!("模型返回错误: {err}")));
            }
            let mut content = None;
            if let Some(msg) = chunk.message {
                content = msg.content;
            }
            out.push(StreamChunk {
                content,
                done: chunk.done.unwrap_or(false),
            });
        }
        Ok(out)
    }
}
