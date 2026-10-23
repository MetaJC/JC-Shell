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

//! DB master key 管理 —— 32 字节随机 key，主存 OS KeyChain，旁路备份存数据目录文件
//! 作者：JC
//!
//! 设计：
//! - 首次启动随机生成 256-bit master key
//! - 主存：KeyChain（hex 编码）；旁路备份：数据目录下的 key 文件（base64 hex）
//!   —— 实测 Windows 凭据管理器条目可能因系统清理/策略丢失，仅存 KeyChain 时
//!   key 一丢全库密文永久不可读（表现为重装后“会话丢失”），故必须有文件兜底。
//!   key 文件与 DB 同目录：卸载保留、备份随迁、与 DB 同生命周期。
//! - 卸载软件不影响 KeyChain 条目，重装仍可解锁 DB

use std::path::Path;

use rand::RngCore;

use crate::common::{AppError, AppResult};

/// KeyChain service 名（区分于其他 keyring 条目）
#[cfg(desktop)]
const KEYRING_SERVICE: &str = "jc-shell";
/// KeyChain account 名（DB master key）
#[cfg(desktop)]
const KEYRING_DB_MASTER_KEY: &str = "db-master-key";
/// master key 字节数（AES-256）
const MASTER_KEY_BYTES: usize = 32;

/// 读取 DB master key，不存在则生成。
/// 查找顺序：KeyChain → 备份文件 → 新生成（同时写两处）；
/// 发现单侧缺失时自动补齐另一侧。
pub fn get_or_create_db_master_key(key_file: &Path) -> AppResult<String> {
    match read_db_master_key()? {
        Some(hex) => {
            // KeyChain 有：确保文件备份存在（首次升级的老安装没有备份文件）
            if read_key_file(key_file).ok().flatten().is_none() {
                let _ = write_key_file(key_file, &hex);
            }
            Ok(hex)
        }
        None => match read_key_file(key_file)? {
            // KeyChain 条目丢失（凭据管理器被清理）：从文件备份恢复并回写 KeyChain
            Some(hex) => {
                let _ = store_key_to_keychain(&hex);
                Ok(hex)
            }
            None => {
                let hex = generate_key_hex();
                store_key_to_keychain(&hex)?;
                write_key_file(key_file, &hex)?;
                Ok(hex)
            }
        },
    }
}

/// 显式删除 DB master key（卸载或用户主动"清空所有数据"时调用）
/// 注意：删除后 DB 永远无法打开
pub fn delete_db_master_key() -> AppResult<bool> {
    #[cfg(desktop)]
    {
        let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_DB_MASTER_KEY)
            .map_err(|e| AppError::Internal(format!("KeyChain 初始化失败: {e}")))?;
        match entry.delete_credential() {
            Ok(()) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(e) => Err(AppError::Internal(format!(
                "KeyChain 删除 db-master-key 失败: {e}"
            ))),
        }
    }
    #[cfg(mobile)]
    {
        // 移动端无 OS KeyChain，此条目不持久化，视为无条目
        Ok(false)
    }
}

fn read_db_master_key() -> AppResult<Option<String>> {
    #[cfg(desktop)]
    {
        let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_DB_MASTER_KEY)
            .map_err(|e| AppError::Internal(format!("KeyChain 初始化失败: {e}")))?;
        match entry.get_password() {
            Ok(hex) => {
                validate_hex_key(&hex)?;
                Ok(Some(hex))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(AppError::Internal(format!(
                "KeyChain 读取 db-master-key 失败: {e}"
            ))),
        }
    }
    #[cfg(mobile)]
    {
        Ok(None)
    }
}

fn generate_key_hex() -> String {
    let mut buf = [0u8; MASTER_KEY_BYTES];
    rand::thread_rng().fill_bytes(&mut buf);
    hex_encode(&buf)
}

fn store_key_to_keychain(hex: &str) -> AppResult<()> {
    #[cfg(desktop)]
    {
        let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_DB_MASTER_KEY)
            .map_err(|e| AppError::Internal(format!("KeyChain 初始化失败: {e}")))?;
        entry
            .set_password(hex)
            .map_err(|e| AppError::Internal(format!("KeyChain 写入 db-master-key 失败: {e}")))?;
    }
    let _ = hex;
    Ok(())
}

// ===== 文件备份（与 DB 同目录，卸载不清除，随数据备份迁移）=====
// 存 base64(hex) 而非裸 hex，避免被文本工具误当普通配置读出；
// Windows 下追加隐藏+系统属性，资源管理器默认不可见。

fn read_key_file(key_file: &Path) -> AppResult<Option<String>> {
    let Ok(content) = std::fs::read_to_string(key_file) else {
        return Ok(None);
    };
    use base64::Engine;
    let Ok(raw) = base64::engine::general_purpose::STANDARD.decode(content.trim()) else {
        return Ok(None);
    };
    let hex = String::from_utf8_lossy(&raw).to_string();
    if validate_hex_key(&hex).is_err() {
        return Ok(None);
    }
    Ok(Some(hex))
}

fn write_key_file(key_file: &Path, hex: &str) -> AppResult<()> {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(hex.as_bytes());
    std::fs::write(key_file, b64)?;
    hide_key_file(key_file);
    Ok(())
}

#[cfg(windows)]
fn hide_key_file(key_file: &Path) {
    use std::os::windows::ffi::OsStrExt;
    extern "system" {
        fn SetFileAttributesW(lpFileName: *const u16, dwFileAttributes: u32) -> i32;
    }
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
    let mut wide: Vec<u16> = key_file.as_os_str().encode_wide().collect();
    wide.push(0);
    unsafe {
        SetFileAttributesW(wide.as_ptr(), FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM);
    }
}

#[cfg(not(windows))]
fn hide_key_file(_key_file: &Path) {}

fn validate_hex_key(hex: &str) -> AppResult<()> {
    if hex.len() != MASTER_KEY_BYTES * 2 {
        return Err(AppError::Internal(format!(
            "db-master-key 长度异常: {} (期望 {})",
            hex.len(),
            MASTER_KEY_BYTES * 2
        )));
    }
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::Internal("db-master-key 含非 hex 字符".into()));
    }
    Ok(())
}

fn hex_encode(bytes: &[u8]) -> String {
    const LUT: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(LUT[(b >> 4) as usize] as char);
        s.push(LUT[(b & 0x0f) as usize] as char);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_encode_roundtrip() {
        let bytes = [0u8, 1, 15, 16, 255];
        assert_eq!(hex_encode(&bytes), "00010f10ff");
    }

    #[test]
    fn validate_hex_key_rejects_bad_length() {
        assert!(validate_hex_key("abcd").is_err());
        assert!(validate_hex_key(&"a".repeat(64)).is_ok());
    }
}
