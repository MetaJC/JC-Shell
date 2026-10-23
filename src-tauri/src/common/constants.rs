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

//! 全局常量（零魔法值）
//! 作者：JC

/// 应用数据目录名（用户目录下）
pub const APP_DIR_NAME: &str = "JC Shell";

/// SQLite 数据库文件名
pub const DB_FILE_NAME: &str = "jc-shell.db";

/// 配置文件名（YAML）
pub const CONFIG_FILE_NAME: &str = "config.yml";

/// SSH 私钥托管目录名（位于数据目录下，与 DB 同生命周期，卸载保留）
pub const SSH_KEYS_DIR_NAME: &str = "ssh-keys";

/// 默认本地终端 Tab 标题
pub const DEFAULT_LOCAL_TAB_TITLE: &str = "本地终端";

/// AI — 默认 Ollama base url
pub const DEFAULT_OLLAMA_BASE_URL: &str = "http://127.0.0.1:11434/v1";

/// AI — 默认模型名
pub const DEFAULT_AI_MODEL: &str = "qwen3:8b";

/// AI — 流式解析缓冲上限（字节）
pub const AI_STREAM_BUFFER_MAX: usize = 8 * 1024 * 1024; // 8MB

/// SSH — 连接超时毫秒
pub const SSH_CONNECT_TIMEOUT_MS: u64 = 10_000;

/// SSH — 默认端口
pub const SSH_DEFAULT_PORT: u16 = 22;

/// 默认分页：第 1 页
pub const DEFAULT_PAGE_NUM: u32 = 1;

/// 默认分页：每页 20 条（全系统统一，符合后台分页一致规范）
pub const DEFAULT_PAGE_SIZE: u32 = 20;

/// 插件 — 插件子目录名
pub const PLUGIN_DIR_NAME: &str = "plugins";

/// OAuth — 编译期默认后端 server base url
/// 优先级: 运行时 SQLite (oauth_config) > 运行时 YAML (auth) > 编译期环境变量 OPENJC_SERVER_BASE_URL > 常量
///
/// 兜底为空字符串 (非 localhost): 避免用错构建命令 (如直接 `cargo tauri build` 跳过 with-env.mjs)
/// 时仍"假成功"地访问本地 8080. 启动期会强校验, 控空时日志告警 + 请求前早退.
pub const OAUTH_DEFAULT_SERVER_BASE_URL: &str = match option_env!("OPENJC_SERVER_BASE_URL") {
    Some(v) if !v.is_empty() => v,
    _ => "",
};

/// OAuth — 编译期默认客户端标识 (jc-official V14 注册的公开客户端)
pub const OAUTH_CLIENT_ID: &str = match option_env!("OPENJC_CLIENT_ID") {
    Some(v) if !v.is_empty() => v,
    _ => "jc-shell",
};

/// OAuth — 编译期默认授权范围 (AI + 配置同步)
pub const OAUTH_DEFAULT_SCOPES: &str = match option_env!("OPENJC_SCOPES") {
    Some(v) if !v.is_empty() => v,
    _ => "ai,config-sync",
};

/// 应用展示名（品牌化），用于 Rust 侧原生菜单等文案；前端品牌见 src/utils/edition.ts 的 appName()。
/// 具体取值由 `pro-edition` cargo feature 在编译期决定（本工程默认 "JC Shell"）。
pub const APP_NAME: &str = if cfg!(feature = "pro-edition") {
    "JC Shell Pro"
} else {
    "JC Shell"
};

/// AI 网关 — 客户端请求签名密钥 (与服务端 openjc.ai.client-sign-secret 一致)
/// 构建期注入: OPENJC_AI_SIGN_SECRET; 空 = 不发送签名头 (仅开发联调)
pub const AI_CLIENT_SIGN_SECRET: &str = match option_env!("OPENJC_AI_SIGN_SECRET") {
    Some(v) if !v.is_empty() => v,
    _ => "",
};