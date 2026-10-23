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

//! 主机数据访问（M2 实现 sqlx 读写）
//! 作者：JC

use sqlx::SqlitePool;

use crate::common::{AppResult, Page, PageQuery};
use crate::models::host::{HostDO, HostVO};

/// 主机 Repository
pub struct HostRepository<'a> {
    pub pool: &'a SqlitePool,
}

impl<'a> HostRepository<'a> {
    pub const fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// 分页查询
    pub async fn page(
        &self,
        _pq: PageQuery,
        _keyword: Option<&str>,
        _group: Option<&str>,
    ) -> AppResult<Page<HostVO>> {
        Ok(Page::default())
    }

    pub async fn get(&self, _id: &str) -> AppResult<Option<HostDO>> {
        Ok(None)
    }

    pub async fn insert(&self, _do_: &HostDO) -> AppResult<u64> {
        Ok(0)
    }

    pub async fn update(&self, _do_: &HostDO) -> AppResult<bool> {
        Ok(false)
    }

    pub async fn delete(&self, _id: &str) -> AppResult<bool> {
        Ok(false)
    }
}
