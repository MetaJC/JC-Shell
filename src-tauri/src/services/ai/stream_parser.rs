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

//! 流式协议解析器抽象（SSE / NDJSON）—— 对应经验教训：必须可切换
//! 作者：JC

use crate::common::AppResult;

/// 解析后的单个事件
#[derive(Debug, Clone)]
pub struct StreamChunk {
    /// 有效负载内容
    pub content: Option<String>,
    /// 若为 true 表示本次流结束
    pub done: bool,
}

/// 流式解析器统一接口
pub trait StreamParser: Send + Sync {
    /// 名字：sse / ndjson
    fn name(&self) -> &'static str;

    /// 喂入新字节数据，返回解析出的所有 chunk
    fn feed(&mut self, bytes: &[u8]) -> AppResult<Vec<StreamChunk>>;

    /// 数据流结束后的最后一次 flush（处理 buffer 尾部）
    fn flush(&mut self) -> AppResult<Vec<StreamChunk>> {
        Ok(vec![])
    }
}

mod sse_parser;
mod ndjson_parser;

pub use sse_parser::SseParser;
pub use ndjson_parser::NdJsonParser;
