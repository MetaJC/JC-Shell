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

//! AI 对话历史 Repository
//! 作者：JC

use sqlx::SqlitePool;

use crate::common::{AppResult, Page, PageQuery};
use crate::models::ai::AiChatMessage;

pub struct AiChatRepository<'a> {
    pub pool: &'a SqlitePool,
}

impl<'a> AiChatRepository<'a> {
    pub const fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list_conversations(
        &self,
        _pq: PageQuery,
    ) -> AppResult<Page<(String, Option<String>, chrono::DateTime<chrono::Local>)>> {
        Ok(Page::default())
    }

    pub async fn list_messages(
        &self,
        _conversation_id: &str,
    ) -> AppResult<Vec<AiChatMessage>> {
        Ok(vec![])
    }

    pub async fn append_messages(&self, _msgs: &[AiChatMessage]) -> AppResult<u64> {
        Ok(0)
    }
}
