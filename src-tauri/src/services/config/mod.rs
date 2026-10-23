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

//! 应用配置服务（读写 YAML）
//! 作者：JC

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::common::{AppError, AppResult, CONFIG_FILE_NAME};

/// 根配置结构（YAML 映射）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub general: GeneralConfig,
    pub terminal: TerminalConfig,
    pub ai: AiFeatureConfig,
    /// 登录认证相关配置 (非空字段会覆盖编译常量)
    #[serde(default)]
    pub auth: AuthConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub theme: String,
    pub language: String,
    pub auto_update: bool,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            language: "zh-CN".into(),
            auto_update: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalConfig {
    pub font_family: String,
    pub font_size: u16,
    pub line_height: f32,
    pub default_shell: String,
    pub scrollback_lines: u32,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            font_family: "Consolas, Cascadia Code, Menlo, monospace".into(),
            font_size: 14,
            line_height: 1.2,
            default_shell: "".into(),
            scrollback_lines: 10_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiFeatureConfig {
    pub default_model_config_id: Option<String>,
    pub auto_summarize_session: bool,
    pub max_context_lines: u32,
}

impl Default for AiFeatureConfig {
    fn default() -> Self {
        Self {
            default_model_config_id: None,
            auto_summarize_session: false,
            max_context_lines: 200,
        }
    }
}

/// 登录认证配置（YAML 里的 auth 节点）
/// 字段非空才会覆盖默认值, 方便只需要改 server_base_url 时不必写全部内容
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuthConfig {
    /// jc-official 后端地址, 例如 https://openjc.cc 或 http://localhost:8080
    pub server_base_url: Option<String>,
    /// OAuth client_id (公开客户端, 固定 jc-shell, 特殊部署可覆盖)
    pub client_id: Option<String>,
    /// OAuth 授权范围 逗号分隔
    pub scopes: Option<String>,
}

/// 配置服务抽象
#[async_trait::async_trait]
pub trait ConfigService: Send + Sync {
    async fn load(&self) -> AppResult<AppConfig>;
    async fn save(&self, cfg: &AppConfig) -> AppResult<()>;
}

/// 基于 YAML 文件的配置服务实现
/// 文件路径: %AppData%/JC Shell/config.yml（数据目录，与安装目录隔离）；
/// 旧位置（安装目录内）存在时兼容回退读取（迁移前的历史安装）
pub struct YamlConfigService {
    path: PathBuf,
}

impl YamlConfigService {
    pub fn new() -> Self {
        let primary = crate::common::paths::app_data_dir().join(CONFIG_FILE_NAME);
        // 新位置还没有（且存量迁移未触发）时回退读旧安装目录内的配置，避免用户配置丢失；
        // 写入仍落到新位置（save 前先建目录）
        let path = if primary.exists() {
            primary
        } else {
            let legacy = crate::common::paths::legacy_data_dir().join(CONFIG_FILE_NAME);
            if legacy.exists() { legacy } else { primary }
        };
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Default for YamlConfigService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ConfigService for YamlConfigService {
    async fn load(&self) -> AppResult<AppConfig> {
        let p = self.path.clone();
        let cfg = tokio::task::spawn_blocking(move || -> AppResult<AppConfig> {
            if !p.exists() {
                return Ok(AppConfig::default());
            }
            let text = std::fs::read_to_string(&p)
                .map_err(|e| AppError::Internal(format!("读取 config.yml 失败: {e}")))?;
            if text.trim().is_empty() {
                return Ok(AppConfig::default());
            }
            serde_yaml::from_str::<AppConfig>(&text).map_err(|e| {
                AppError::Internal(format!("config.yml 解析失败: {e}"))
            })
        }).await.map_err(|e| AppError::Internal(format!("配置加载任务失败: {e}")))??;
        Ok(cfg)
    }

    async fn save(&self, cfg: &AppConfig) -> AppResult<()> {
        let p = self.path.clone();
        let yaml = serde_yaml::to_string(cfg)
            .map_err(|e| AppError::Internal(format!("config.yml 序列化失败: {e}")))?;
        tokio::task::spawn_blocking(move || -> AppResult<()> {
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            std::fs::write(&p, yaml)
                .map_err(|e| AppError::Internal(format!("写入 config.yml 失败: {e}")))?;
            Ok(())
        }).await.map_err(|e| AppError::Internal(format!("配置保存任务失败: {e}")))??;
        Ok(())
    }
}