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

//! 终端会话模型
//! 作者：JC

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

/// 会话状态
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TerminalSessionState {
    Disconnected,
    Connecting,
    Connected,
    Error,
}

impl Default for TerminalSessionState {
    fn default() -> Self {
        TerminalSessionState::Disconnected
    }
}

/// 终端会话信息（运行期对象，由 Session Manager 持有）
#[derive(Debug, Clone, Serialize)]
pub struct TerminalSession {
    pub id: String,
    pub host_id: Option<String>,
    pub title: String,
    pub state: TerminalSessionState,
    pub cols: u16,
    pub rows: u16,
    pub created_at: DateTime<Local>,
    pub error: Option<String>,
}

/// 新建会话入参
#[derive(Debug, Clone, Deserialize)]
pub struct CreateSessionDTO {
    pub host_id: Option<String>,
    pub title: Option<String>,
    pub cols: u16,
    pub rows: u16,
}

/// 调整窗口大小入参
#[derive(Debug, Clone, Deserialize)]
pub struct ResizeDTO {
    pub session_id: String,
    pub cols: u16,
    pub rows: u16,
}

/// 会话发送输入
#[derive(Debug, Clone, Deserialize)]
pub struct SessionInputDTO {
    pub session_id: String,
    pub data: String,
}

/// SSH 连接入参
#[derive(Debug, Clone, Deserialize)]
pub struct ConnectSshDTO {
    pub host: String,
    pub port: u16,
    pub username: String,
    /// password / private_key
    pub auth_type: String,
    pub password: Option<String>,
    pub private_key_path: Option<String>,
    pub passphrase: Option<String>,
    pub cols: u16,
    pub rows: u16,
    /// 连接超时（秒，来自设置项；缺省用常量）
    #[serde(default)]
    pub connect_timeout_secs: Option<u64>,
    /// 保活间隔（秒，来自设置项；>0 表示不因空闲断开）
    #[serde(default)]
    pub keep_alive_secs: Option<u64>,
    /// 代理地址 host:port（来自「通用-网络代理」设置；缺省直连）
    #[serde(default)]
    pub proxy_addr: Option<String>,
    /// 代理类型 socks5 / http（缺省 http）
    #[serde(default)]
    pub proxy_kind: Option<String>,
}
