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

//! SSE 协议解析器：data: ...\n\n 分隔
//! 作者：JC

use serde::Deserialize;

use super::StreamChunk;
use crate::common::{AppError, AppResult};

#[derive(Debug, Default)]
pub struct SseParser {
    buf: Vec<u8>,
}

#[derive(Debug, Deserialize)]
struct SseDelta {
    choices: Option<Vec<SseChoice>>,
    #[allow(dead_code)]
    error: Option<SseError>,
}

#[derive(Debug, Deserialize)]
struct SseChoice {
    delta: Option<SseDeltaText>,
}

#[derive(Debug, Deserialize)]
struct SseDeltaText {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SseError {
    message: Option<String>,
}

impl super::StreamParser for SseParser {
    fn name(&self) -> &'static str {
        "sse"
    }

    fn feed(&mut self, bytes: &[u8]) -> AppResult<Vec<StreamChunk>> {
        self.buf.extend_from_slice(bytes);
        let mut out = Vec::new();
        loop {
            // 查找 \n\n 分隔
            let sep = self
                .buf
                .windows(2)
                .position(|w| w == b"\n\n")
                .or_else(|| {
                    // 兼容 \r\n\r\n
                    self.buf.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 2)
                });
            let Some(pos) = sep else {
                break;
            };
            let raw = self.buf.drain(..pos + 2).collect::<Vec<_>>();
            let text = String::from_utf8_lossy(&raw).to_string();
            for line in text.lines() {
                let Some(rest) = line.strip_prefix("data:") else {
                    continue;
                };
                let payload = rest.trim();
                if payload.is_empty() {
                    continue;
                }
                if payload == "[DONE]" {
                    out.push(StreamChunk { content: None, done: true });
                    continue;
                }
                let parsed: SseDelta = serde_json::from_str(payload).map_err(|e| {
                    AppError::Ai(format!("解析 SSE chunk 失败: {e}"))
                })?;
                if let Some(choices) = parsed.choices {
                    if let Some(choice) = choices.into_iter().next() {
                        if let Some(delta) = choice.delta {
                            if let Some(content) = delta.content {
                                out.push(StreamChunk {
                                    content: Some(content),
                                    done: false,
                                });
                            }
                        }
                    }
                }
            }
        }
        Ok(out)
    }
}
