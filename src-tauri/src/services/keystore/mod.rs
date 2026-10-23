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

//! 密钥安全存储服务 —— 底层用 OS KeyChain，绝不明文落盘
//! 作者：JC

pub mod db_key;
pub mod db_hmac;
pub mod db_crypto;

use crate::common::AppResult;

/// 服务端条目类型
#[derive(Debug, Clone, Copy)]
pub enum KeyScope {
    /// SSH 密码/私钥
    SshHost,
    /// AI 模型 API Key
    AiModel,
    /// 通用
    Generic,
}

#[async_trait::async_trait]
pub trait KeyStoreService: Send + Sync {
    /// 写入一个密钥
    async fn set(&self, scope: KeyScope, key_id: &str, value: &str) -> AppResult<()>;
    /// 读取密钥；不存在返回 None
    async fn get(&self, scope: KeyScope, key_id: &str) -> AppResult<Option<String>>;
    /// 删除密钥
    async fn delete(&self, scope: KeyScope, key_id: &str) -> AppResult<bool>;
}
