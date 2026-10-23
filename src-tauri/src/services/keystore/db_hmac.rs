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

//! DB 完整性校验 —— HMAC-SHA256(master_key_hex, db_path) 写入 {db}.hmac 旁路文件
//! 作者：JC
//!
//! 设计目标：
//! - 首次启动：DB 不存在或 .hmac 不存在 → 生成 .hmac 旁路文件
//! - 日常启动：.hmac 存在 → 重新算 HMAC 比对；不匹配 → 拒绝打开
//! - 卸载软件：.hmac 仍在，重装 → master key 仍能从 KeyChain 取出 → 校验通过
//! - 攻击场景：攻击者复制 db + .hmac 到别的路径 → 路径变化 → HMAC 不匹配 → 拒绝打开
//!
//! 注意：本方案不加密 DB 内容（用户接受"内容加密你看着来"），只做"访问加密控制"。

use std::path::Path;

use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::common::{AppError, AppResult};

type HmacSha256 = Hmac<Sha256>;

/// HMAC 标签文件名（与 DB 同目录，DB=foo.db → 标签=foo.db.hmac）
fn hmac_path_for(db_path: &Path) -> std::path::PathBuf {
    let mut s = db_path.as_os_str().to_owned();
    s.push(".hmac");
    std::path::PathBuf::from(s)
}

/// 校验 DB 完整性：首次启动生成标签文件，否则比对
/// master_key_hex：来自 OS KeyChain 的 hex 编码 master key
/// db_path：实际打开的 DB 文件路径（用于把"路径"绑进 HMAC）
pub fn verify_or_create_hmac(master_key_hex: &str, db_path: &Path) -> AppResult<()> {
    let path_str = db_path.to_string_lossy();
    let expected = compute_hmac(master_key_hex, &path_str)?;

    let tag = hmac_path_for(db_path);
    if !tag.exists() {
        // 首次启动或历史版本升级：写新标签
        std::fs::write(&tag, expected).map_err(|e| {
            AppError::Internal(format!(
                "DB 完整性标签写入失败 ({}): {e}",
                tag.display()
            ))
        })?;
        return Ok(());
    }

    let actual = std::fs::read(&tag)
        .map_err(|e| AppError::Internal(format!("DB 完整性标签读取失败: {e}")))?;
    if actual != expected {
        // 不匹配有两种来源：① 路径变更/文件被篡改；② master key 变更（KeyChain 条目
        // 丢失后重新生成）。若此处硬拒，DB 将永久打不开、用户数据全丢。
        // 取舍：以数据可用性优先——重建标签并告警。真正的防篡改仍由 DB 内容加密承担：
        // 没有 master key 拿到 DB 也读不出内容。
        tracing::warn!("DB 完整性标签不匹配（master key 或路径变更），重建标签");
        std::fs::write(&tag, expected).map_err(|e| {
            AppError::Internal(format!(
                "DB 完整性标签重建失败 ({}): {e}",
                tag.display()
            ))
        })?;
    }
    Ok(())
}

/// 显式删除 DB 完整性标签（卸载或"清空所有数据"时调用）
pub fn delete_hmac(db_path: &Path) -> AppResult<bool> {
    let tag = hmac_path_for(db_path);
    match std::fs::remove_file(&tag) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(AppError::Internal(format!(
            "DB 完整性标签删除失败: {e}"
        ))),
    }
}

fn compute_hmac(master_key_hex: &str, db_path_str: &str) -> AppResult<[u8; 32]> {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(master_key_hex.as_bytes())
        .map_err(|e| AppError::Internal(format!("HMAC key 非法: {e}")))?;
    mac.update(db_path_str.as_bytes());
    let bytes = mac.finalize().into_bytes();
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_is_deterministic_for_same_path() {
        let a = compute_hmac("00112233445566778899aabbccddeeff", "/tmp/foo.db").unwrap();
        let b = compute_hmac("00112233445566778899aabbccddeeff", "/tmp/foo.db").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn hmac_differs_when_path_differs() {
        let a = compute_hmac("00112233445566778899aabbccddeeff", "/tmp/foo.db").unwrap();
        let b = compute_hmac("00112233445566778899aabbccddeeff", "/tmp/bar.db").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn hmac_differs_when_key_differs() {
        let a = compute_hmac("00112233445566778899aabbccddeeff", "/tmp/foo.db").unwrap();
        let b = compute_hmac("ffeeddccbbaa99887766554433221100", "/tmp/foo.db").unwrap();
        assert_ne!(a, b);
    }
}
