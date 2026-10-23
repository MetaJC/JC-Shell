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

//! 插件元数据 Repository
//! 作者：JC

use sqlx::SqlitePool;

use crate::common::AppResult;
use crate::models::plugin::PluginManifest;

pub struct PluginRepository<'a> {
    pub pool: &'a SqlitePool,
}

impl<'a> PluginRepository<'a> {
    pub const fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list(&self) -> AppResult<Vec<PluginManifest>> {
        Ok(vec![])
    }

    pub async fn upsert(&self, _m: &PluginManifest) -> AppResult<u64> {
        Ok(0)
    }

    pub async fn delete(&self, _id: &str) -> AppResult<bool> {
        Ok(false)
    }
}
