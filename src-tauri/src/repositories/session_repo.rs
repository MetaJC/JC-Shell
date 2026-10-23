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

//! 终端会话历史（持久化操作记录）
//! 作者：JC

use sqlx::SqlitePool;

use crate::common::{AppResult, Page, PageQuery};

pub struct SessionHistoryRecord {
    pub id: String,
    pub session_id: String,
    pub host_id: Option<String>,
    pub input: String,
    pub output_tail: Option<String>,
    pub created_at: chrono::DateTime<chrono::Local>,
}

pub struct SessionRepository<'a> {
    pub pool: &'a SqlitePool,
}

impl<'a> SessionRepository<'a> {
    pub const fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn append(&self, _rec: &SessionHistoryRecord) -> AppResult<u64> {
        Ok(0)
    }

    pub async fn list(
        &self,
        _pq: PageQuery,
        _session_id: Option<&str>,
    ) -> AppResult<Page<SessionHistoryRecord>> {
        Ok(Page::default())
    }
}
