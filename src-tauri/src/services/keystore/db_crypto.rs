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

//! app_setting 值透明加密 —— AES-256-GCM，密钥 = OS KeyChain 中的 DB master key
//! 作者：JC
//!
//! 背景：DB 文件是明文 SQLite，用 Navicat 等任意工具可直接读出全部用户数据。
//! 本模块在 repo 层对 app_setting.value 做透明加密：落盘为密文，仅持有
//! master key（KeyChain 保护）的本软件可解密。
//!
//! 安全边界：可阻断第三方工具/离线拷贝的直接读取；同用户同机的恶意程序
//! 理论上可读 KeyChain，故定位为"访问控制"而非绝对机密（与 HMAC 校验互补）。
//!
//! 密文格式：JCENC:v1:<base64(nonce(12B) || ciphertext || tag(16B))>

use aes_gcm::{aead::Aead, Aes256Gcm, Key, KeyInit, Nonce};
use once_cell::sync::OnceCell;
use rand::RngCore;

use crate::common::{AppError, AppResult};

/// 密文前缀（含版本号，便于未来算法升级时区分）
const PREFIX: &str = "JCENC:v1:";
/// AES-GCM nonce 字节数
const NONCE_BYTES: usize = 12;
/// master key 字节数（AES-256）
const KEY_BYTES: usize = 32;

/// 进程内 master key（init_db 时注册一次，repo 层加解密使用）
static MASTER_KEY: OnceCell<String> = OnceCell::new();

/// 注册 master key（hex 编码的 32 字节；重复注册忽略）
pub fn register_master_key(hex: String) {
    let _ = MASTER_KEY.set(hex);
}

/// 判断值是否为本模块加密格式
pub fn is_encrypted(value: &str) -> bool {
    value.starts_with(PREFIX)
}

/// 加密明文 → 带前缀的密文字符串
pub fn encrypt_value(plaintext: &str) -> AppResult<String> {
    let key = master_key_bytes()?;
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
    let mut nonce_bytes = [0u8; NONCE_BYTES];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let ct = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext.as_bytes())
        .map_err(|e| AppError::Internal(format!("设置值加密失败: {e}")))?;
    let mut blob = Vec::with_capacity(NONCE_BYTES + ct.len());
    blob.extend_from_slice(&nonce_bytes);
    blob.extend_from_slice(&ct);
    use base64::Engine;
    Ok(format!(
        "{PREFIX}{}",
        base64::engine::general_purpose::STANDARD.encode(&blob)
    ))
}

/// 解密；非加密格式（历史明文）返回 None，调用方按原样使用
pub fn decrypt_value(value: &str) -> AppResult<Option<String>> {
    let Some(b64) = value.strip_prefix(PREFIX) else {
        return Ok(None);
    };
    use base64::Engine;
    let blob = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| AppError::Internal(format!("设置值密文格式非法: {e}")))?;
    if blob.len() <= NONCE_BYTES {
        return Err(AppError::Internal("设置值密文长度非法".into()));
    }
    let key = master_key_bytes()?;
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
    let pt = cipher
        .decrypt(Nonce::from_slice(&blob[..NONCE_BYTES]), &blob[NONCE_BYTES..])
        .map_err(|e| AppError::Internal(format!("设置值解密失败（master key 不匹配？）: {e}")))?;
    String::from_utf8(pt).map(Some).map_err(|_| AppError::Internal("设置值解密后非有效 UTF-8".into()))
}

/// 从已注册的 master key（hex）解出 32 字节原始密钥
fn master_key_bytes() -> AppResult<[u8; KEY_BYTES]> {
    let hex = MASTER_KEY
        .get()
        .ok_or_else(|| AppError::Internal("master key 未注册（init_db 未完成）".into()))?;
    if hex.len() != KEY_BYTES * 2 {
        return Err(AppError::Internal("master key 长度异常".into()));
    }
    let mut out = [0u8; KEY_BYTES];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
            .map_err(|_| AppError::Internal("master key 含非 hex 字符".into()))?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        register_master_key("00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff".into());
        let enc = encrypt_value("hello 会话数据").unwrap();
        assert!(is_encrypted(&enc));
        assert_eq!(decrypt_value(&enc).unwrap().as_deref(), Some("hello 会话数据"));
    }

    #[test]
    fn plaintext_returns_none() {
        assert!(!is_encrypted("{\"a\":1}"));
        assert_eq!(decrypt_value("{\"a\":1}").unwrap(), None);
    }
}
