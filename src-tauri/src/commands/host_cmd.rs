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

//! 主机管理 Tauri Commands（M1 骨架，M2 接入 Repository + KeyChain）
//! 作者：JC

use crate::common::AppResult;
use crate::models::host::{HostUpsertDTO, HostVO};

/// 新增或保存主机
#[tauri::command]
pub async fn upsert_host(_dto: HostUpsertDTO) -> AppResult<HostVO> {
    // M2 调用 HostRepository + KeyStoreService
    Err(crate::common::AppError::NotFound("M2 里程碑实现主机管理".to_string()))
}

/// 列出主机（分页，每页默认 20）
#[tauri::command]
pub async fn list_hosts(
    _page_num: Option<u32>,
    _page_size: Option<u32>,
    _keyword: Option<String>,
    _group: Option<String>,
) -> AppResult<crate::common::Page<HostVO>> {
    Ok(crate::common::Page {
        list: vec![],
        total: 0,
        page_num: _page_num.unwrap_or(1),
        page_size: _page_size.unwrap_or(crate::common::DEFAULT_PAGE_SIZE),
    })
}

/// 删除主机
#[tauri::command]
pub async fn delete_host(_id: String) -> AppResult<bool> {
    Err(crate::common::AppError::NotFound("M2 里程碑实现主机管理".to_string()))
}
