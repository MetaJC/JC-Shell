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

//! 应用设置数据模型（通用 key/value）
//! 作者：JC

use serde::{Deserialize, Serialize};

/// app_setting 表对应实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingDO {
    /// 业务键，如 ui_layout / theme 等
    pub key: String,
    /// JSON 字符串值
    pub value: String,
    /// ISO8601 时间戳
    pub updated_at: String,
}
