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

//! 应用设置数据访问（app_setting 表 CRUD）
//! 作者：JC
//! value 落盘前透明加密（AES-256-GCM，密钥在 OS KeyChain），
//! 读取时自动解密；历史明文值兼容直读，并由 init_db 一次性补加密。

use chrono::Utc;
use sqlx::{Row, SqlitePool};

use crate::common::AppResult;
use crate::models::setting::SettingDO;
use crate::services::keystore::db_crypto;

/// 应用设置 Repository
pub struct SettingRepository<'a> {
    pub pool: &'a SqlitePool,
}

impl<'a> SettingRepository<'a> {
    pub const fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// 读取单条（密文自动解密；历史明文兼容直读）
    pub async fn get(&self, key: &str) -> AppResult<Option<SettingDO>> {
        let row = sqlx::query("SELECT key, value, updated_at FROM app_setting WHERE key = ?")
            .bind(key)
            .fetch_optional(self.pool)
            .await?;
        match row {
            None => Ok(None),
            Some(r) => {
                let raw: String = r.get("value");
                let value = match db_crypto::decrypt_value(&raw)? {
                    Some(plain) => plain,
                    None => raw,
                };
                Ok(Some(SettingDO {
                    key: r.get("key"),
                    value,
                    updated_at: r.get("updated_at"),
                }))
            }
        }
    }
    
    /// 新增或更新（UPSERT；落盘前加密）
    pub async fn upsert(&self, key: &str, value: &str) -> AppResult<()> {
        let now = Utc::now().to_rfc3339();
        let stored = db_crypto::encrypt_value(value)?;
        sqlx::query(
            r#"INSERT INTO app_setting (key, value, updated_at) VALUES (?, ?, ?)
               ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at"#,
        )
        .bind(key)
        .bind(&stored)
        .bind(&now)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// 删除
    pub async fn delete(&self, key: &str) -> AppResult<bool> {
        let res = sqlx::query("DELETE FROM app_setting WHERE key = ?")
            .bind(key)
            .execute(self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    /// 全量列表（数据备份导出用；导出为解密后的明文）
    pub async fn list_all(&self) -> AppResult<Vec<SettingDO>> {
        let rows = sqlx::query("SELECT key, value, updated_at FROM app_setting ORDER BY key")
            .fetch_all(self.pool)
            .await?;
        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let raw: String = r.get("value");
            // 解密失败不阻断导出，保留原值（理论上 init_db 已全量补加密，不应出现）
            let value = db_crypto::decrypt_value(&raw).ok().flatten().unwrap_or(raw);
            out.push(SettingDO {
                key: r.get("key"),
                value,
                updated_at: r.get("updated_at"),
            });
        }
        Ok(out)
    }
}
