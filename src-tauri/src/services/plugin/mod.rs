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

//! 插件加载器抽象（M5 实现 WASM / Native）
//! 作者：JC

use crate::common::AppResult;
use crate::models::plugin::{PluginHook, PluginManifest};

/// 插件实例接口
#[async_trait::async_trait]
pub trait PluginInstance: Send + Sync {
    fn manifest(&self) -> &PluginManifest;
    async fn invoke(&self, cmd: &str, payload: serde_json::Value) -> AppResult<serde_json::Value>;
    async fn on_hook(&self, hook: PluginHook, ctx: serde_json::Value) -> AppResult<serde_json::Value>;
}

/// 插件管理器
#[async_trait::async_trait]
pub trait PluginManager: Send + Sync {
    async fn list(&self) -> AppResult<Vec<PluginManifest>>;
    async fn load_all(&self) -> AppResult<usize>;
    async fn enable(&self, id: &str, enabled: bool) -> AppResult<bool>;
    async fn uninstall(&self, id: &str) -> AppResult<bool>;
}
