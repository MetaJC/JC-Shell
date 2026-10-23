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

//! 插件管理 Commands（M5 实现）
//! 作者：JC

use crate::common::AppResult;
use crate::models::plugin::PluginManifest;

/// 列出已安装插件
#[tauri::command]
pub async fn list_plugins() -> AppResult<Vec<PluginManifest>> {
    Ok(vec![])
}

/// 启用/禁用插件
#[tauri::command]
pub async fn toggle_plugin(_id: String, _enabled: bool) -> AppResult<bool> {
    Err(crate::common::AppError::NotFound("M5 里程碑实现插件框架".to_string()))
}

/// 卸载插件
#[tauri::command]
pub async fn uninstall_plugin(_id: String) -> AppResult<bool> {
    Err(crate::common::AppError::NotFound("M5 里程碑实现插件框架".to_string()))
}
