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

//! 统一错误类型
//! 作者：JC

use serde::Serialize;
use thiserror::Error;

/// 应用全局错误枚举
#[derive(Debug, Error)]
pub enum AppError {
    #[error("I/O 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("序列化错误: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("数据库错误: {0}")]
    Db(#[from] sqlx::Error),

    #[error("参数校验失败: {0}")]
    Validation(String),

    #[error("未找到资源: {0}")]
    NotFound(String),

    #[error("SSH 错误: {0}")]
    Ssh(String),

    #[error("AI 模型调用错误: {0}")]
    Ai(String),

    #[error("插件错误: {0}")]
    Plugin(String),

    #[error("未授权: {0}")]
    Unauthorized(String),

    #[error("HTTP 错误: {0}")]
    Http(String),

    #[error("操作已取消: {0}")]
    Cancelled(String),

    #[error("内部错误: {0}")]
    Internal(String),
}

/// 序列化友好的错误载荷，返回给前端
#[derive(Serialize)]
pub struct AppErrorPayload {
    pub code: String,
    pub message: String,
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Io(_) => "IO_ERROR",
            AppError::Serde(_) => "SERDE_ERROR",
            AppError::Db(_) => "DB_ERROR",
            AppError::Validation(_) => "VALIDATION_ERROR",
            AppError::NotFound(_) => "NOT_FOUND",
            AppError::Ssh(_) => "SSH_ERROR",
            AppError::Ai(_) => "AI_ERROR",
            AppError::Plugin(_) => "PLUGIN_ERROR",
            AppError::Unauthorized(_) => "UNAUTHORIZED",
            AppError::Http(_) => "HTTP_ERROR",
            AppError::Cancelled(_) => "CANCELLED",
            AppError::Internal(_) => "INTERNAL_ERROR",
        }
    }
}

impl From<&str> for AppError {
    fn from(s: &str) -> Self {
        AppError::Internal(s.to_string())
    }
}

impl From<String> for AppError {
    fn from(s: String) -> Self {
        AppError::Internal(s)
    }
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::Internal(e.to_string())
    }
}

// 让 Tauri 可以把 AppError 作为 command 返回值直接序列化
impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        AppErrorPayload {
            code: self.code().to_string(),
            message: self.to_string(),
        }
        .serialize(serializer)
    }
}
