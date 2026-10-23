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

//! 统一安全密码（本地保护）后端实现。
//! 作者：JC
//!
//! 设计要点：
//! - 一个 4 位数字 PIN，按用户勾选的保护范围（查看会话密码必选 / 软件启动可选 / 修改删除会话可选）对敏感操作做验证。
//! - PIN 哈希 + 盐 + 失败计数 + 锁定截止时间，整体序列化后存入 **OS 凭据库（keyring）**，
//!   不写本地 app 数据文件、不写前端 localStorage。凭据库在应用卸载时不会被删除，
//!   故「删除本地文件 / 卸载重装」都无法关闭安全密码。
//! - 锁定状态（连续 5 次错误锁定 10 分钟）同样落在凭据库，重启 / 关闭软件无法跳过锁定时长。
//! - 同步：策略文档（enabled + scopes + salt + pin_hash + updated_at）可经前端同步引擎随 prefs 上云，
//!   实现「公司开了、家里自动开、同一 PIN」；锁定计数不同步（避免跨设备锁死）。

use std::sync::Arc;

use base64::Engine;
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::{Digest, Sha256};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::common::{AppError, AppResult};
use crate::AppState;

#[cfg(desktop)]
const KEYRING_SERVICE: &str = "jc-shell";
#[cfg(desktop)]
const KEYRING_SECURITY_ACCOUNT: &str = "security-pwd-v1";

const PIN_LEN: usize = 4;
/// 当前 PBKDF2 迭代次数（SHA256）。4 位 PIN 搜索空间仅 10^4，KDF 本就无法抵御离线暴力，
/// 降迭代仅为让「解锁 / 验证」有零延迟的体感；真正的防护来自 OS 凭据库 + 云端账号鉴权。
const PBKDF2_ITER: u32 = 10_000;
/// 旧版本存储的哈希未记录迭代次数，回退到此值以保证存量 PIN 仍可验证
const LEGACY_PBKDF2_ITER: u32 = 100_000;
const MAX_ATTEMPTS: u32 = 5;
const LOCK_MS: i64 = 10 * 60 * 1000;

type HmacSha256 = Hmac<Sha256>;

/// 保护范围（用户勾选）。view_password / export_password 始终为 true（必选）。
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub struct SecurityScopes {
    pub view_password: bool,
    pub startup: bool,
    pub edit_delete: bool,
    /// 导出会话数据时验证（可选）
    #[serde(default)]
    pub export_data: bool,
    /// 导出会话密码时验证（始终开启，强制为 true）
    #[serde(default)]
    pub export_password: bool,
}

impl SecurityScopes {
    /// 归一化：export_password 为强制项，兼容旧数据缺字段（serde default = false）的情况
    pub fn normalized(mut self) -> Self {
        self.export_password = true;
        self
    }
}

/// 凭据库中的完整状态（含本地才有的锁定信息）。
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
struct SecurityBlob {
    enabled: bool,
    scopes: SecurityScopes,
    /// 盐（hex）
    salt: String,
    /// PIN 哈希（base64 of PBKDF2-SHA256 输出）
    pin_hash: String,
    /// 计算该哈希所用的 PBKDF2 迭代次数（旧数据缺省为 0，按 LEGACY_PBKDF2_ITER 回退）
    #[serde(default)]
    iterations: u32,
    /// 策略最后更新时间（unix 毫秒）
    updated_at: i64,
    /// 连续失败次数（仅本机）
    attempts: u32,
    /// 锁定截止时间（unix 毫秒；0 表示未锁定）
    lock_until: i64,
    /// 是否启用生物识别解锁（**本机因子**：仅移动端可能为 true；桌面端恒 false，
    /// 且刻意不随云端策略同步 —— 换台设备指纹/人脸并不通用）
    #[serde(default)]
    biometric: bool,
    /// 生物识别方式：'both'（指纹+人脸）/ 'fingerprint' / 'face'。
    /// 只能做**近似**：Android 的 BiometricPrompt 不提供「指定模态」的能力，应用只能选择
    /// 认证强度 —— 'fingerprint' 用 BIOMETRIC_STRONG（对 class-2 人脸机型即等价于仅指纹，
    /// 因为 class-2 人脸只属于 WEAK），'face'/'both' 用 BIOMETRIC_WEAK（是否弹人脸由系统
    /// 与 ROM 决定，多数机型会优先人脸）。精确的「仅人脸」在 Android 上无法强制。
    #[serde(default = "default_biometric_mode")]
    biometric_mode: String,
}

/// 随 prefs 同步到云端的策略文档（不含锁定信息）。
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SecurityPolicySync {
    pub enabled: bool,
    pub scopes: SecurityScopes,
    pub salt: String,
    pub pin_hash: String,
    pub updated_at: i64,
    /// 同步下发的 PBKDF2 迭代次数（缺省 0 = 旧版，按 LEGACY 回退）
    #[serde(default)]
    pub iterations: u32,
}

#[derive(Serialize)]
pub struct SecurityStatus {
    pub has_password: bool,
    pub scopes: SecurityScopes,
    pub locked: bool,
    pub lock_remaining_sec: i64,
    /// 当前平台是否支持生物识别解锁（移动端 true；桌面端 false，其解锁仅走 PIN）
    pub biometric_supported: bool,
    /// 是否已开启生物识别解锁（平台不支持 / 未设密码时恒 false）
    pub biometric_enabled: bool,
    /// 生物识别方式：'both' | 'fingerprint' | 'face'（未开启时也返回用户上次的选择）
    pub biometric_mode: String,
}

#[derive(Serialize)]
pub struct VerifyResult {
    pub ok: bool,
    pub locked: bool,
    pub lock_remaining_sec: i64,
}

// ===== 状态读写 =====
// 桌面端：密钥存 OS 凭据库（卸载 / 删除本地文件无法绕过），这是该功能的安全前提。
// 移动端（iOS/Android）：平台没有可供 Rust 使用的凭据库后端，改为落到**应用私有目录内的
//   AES-256-GCM 加密文件**（密钥取自 keystore 模块的 DB master key；移动端该 key 存于
//   数据目录下的 jcshell.key），故密文无法被第三方工具直接读出。
//   定位与 app_setting 的透明加密一致：属「访问控制」而非绝对机密 —— 锁定本就是前端提示、
//   非硬安全边界；移动端另以系统生物识别作为更强的独立因子叠加，见 security_set_biometric。

/// 移动端安全密码状态文件名（应用私有目录内，内容为 JCENC 密文）
#[cfg(mobile)]
const MOBILE_BLOB_FILE: &str = "security.blob";

#[cfg(mobile)]
fn mobile_blob_path() -> std::path::PathBuf {
    crate::common::paths::app_data_dir().join(MOBILE_BLOB_FILE)
}

#[cfg(desktop)]
fn entry() -> AppResult<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_SECURITY_ACCOUNT)
        .map_err(|e| AppError::Internal(format!("KeyChain 初始化失败: {e}")))
}

fn read_blob() -> AppResult<Option<SecurityBlob>> {
    #[cfg(desktop)]
    {
        let e = entry()?;
        match e.get_password() {
            Ok(json) => {
                let blob: SecurityBlob = serde_json::from_str(&json)
                    .map_err(|e| AppError::Internal(format!("安全密码状态解析失败: {e}")))?;
                Ok(Some(blob))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(AppError::Internal(format!("KeyChain 读取安全密码失败: {e}"))),
        }
    }
    // 移动端：读、解密、解析三步任何一步失败都必须**报错**，绝不能吞掉当作「未设置密码」——
    // 那属于 fail-open：用户以为保护还在，实际已可长驱直入（例如文件被截断 / master key 不匹配）。
    #[cfg(mobile)]
    {
        let path = mobile_blob_path();
        let raw = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(AppError::Internal(format!("安全密码状态读取失败: {e}"))),
        };
        let raw = raw.trim();
        if raw.is_empty() {
            return Ok(None);
        }
        // 带 JCENC 前缀 = 密文；否则视为历史明文（理论上不出现，仍按明文解析以便自愈）
        let json = match crate::services::keystore::db_crypto::decrypt_value(raw)? {
            Some(plain) => plain,
            None => raw.to_string(),
        };
        let blob: SecurityBlob = serde_json::from_str(&json)
            .map_err(|e| AppError::Internal(format!("安全密码状态解析失败: {e}")))?;
        Ok(Some(blob))
    }
}

fn write_blob(blob: &SecurityBlob) -> AppResult<()> {
    #[cfg(desktop)]
    {
        let json = serde_json::to_string(blob)
            .map_err(|e| AppError::Internal(format!("安全密码状态序列化失败: {e}")))?;
        entry()?
            .set_password(&json)
            .map_err(|e| AppError::Internal(format!("KeyChain 写入安全密码失败: {e}")))
    }
    // 移动端：先写临时文件再原子 rename —— 避免写入中途掉电/被杀进程留下半截文件，
    // 那会让用户在重启后既验不了密码也解不开锁（等于永久锁在门外）。
    #[cfg(mobile)]
    {
        let json = serde_json::to_string(blob)
            .map_err(|e| AppError::Internal(format!("安全密码状态序列化失败: {e}")))?;
        let encrypted = crate::services::keystore::db_crypto::encrypt_value(&json)?;
        let path = mobile_blob_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| AppError::Internal(format!("安全密码目录创建失败: {e}")))?;
        }
        let tmp = path.with_extension("blob.tmp");
        std::fs::write(&tmp, encrypted)
            .map_err(|e| AppError::Internal(format!("安全密码写入失败: {e}")))?;
        std::fs::rename(&tmp, &path)
            .map_err(|e| AppError::Internal(format!("安全密码保存失败: {e}")))
    }
}

fn delete_blob() -> AppResult<bool> {
    #[cfg(desktop)]
    {
        let e = entry()?;
        match e.delete_credential() {
            Ok(()) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(e) => Err(AppError::Internal(format!("KeyChain 删除安全密码失败: {e}"))),
        }
    }
    #[cfg(mobile)]
    {
        let path = mobile_blob_path();
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(AppError::Internal(format!("安全密码删除失败: {e}"))),
        }
    }
}

// ===== KDF（PBKDF2-HMAC-SHA256）=====
// 注：4 位数字 PIN 搜索空间仅 10^4，KDF 无法抵御离线暴力；此处用 PBKDF2 仅作"强 KDF"形式，
// 真正的防护来自凭据库本身 + 云端账号鉴权。若需更强 KDF 可换 scrypt/argon2（需新增依赖）。

fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::thread_rng().fill_bytes(&mut buf);
    let mut s = String::with_capacity(bytes * 2);
    for b in &buf {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn pbkdf2(password: &[u8], salt: &[u8], iterations: u32, out: &mut [u8]) {
    let hlen = 32usize;
    let mut idx: u32 = 1;
    let mut offset = 0;
    while offset < out.len() {
        // U1 = PRF(password, salt || INT_32_BE(idx))
        let mut prf = HmacSha256::new_from_slice(password).expect("HMAC 接受任意长度 key");
        prf.update(salt);
        prf.update(&idx.to_be_bytes());
        let mut u = prf.finalize().into_bytes().to_vec();
        let mut t = u.clone();
        for _ in 1..iterations {
            let mut prf2 = HmacSha256::new_from_slice(password).expect("HMAC 接受任意长度 key");
            prf2.update(&u);
            u = prf2.finalize().into_bytes().to_vec();
            for (a, b) in t.iter_mut().zip(u.iter()) {
                *a ^= *b;
            }
        }
        let take = hlen.min(out.len() - offset);
        out[offset..offset + take].copy_from_slice(&t[..take]);
        offset += take;
        idx += 1;
    }
}

fn hash_pin(pin: &str, salt_hex: &str, iterations: u32) -> String {
    let salt = hex_decode(salt_hex);
    let mut dk = [0u8; 32];
    pbkdf2(pin.as_bytes(), &salt, iterations, &mut dk);
    base64::engine::general_purpose::STANDARD.encode(dk)
}

fn hex_decode(hex: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(hex.len() / 2);
    let mut chars = hex.chars();
    while let (Some(h), Some(l)) = (chars.next(), chars.next()) {
        let hv = h.to_digit(16).unwrap_or(0);
        let lv = l.to_digit(16).unwrap_or(0);
        out.push(((hv << 4) | lv) as u8);
    }
    out
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn validate_pin(pin: &str) -> AppResult<()> {
    if pin.len() != PIN_LEN || !pin.chars().all(|c| c.is_ascii_digit()) {
        return Err(AppError::Validation(format!("安全密码必须是 {PIN_LEN} 位数字")));
    }
    Ok(())
}

/// 生物识别方式默认值：指纹与人脸都允许
fn default_biometric_mode() -> String {
    "both".to_string()
}

/// 归一化生物识别方式：非法值一律回落到 both。
/// 后端不信任前端传参（该值会决定认证强度，传错等于放宽了验证要求）。
fn normalize_biometric_mode(mode: &str) -> String {
    match mode {
        "fingerprint" | "face" | "both" => mode.to_string(),
        _ => default_biometric_mode(),
    }
}

// ===== Commands =====

#[tauri::command]
pub async fn security_status() -> AppResult<SecurityStatus> {
    // 平台是否支持生物识别：这里只回答「平台能力」，硬件/录入情况由前端问原生桥
    // （BiometricManager），避免 Rust 侧为一件纯原生的事去跨语言调用。
    let supported = cfg!(mobile);
    match read_blob()? {
        None => Ok(SecurityStatus {
            has_password: false,
            scopes: SecurityScopes::default(),
            locked: false,
            lock_remaining_sec: 0,
            biometric_supported: supported,
            biometric_enabled: false,
            biometric_mode: default_biometric_mode(),
        }),
        Some(b) => {
            let locked = b.enabled && b.lock_until > now_ms();
            let remaining = if locked { ((b.lock_until - now_ms()) / 1000).max(0) } else { 0 };
            let mode = normalize_biometric_mode(&b.biometric_mode);
            Ok(SecurityStatus {
                has_password: b.enabled,
                scopes: b.scopes.normalized(),
                locked,
                lock_remaining_sec: remaining,
                biometric_supported: supported,
                biometric_enabled: supported && b.enabled && b.biometric,
                biometric_mode: mode,
            })
        }
    }
}

#[tauri::command]
pub async fn security_set_password(
    password: String,
    scopes: SecurityScopes,
) -> AppResult<()> {
    validate_pin(&password)?;
    let salt = random_hex(16);
    let pin_hash = hash_pin(&password, &salt, PBKDF2_ITER);
    let blob = SecurityBlob {
        enabled: true,
        scopes: scopes.normalized(),
        salt,
        pin_hash,
        updated_at: now_ms(),
        attempts: 0,
        lock_until: 0,
        iterations: PBKDF2_ITER,
        // 新设 / 重设密码时生物识别一律关闭，必须由用户在设置里显式开启（且要再验一次 PIN）
        biometric: false,
        biometric_mode: default_biometric_mode(),
    };
    write_blob(&blob)?;
    // 写后回读校验：防止 keyring 未启用平台后端而静默回退到 mock 存储
    match read_blob()? {
        Some(b)
            if b.enabled
                && b.pin_hash == blob.pin_hash
                && b.salt == blob.salt
                && b.scopes.view_password == blob.scopes.view_password =>
        {
            Ok(())
        }
        _ => Err(AppError::Internal(
            "安全密码写入后无法回读，系统凭据库可能不可用".into(),
        )),
    }
}

#[tauri::command]
pub async fn security_verify(password: String) -> AppResult<VerifyResult> {
    let mut blob = match read_blob()? {
        Some(b) if b.enabled => b,
        _ => return Err(AppError::Validation("尚未设置安全密码".into())),
    };
    if blob.lock_until > now_ms() {
        let remaining = ((blob.lock_until - now_ms()) / 1000).max(0);
        return Ok(VerifyResult { ok: false, locked: true, lock_remaining_sec: remaining });
    }
    // 失败计数 + 锁定
    blob.attempts += 1;
    if blob.attempts >= MAX_ATTEMPTS {
        blob.lock_until = now_ms() + LOCK_MS;
        blob.attempts = 0;
        write_blob(&blob)?;
        return Ok(VerifyResult { ok: false, locked: true, lock_remaining_sec: LOCK_MS / 1000 });
    }
    let iter = if blob.iterations == 0 { LEGACY_PBKDF2_ITER } else { blob.iterations };
    let ok = hash_pin(&password, &blob.salt, iter) == blob.pin_hash;
    if ok {
        // 仅当此前有过失败计数才回写（重置为 0）；首次即正确的常见路径跳过一次 Keychain 写入，解锁更快
        if blob.attempts != 0 {
            blob.attempts = 0;
            write_blob(&blob)?;
        }
        return Ok(VerifyResult { ok: true, locked: false, lock_remaining_sec: 0 });
    }
    write_blob(&blob)?;
    Ok(VerifyResult { ok: false, locked: false, lock_remaining_sec: 0 })
}

#[tauri::command]
pub async fn security_disable(password: String) -> AppResult<()> {
    let blob = match read_blob()? {
        Some(b) if b.enabled => b,
        _ => return Err(AppError::Validation("尚未设置安全密码".into())),
    };
    if blob.lock_until > now_ms() {
        return Err(AppError::Validation("安全密码已锁定，请稍后再试".into()));
    }
    let iter = if blob.iterations == 0 { LEGACY_PBKDF2_ITER } else { blob.iterations };
    if hash_pin(&password, &blob.salt, iter) != blob.pin_hash {
        return Err(AppError::Validation("安全密码错误".into()));
    }
    delete_blob()?;
    Ok(())
}

#[tauri::command]
pub async fn security_update_scopes(password: String, scopes: SecurityScopes) -> AppResult<()> {
    let mut blob = match read_blob()? {
        Some(b) if b.enabled => b,
        _ => return Err(AppError::Validation("尚未设置安全密码".into())),
    };
    if blob.lock_until > now_ms() {
        return Err(AppError::Validation("安全密码已锁定，请稍后再试".into()));
    }
    let iter = if blob.iterations == 0 { LEGACY_PBKDF2_ITER } else { blob.iterations };
    if hash_pin(&password, &blob.salt, iter) != blob.pin_hash {
        return Err(AppError::Validation("安全密码错误".into()));
    }
    blob.scopes = scopes.normalized();
    blob.updated_at = now_ms();
    blob.attempts = 0;
    write_blob(&blob)
}

/// 开启 / 关闭生物识别解锁（仅移动端）。
/// 按产品要求：**开启与关闭都必须先验证 PIN** —— 开启前验 PIN 防止他人偷开；
/// 关闭前验 PIN 防止拿到已解锁手机的人直接关掉保护。
#[tauri::command]
pub async fn security_set_biometric(
    password: String,
    enabled: bool,
    mode: Option<String>,
) -> AppResult<()> {
    if !cfg!(mobile) {
        return Err(AppError::Validation("当前平台不支持生物识别解锁".into()));
    }
    let mut blob = match read_blob()? {
        Some(b) if b.enabled => b,
        _ => return Err(AppError::Validation("尚未设置安全密码".into())),
    };
    if blob.lock_until > now_ms() {
        return Err(AppError::Validation("安全密码已锁定，请稍后再试".into()));
    }
    let iter = if blob.iterations == 0 { LEGACY_PBKDF2_ITER } else { blob.iterations };
    if hash_pin(&password, &blob.salt, iter) != blob.pin_hash {
        return Err(AppError::Validation("安全密码错误".into()));
    }
    blob.biometric = enabled;
    // 每次改动（含只换方式）都要走一遍上面的 PIN 校验，所以这里没有任何「免验证」的快捷分支
    if let Some(m) = mode {
        blob.biometric_mode = normalize_biometric_mode(&m);
    }
    blob.updated_at = now_ms();
    blob.attempts = 0;
    write_blob(&blob)
}

/// 不验证 PIN，直接抹除会话数据并移除安全密码（"抹除会话数据并关闭"专用）。
/// 仅删除本地 `host_manager*` 会话数据，不动其它设置；同时移除凭据库中的 PIN。
#[tauri::command]
pub async fn security_wipe_and_disable(state: State<'_, Arc<AppState>>) -> AppResult<()> {
    if read_blob()?.is_some() {
        delete_blob()?;
    }
    sqlx::query("DELETE FROM app_setting WHERE key LIKE ?")
        .bind("host_manager%")
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(format!("清除会话数据失败: {e}")))?;
    Ok(())
}

/// 供前端同步引擎读取要上云的策略（未开启则返回 null）。
#[tauri::command]
pub async fn security_get_sync() -> AppResult<Option<SecurityPolicySync>> {
    match read_blob()? {
        Some(b) if b.enabled => Ok(Some(SecurityPolicySync {
            enabled: b.enabled,
            scopes: b.scopes.normalized(),
            salt: b.salt,
            pin_hash: b.pin_hash,
            updated_at: b.updated_at,
            iterations: b.iterations,
        })),
        _ => Ok(None),
    }
}

/// 供前端同步引擎应用云端下发的策略（last-write-wins by updated_at）。
/// 传入 null 表示其它设备已抹除并关闭 → 本机同步移除 PIN（锁定计数不继承）。
#[tauri::command]
pub async fn security_set_sync(policy: Option<SecurityPolicySync>) -> AppResult<bool> {
    match policy {
        None => {
            let existed = delete_blob()?;
            Ok(existed)
        }
        Some(p) => {
            let local = read_blob()?;
            let should_apply = match &local {
                None => true,
                Some(b) => p.updated_at >= b.updated_at,
            };
            if !should_apply {
                return Ok(false);
            }
            // 生物识别是本机因子，不随云端策略覆盖：否则「桌面端改了保护范围」同步过来时，
            // 会把本机已开启的指纹/人脸解锁悄悄抹掉。方式（mode）同理，一起保留。
            let local_biometric = local.as_ref().map(|b| b.biometric).unwrap_or(false);
            let local_mode = local
                .as_ref()
                .map(|b| normalize_biometric_mode(&b.biometric_mode))
                .unwrap_or_else(default_biometric_mode);
            let blob = SecurityBlob {
                enabled: p.enabled,
                scopes: p.scopes.normalized(),
                salt: p.salt,
                pin_hash: p.pin_hash,
                updated_at: p.updated_at,
                attempts: 0,
                lock_until: 0,
                iterations: if p.iterations == 0 { LEGACY_PBKDF2_ITER } else { p.iterations },
                biometric: local_biometric,
                biometric_mode: local_mode,
            };
            write_blob(&blob)?;
            Ok(true)
        }
    }
}

// ===== 纯逻辑单元测试（无需 GUI / 凭据库 / 数据库，headless 也可跑）=====
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pbkdf2_确定性且不同输入产生不同哈希() {
        let pw = b"1234";
        let salt = [7u8; 16];
        let mut a = [0u8; 32];
        let mut b = [0u8; 32];
        pbkdf2(pw, &salt, PBKDF2_ITER, &mut a);
        pbkdf2(pw, &salt, PBKDF2_ITER, &mut b);
        assert_eq!(a, b, "相同输入必须产生相同哈希");
        let mut c = [0u8; 32];
        pbkdf2(b"9999", &salt, PBKDF2_ITER, &mut c);
        assert_ne!(a, c, "不同 PIN 必须产生不同哈希");
    }

    #[test]
    fn hash_pin_可复现且盐不同结果不同() {
        let salt = "abcdef0123456789abcdef0123456789";
        assert_eq!(hash_pin("1234", salt), hash_pin("1234", salt));
        assert_ne!(hash_pin("1234", salt), hash_pin("0000", salt));
        // 盐不同 => 哈希不同（防彩虹表）
        let other = "0123456789abcdef0123456789abcdef";
        assert_ne!(hash_pin("1234", salt), hash_pin("1234", other));
    }

    #[test]
    fn hex_decode_可逆() {
        let hex = random_hex(24);
        let bytes = hex_decode(&hex);
        assert_eq!(bytes.len(), 24);
        // 重新 hex 编码应一致
        let mut s = String::new();
        for b in &bytes {
            s.push_str(&format!("{b:02x}"));
        }
        assert_eq!(s, hex);
    }

    #[test]
    fn validate_pin_只接受四位数字() {
        assert!(validate_pin("1234").is_ok());
        assert!(validate_pin("0123").is_ok());
        assert!(validate_pin("123").is_err());
        assert!(validate_pin("12345").is_err());
        assert!(validate_pin("12a4").is_err());
    }

    /// 关键回归护栏：验证 keyring 启用了真正的平台原生后端。
    /// mock 后端下「新建 Entry 实例读取另一个实例写入的内容」会失败，此测试必须变红。
    #[test]
    fn keyring_跨实例回读_必须走真实持久化后端() {
        use keyring::Entry;
        let svc = "jc-shell-regression-test";
        let acct = "roundtrip";
        let e1 = Entry::new(svc, acct).expect("创建凭据条目失败");
        let _ = e1.delete_credential(); // 清理旧数据
        e1.set_password("persist-check").expect("写入凭据失败");
        let e2 = Entry::new(svc, acct).expect("重新打开凭据条目失败");
        assert_eq!(
            e2.get_password().expect("跨实例读取凭据失败"),
            "persist-check",
            "keyring 可能未启用平台原生后端（mock 回退）"
        );
        e2.delete_credential().expect("清理测试凭据失败");
    }
}
