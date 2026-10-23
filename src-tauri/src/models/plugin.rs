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

//! 插件元数据模型
//! 作者：JC

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

/// 插件形态（预留）
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginKind {
    /// WebAssembly 插件
    Wasm,
    /// 原生动态库
    Native,
}

/// 插件钩子点
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginHook {
    /// 终端数据输入前
    TerminalBeforeInput,
    /// 终端数据输出后
    TerminalAfterOutput,
    /// AI 对话请求前
    AiBeforeRequest,
    /// AI 对话响应后
    AiAfterResponse,
}

/// 插件元数据（manifest）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub kind: PluginKind,
    pub description: Option<String>,
    pub entry: String,
    pub permissions: Vec<String>,
    pub hooks: Vec<PluginHook>,
    pub installed_at: DateTime<Local>,
    pub enabled: bool,
}
