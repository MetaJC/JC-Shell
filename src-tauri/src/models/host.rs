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

//! 主机实体（SSH 连接配置）
//! 作者：JC

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::common::SSH_DEFAULT_PORT;

/// 认证方式
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HostAuthType {
    /// 密码认证
    Password,
    /// 私钥认证
    PrivateKey,
}

impl Default for HostAuthType {
    fn default() -> Self {
        HostAuthType::Password
    }
}

/// 主机 DO（数据库映射）
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct HostDO {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: String, // DB 用字符串存储：password / private_key
    /// 私钥路径或引用 ID（密钥值本身存入 KeyChain）
    pub key_ref: Option<String>,
    pub jump_host_id: Option<String>,
    pub remark: Option<String>,
    pub group_name: Option<String>,
    pub created_at: DateTime<Local>,
    pub updated_at: DateTime<Local>,
}

/// 主机 DTO —— 新增/编辑入参
#[derive(Debug, Clone, Deserialize)]
pub struct HostUpsertDTO {
    pub id: Option<String>,
    pub name: String,
    pub host: String,
    #[serde(default = "default_port")]
    pub port: Option<u16>,
    pub username: String,
    pub auth_type: HostAuthType,
    /// 密码或私钥内容，创建/更新时一次性传入，实际落 KeyChain
    pub secret: Option<String>,
    pub key_path: Option<String>,
    pub jump_host_id: Option<String>,
    pub remark: Option<String>,
    pub group_name: Option<String>,
}

fn default_port() -> Option<u16> {
    Some(SSH_DEFAULT_PORT)
}

/// 主机 VO —— 返回列表
#[derive(Debug, Clone, Serialize)]
pub struct HostVO {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: HostAuthType,
    pub jump_host_id: Option<String>,
    pub remark: Option<String>,
    pub group_name: Option<String>,
    pub created_at: DateTime<Local>,
    pub updated_at: DateTime<Local>,
}
