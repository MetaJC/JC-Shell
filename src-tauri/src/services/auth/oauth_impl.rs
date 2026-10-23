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

//! OAuth + PKCE + 本地回调 HTTP 服务器实现
//! 作者：JC

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use rand::RngCore;
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, RwLock};

use crate::common::{AppError, AppResult};
use crate::services::auth::{AuthClientConfig, AuthState, AuthService, PhoneCaptchaData, TokenSet, UserInfo};

#[cfg(desktop)]
const KEYRING_SERVICE: &str = "jc-shell";
const KEYRING_ACCESS: &str = "auth-access-token";
const KEYRING_REFRESH: &str = "auth-refresh-token";
const KEYRING_EXPIRY: &str = "auth-token-expiry";
/// token 文件备份名（数据目录 %AppData%\JC Shell 下；与 DB master key 备份同源策略）
const TOKEN_BACKUP_FILE: &str = "auth-token.backup";
const CALLBACK_TIMEOUT_SECS: u64 = 300;
const STATE_LEN: usize = 32;
const VERIFIER_LEN: usize = 48;
/// token 提前多久刷新（秒），避免边界过期
const TOKEN_REFRESH_LEAD_SECS: u64 = 60;

const CALLBACK_HTML: &str = r#"<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>授权完成</title></head>
<body style="font-family:system-ui,-apple-system,sans-serif;background:#1a1a1f;color:#efece6;display:flex;align-items:center;justify-content:center;height:100vh;margin:0;">
<div style="text-align:center;">
<h2 style="margin:0 0 8px;font-size:22px;">授权完成</h2>
<p style="margin:0;color:#b8b4ac;font-size:14px;">请回到 JC Shell 桌面端继续</p>
</div>
<script>window.close();</script>
</body></html>"#;

#[derive(Debug, Clone)]
struct PendingLogin {
    state: String,
    code_verifier: String,
    started_at: std::time::Instant,
}

pub struct OAuthClient {
    inner: Arc<Inner>,
}

struct Inner {
    cfg: RwLock<AuthClientConfig>,
    http: HttpClient,
    pending: Mutex<Option<PendingLogin>>,
    /// 内存中的 access_token 缓存（KeyChain 的 fallback，避免刚登录却读不到 token）
    cached_access_token: RwLock<Option<String>>,
}

impl OAuthClient {
    pub fn new(cfg: AuthClientConfig) -> Self {
        let http = crate::common::http::apply_client_defaults(HttpClient::builder())
            .timeout(Duration::from_secs(15))
            .build()
            .expect("reqwest::Client 构建失败");
        Self {
            inner: Arc::new(Inner {
                cfg: RwLock::new(cfg),
                http,
                pending: Mutex::new(None),
                cached_access_token: RwLock::new(None),
            }),
        }
    }

    pub async fn update_config(&self, cfg: AuthClientConfig) {
        let mut w = self.inner.cfg.write().await;
        *w = cfg;
    }

    pub async fn cfg_snapshot(&self) -> AuthClientConfig {
        self.inner.cfg.read().await.clone()
    }

    async fn post_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> AppResult<ApiData<T>> {
        let cfg = self.cfg_snapshot().await;
        if cfg.server_base_url.trim().is_empty() {
            return Err(AppError::Internal(
                "后端地址未配置 (server_base_url 为空). 请检查 .env 是否正确加载, 或在「设置 → 后端」中手动填写.".into(),
            ));
        }
        let url = format!("{}{}", cfg.server_base_url.trim_end_matches('/'), path);
        let resp = self
            .inner
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("HTTP 请求失败: {e}")))?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(AppError::Internal(format!("HTTP {status}: {text}")));
        }
        let parsed = serde_json::from_str::<ApiData<T>>(&text)
            .map_err(|e| AppError::Internal(format!("响应解析失败({path}): {e}")))?;
        if parsed.code != 0 {
            return Err(AppError::Internal(
                parsed.message.unwrap_or_else(|| format!("业务失败 code={}", parsed.code)),
            ));
        }
        Ok(parsed)
    }

    /// 带 Bearer Token 的 GET 请求
    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        access_token: &str,
    ) -> AppResult<ApiData<T>> {
        let cfg = self.cfg_snapshot().await;
        if cfg.server_base_url.trim().is_empty() {
            return Err(AppError::Internal(
                "后端地址未配置 (server_base_url 为空). 请检查 .env 是否正确加载, 或在「设置 → 后端」中手动填写.".into(),
            ));
        }
        let url = format!("{}{}", cfg.server_base_url.trim_end_matches('/'), path);
        let resp = self
            .inner
            .http
            .get(&url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("HTTP 请求失败: {e}")))?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(AppError::Internal(format!("HTTP {status}: {text}")));
        }
        let parsed = serde_json::from_str::<ApiData<T>>(&text)
            .map_err(|e| AppError::Internal(format!("响应解析失败({path}): {e}")))?;
        if parsed.code != 0 {
            return Err(AppError::Internal(
                parsed.message.unwrap_or_else(|| format!("业务失败 code={}", parsed.code)),
            ));
        }
        Ok(parsed)
    }

    /// 公开 GET 请求（无需 Bearer Token），用于滑块验证码拉取等 permitAll 接口
    async fn get_public_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
    ) -> AppResult<ApiData<T>> {
        let cfg = self.cfg_snapshot().await;
        if cfg.server_base_url.trim().is_empty() {
            return Err(AppError::Internal(
                "后端地址未配置 (server_base_url 为空). 请检查 .env 是否正确加载, 或在「设置 → 后端」中手动填写.".into(),
            ));
        }
        let url = format!("{}{}", cfg.server_base_url.trim_end_matches('/'), path);
        let resp = self
            .inner
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("HTTP 请求失败: {e}")))?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(AppError::Internal(format!("HTTP {status}: {text}")));
        }
        let parsed = serde_json::from_str::<ApiData<T>>(&text)
            .map_err(|e| AppError::Internal(format!("响应解析失败({path}): {e}")))?;
        if parsed.code != 0 {
            return Err(AppError::Internal(
                parsed.message.unwrap_or_else(|| format!("业务失败 code={}", parsed.code)),
            ));
        }
        Ok(parsed)
    }

    /// 读取 access_token：优先内存缓存，其次 KeyChain，最后数据目录文件备份
    async fn load_access_token_cached(&self) -> AppResult<Option<String>> {
        // 1. 先查内存缓存（刚登录的 token 一定在这里）
        {
            let cache = self.inner.cached_access_token.read().await;
            if let Some(t) = cache.as_ref() {
                return Ok(Some(t.clone()));
            }
        }
        // 2. 缓存未命中，回退 KeyChain / 文件备份
        match Self::load_access_token()? {
            Some(v) => {
                // 回填缓存
                let mut cache = self.inner.cached_access_token.write().await;
                *cache = Some(v.clone());
                Ok(Some(v))
            }
            None => Ok(None),
        }
    }

    /// 读取 access_token：KeyChain 为主，数据目录文件备份兜底。
    /// 单侧缺失时自动补齐另一侧，避免下次换环境后彻底丢失登录态。
    fn load_access_token() -> AppResult<Option<String>> {
        if let Some(v) = keyring_get(KEYRING_ACCESS) {
            // KeyChain 有：补齐文件备份（老版本升级后的首次读取）
            if read_token_backup().is_none() {
                let refresh = Self::load_refresh_token().ok().flatten().unwrap_or_default();
                let _ = write_token_backup(&v, &refresh, Self::load_token_expiry());
            }
            return Ok(Some(v));
        }
        // KeyChain 条目丢失（Windows 凭据管理器被清理/策略清除）：从文件备份恢复
        let Some(backup) = read_token_backup() else {
            return Ok(None);
        };
        if backup.access.is_empty() {
            return Ok(None);
        }
        tracing::info!("KeyChain 无 access_token，改从数据目录备份恢复登录态");
        restore_keyring_from_backup(&backup);
        Ok(Some(backup.access))
    }

    fn load_refresh_token() -> AppResult<Option<String>> {
        if let Some(v) = keyring_get(KEYRING_REFRESH) {
            return Ok(Some(v));
        }
        Ok(read_token_backup().map(|b| b.refresh).filter(|s| !s.is_empty()))
    }

    /// 读取 token 过期时间戳（Unix 秒），不存在返回 None
    fn load_token_expiry() -> Option<u64> {
        if let Some(v) = keyring_get(KEYRING_EXPIRY) {
            if let Ok(n) = v.parse::<u64>() {
                return Some(n);
            }
        }
        read_token_backup().and_then(|b| b.expiry)
    }

    /// 持久化 token：KeyChain 为主存储，数据目录文件为备份。
    /// 两者其一成功即视为持久化成功 —— 这样 KeyChain 不可用时仍能跨进程保持登录态。
    fn store_tokens(access: &str, refresh: &str, expires_in: u64) -> AppResult<()> {
        // 存储绝对过期时间戳
        let expiry = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            + expires_in;

        let mut persisted = false;
        let mut last_err: Option<AppError> = None;

        // 桌面端 KeyChain 主存；移动端无 KeyChain，仅依赖文件备份维持登录态
        #[cfg(desktop)]
        {
            match keyring_set(KEYRING_ACCESS, access)
                .and_then(|_| keyring_set(KEYRING_REFRESH, refresh))
            {
                Ok(()) => {
                    persisted = true;
                    if let Err(e) = keyring_set(KEYRING_EXPIRY, &expiry.to_string()) {
                        tracing::warn!(%e, "KeyChain 写入过期时间失败（不影响登录）");
                    }
                }
                Err(e) => {
                    tracing::warn!(%e, "KeyChain 写入 token 失败，依赖文件备份维持登录态");
                    last_err = Some(e);
                }
            }
        }

        match write_token_backup(access, refresh, Some(expiry)) {
            Ok(()) => persisted = true,
            Err(e) => {
                tracing::warn!(%e, "token 文件备份写入失败");
                last_err = Some(AppError::Internal(format!("token 备份写入失败: {e}")));
            }
        }

        if persisted {
            Ok(())
        } else {
            Err(last_err.unwrap_or_else(|| AppError::Internal("token 持久化失败".into())))
        }
    }

    /// 将 token 写入内存缓存（供 load_access_token_cached 使用）
    async fn cache_access_token(&self, token: &str) {
        let mut cache = self.inner.cached_access_token.write().await;
        *cache = Some(token.to_string());
    }

    /// 清除内存缓存
    async fn clear_cached_token(&self) {
        let mut cache = self.inner.cached_access_token.write().await;
        *cache = None;
    }

    /// 清除 token：KeyChain 与文件备份都要清。
    /// 否则残留的备份文件会让"已登出"的用户下次启动又自动登录回来。
    fn clear_tokens() -> AppResult<()> {
        #[cfg(desktop)]
        {
            for account in [KEYRING_ACCESS, KEYRING_REFRESH, KEYRING_EXPIRY] {
                if let Ok(e) = keyring::Entry::new(KEYRING_SERVICE, account) {
                    let _ = e.delete_credential();
                }
            }
        }
        delete_token_backup();
        Ok(())
    }
}

// ===== token 文件备份 =====
// 动机（与 services/keystore/db_key.rs 同源）：实测 Windows 凭据管理器条目可能被
// 系统清理/策略删除，仅存 KeyChain 时条目一丢就表现为"重启后登录态消失"。
// 故除 KeyChain 外，在数据目录（%AppData%\JC Shell，卸载保留）再存一份备份：
// 读取 KeyChain 优先、文件兜底，单侧缺失自动补齐另一侧。
// 内容为 base64(JSON)，避免被文本工具直接读出明文 token；Windows 下追加隐藏属性。

#[derive(Debug, Serialize, Deserialize)]
struct TokenBackup {
    access: String,
    refresh: String,
    expiry: Option<u64>,
}

/// KeyChain 读取：条目不存在或 KeyChain 不可用都返回 None（不打断调用方，交由文件备份兜底）
fn keyring_get(account: &str) -> Option<String> {
    #[cfg(desktop)]
    {
        match keyring::Entry::new(KEYRING_SERVICE, account) {
            Ok(e) => match e.get_password() {
                Ok(v) => Some(v),
                Err(keyring::Error::NoEntry) => None,
                Err(e) => {
                    tracing::warn!(account = %account, %e, "KeyChain 读取失败，回退文件备份");
                    None
                }
            },
            Err(e) => {
                tracing::warn!(account = %account, %e, "KeyChain 初始化失败，回退文件备份");
                None
            }
        }
    }
    #[cfg(mobile)]
    {
        let _ = account;
        None
    }
}

fn keyring_set(account: &str, value: &str) -> AppResult<()> {
    #[cfg(desktop)]
    {
        keyring::Entry::new(KEYRING_SERVICE, account)
            .and_then(|e| e.set_password(value))
            .map_err(|e| AppError::Internal(format!("KeyChain 写入 {account} 失败: {e}")))
    }
    #[cfg(mobile)]
    {
        let _ = (account, value);
        Ok(())
    }
}

fn token_backup_path() -> std::path::PathBuf {
    crate::common::paths::app_data_dir().join(TOKEN_BACKUP_FILE)
}

fn read_token_backup() -> Option<TokenBackup> {
    let content = std::fs::read_to_string(token_backup_path()).ok()?;
    let raw = base64::engine::general_purpose::STANDARD
        .decode(content.trim())
        .ok()?;
    serde_json::from_slice::<TokenBackup>(&raw).ok()
}

fn write_token_backup(
    access: &str,
    refresh: &str,
    expiry: Option<u64>,
) -> std::io::Result<()> {
    let path = token_backup_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string(&TokenBackup {
        access: access.to_string(),
        refresh: refresh.to_string(),
        expiry,
    })
    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(json.as_bytes());
    std::fs::write(&path, b64)?;
    hide_token_backup(&path);
    Ok(())
}

fn delete_token_backup() {
    let _ = std::fs::remove_file(token_backup_path());
}

/// 从文件备份把 token 回写到 KeyChain（尽力而为，失败不阻塞登录流程）
fn restore_keyring_from_backup(b: &TokenBackup) {
    #[cfg(desktop)]
    {
        if !b.access.is_empty() {
            if let Err(e) = keyring_set(KEYRING_ACCESS, &b.access) {
                tracing::warn!(%e, "从备份回写 access_token 到 KeyChain 失败");
            }
        }
        if !b.refresh.is_empty() {
            if let Err(e) = keyring_set(KEYRING_REFRESH, &b.refresh) {
                tracing::warn!(%e, "从备份回写 refresh_token 到 KeyChain 失败");
            }
        }
        if let Some(exp) = b.expiry {
            if let Err(e) = keyring_set(KEYRING_EXPIRY, &exp.to_string()) {
                tracing::warn!(%e, "从备份回写过期时间到 KeyChain 失败");
            }
        }
    }
    #[cfg(mobile)]
    {
        let _ = b;
    }
}

#[cfg(windows)]
fn hide_token_backup(path: &std::path::Path) {
    use std::os::windows::ffi::OsStrExt;
    extern "system" {
        fn SetFileAttributesW(lpFileName: *const u16, dwFileAttributes: u32) -> i32;
    }
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide.push(0);
    unsafe {
        SetFileAttributesW(wide.as_ptr(), FILE_ATTRIBUTE_HIDDEN);
    }
}

#[cfg(not(windows))]
fn hide_token_backup(_path: &std::path::Path) {}

#[async_trait::async_trait]
impl AuthService for OAuthClient {
    async fn start_login(&self, app: AppHandle) -> AppResult<AuthState> {
        let cfg = self.cfg_snapshot().await;

        let state = random_base64url(STATE_LEN);
        let code_verifier = random_base64url(VERIFIER_LEN);
        let code_challenge = sha256_base64url(&code_verifier);

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| AppError::Internal(format!("本地回调服务启动失败: {e}")))?;
        let port = listener
            .local_addr()
            .map_err(|e| AppError::Internal(format!("获取本地端口失败: {e}")))?
            .port();
        let redirect_uri = format!("http://127.0.0.1:{port}/callback");

        let body = serde_json::json!({
            "clientId": cfg.client_id,
            "redirectUri": redirect_uri,
            "codeChallenge": code_challenge,
            "state": state,
            "scope": cfg.scopes,
        });
        let resp: ApiData<StartLoginData> = self.post_json("/api/openjc/client/auth/start", body).await?;
        let login_url = resp
            .data
            .ok_or_else(|| AppError::Internal("后端未返回 login_url".into()))?
            .login_url;

        {
            let mut p = self.inner.pending.lock().await;
            *p = Some(PendingLogin {
                state: state.clone(),
                code_verifier,
                started_at: std::time::Instant::now(),
            });
        }

        let inner = self.inner.clone();
        let app_for_task = app.clone();
        let expected_state = state.clone();
        let consumed_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        tokio::spawn(async move {
            let deadline = std::time::Instant::now() + Duration::from_secs(CALLBACK_TIMEOUT_SECS);
            loop {
                let now = std::time::Instant::now();
                if now >= deadline {
                    if !consumed_flag.load(std::sync::atomic::Ordering::SeqCst) {
                        tracing::warn!("OAuth 回调超时 {}s, 自动取消", CALLBACK_TIMEOUT_SECS);
                        let _ = app_for_task.emit("auth_login_timeout", CALLBACK_TIMEOUT_SECS);
                    }
                    break;
                }
                let wait = deadline.saturating_duration_since(now);
                let accept_result = tokio::time::timeout(wait, listener.accept()).await;
                match accept_result {
                    Ok(Ok((stream, _))) => {
                        let inner_c = inner.clone();
                        let app_c = app_for_task.clone();
                        let expected = expected_state.clone();
                        let consumed_c = consumed_flag.clone();
                        tokio::spawn(async move {
                            let (used, res) = handle_callback_stream(inner_c, stream, expected, app_c.clone()).await;
                            if used { consumed_c.store(true, std::sync::atomic::Ordering::SeqCst); }
                            if let Err(e) = res {
                                tracing::error!(%e, "OAuth 回调处理失败");
                                let _ = app_c.emit("auth_login_error", e.to_string());
                            }
                        });
                    }
                    Ok(Err(e)) => {
                        tracing::error!(%e, "OAuth 回调 accept 失败");
                        let _ = app_for_task.emit("auth_login_error", format!("accept 失败: {e}"));
                        break;
                    }
                    Err(_) => {
                        if !consumed_flag.load(std::sync::atomic::Ordering::SeqCst) {
                            tracing::warn!("OAuth 回调超时 {}s, 自动取消", CALLBACK_TIMEOUT_SECS);
                            let _ = app_for_task.emit("auth_login_timeout", CALLBACK_TIMEOUT_SECS);
                        }
                        break;
                    }
                }
                if consumed_flag.load(std::sync::atomic::Ordering::SeqCst) { break; }
            }
        });

        let _ = tauri_plugin_opener::OpenerExt::opener(&app).open_url(&login_url, None::<&str>);

        Ok(AuthState::LoggingIn { login_url })
    }

    async fn cancel_login(&self, app: AppHandle) -> AppResult<()> {
        // 清空 pending，使后续浏览器回调被忽略
        let mut p = self.inner.pending.lock().await;
        *p = None;
        self.clear_cached_token().await;
        // 广播匿名态，让所有窗口同步退出登录态
        let _ = app.emit("auth_state_changed", AuthState::Anonymous);
        Ok(())
    }

    async fn handle_callback(&self, code: String, state: String) -> AppResult<AuthState> {
        // 先校验再消费 pending：state 不匹配时不拿走 pending，允许后续合法回调（如：先取消再授权）
        let pending = {
            let guard = self.inner.pending.lock().await;
            let pending = guard
                .as_ref()
                .ok_or_else(|| AppError::Internal("无 pending 登录会话".into()))?
                .clone();
            if pending.state != state {
                return Err(AppError::Unauthorized("state 不匹配".into()));
            }
            if pending.started_at.elapsed().as_secs() > CALLBACK_TIMEOUT_SECS {
                drop(guard);
                let mut p = self.inner.pending.lock().await;
                *p = None;
                return Err(AppError::Unauthorized("登录会话已超时".into()));
            }
            pending
        };

        let cfg = self.cfg_snapshot().await;
        let body = serde_json::json!({
            "code": code,
            "codeVerifier": pending.code_verifier,
            "clientId": cfg.client_id,
        });
        // 真正发起换 token 后，无论成功失败都消费 pending（授权码通常为一次性）
        let token_res = self.post_json::<TokenSet>("/api/openjc/client/auth/token", body).await;
        {
            let mut p = self.inner.pending.lock().await;
            *p = None;
        }
        let resp = token_res?;
        let token_set = resp
            .data
            .ok_or_else(|| AppError::Internal("后端未返回 token".into()))?;

        // 先用 /me 验证 token 有效性，验证通过后再存储（避免无效 token 残留在 KeyChain）
        let user: UserInfo = self
            .get_json("/api/openjc/client/auth/me", &token_set.access_token)
            .await?
            .data
            .ok_or_else(|| AppError::Internal("后端未返回用户信息".into()))?;

        OAuthClient::store_tokens(&token_set.access_token, token_set.refresh_token.as_deref().unwrap_or(""), token_set.expires_in)?;
        // 写入内存缓存，确保后续调用立即可用
        self.cache_access_token(&token_set.access_token).await;

        Ok(AuthState::Authenticated { user })
    }

    async fn current_state(&self) -> AppResult<AuthState> {
        let access = OAuthClient::load_access_token()?;
        let Some(access) = access else {
            return Ok(AuthState::Anonymous);
        };
        match self
            .get_json::<UserInfo>("/api/openjc/client/auth/me", &access)
            .await
        {
            Ok(resp) => {
                let user = resp
                    .data
                    .ok_or_else(|| AppError::Internal("后端未返回用户信息".into()))?;
                Ok(AuthState::Authenticated { user })
            }
            Err(e) => {
                tracing::warn!(%e, "获取用户信息失败, 尝试 refresh");
                if self.refresh_if_needed().await.is_ok() {
                    if let Some(new_access) = OAuthClient::load_access_token()? {
                        if let Ok(resp) = self
                            .get_json::<UserInfo>("/api/openjc/client/auth/me", &new_access)
                            .await
                        {
                            if let Some(user) = resp.data {
                                return Ok(AuthState::Authenticated { user });
                            }
                        }
                    }
                }
                // 注意: 不在此清空 token —— 后端重启/网络抖动属瞬时故障,
                // 保留 refresh_token 以便下次启动恢复; 显式登出才会清除。
                Ok(AuthState::Anonymous)
            }
        }
    }

    async fn logout(&self) -> AppResult<()> {
        let cfg = self.cfg_snapshot().await;
        if let Ok(Some(refresh)) = OAuthClient::load_refresh_token() {
            let body = serde_json::json!({
                "refreshToken": refresh,
                "clientId": cfg.client_id,
            });
            let _ = self
                .inner
                .http
                .post(format!(
                    "{}/api/openjc/client/auth/logout",
                    cfg.server_base_url.trim_end_matches('/')
                ))
                .json(&body)
                .send()
                .await;
        }
        OAuthClient::clear_tokens()?;
        self.clear_cached_token().await;
        let mut p = self.inner.pending.lock().await;
        *p = None;
        Ok(())
    }

    async fn refresh_if_needed(&self) -> AppResult<()> {
        let refresh = OAuthClient::load_refresh_token()?
            .ok_or_else(|| AppError::Unauthorized("无 refresh_token".into()))?;
        let cfg = self.cfg_snapshot().await;
        let body = serde_json::json!({
            "refreshToken": refresh,
            "clientId": cfg.client_id,
        });
        let resp: ApiData<TokenSet> = self
            .post_json("/api/openjc/client/auth/refresh", body)
            .await?;
        let token_set = resp
            .data
            .ok_or_else(|| AppError::Internal("后端未返回新 token".into()))?;
        OAuthClient::store_tokens(&token_set.access_token, token_set.refresh_token.as_deref().unwrap_or(""), token_set.expires_in)?;
        // 刷新后更新内存缓存
        self.cache_access_token(&token_set.access_token).await;
        Ok(())
    }

    // ===== APP 内手机号登录（移动端，无需跳转浏览器） =====

    async fn phone_captcha_get(&self) -> AppResult<PhoneCaptchaData> {
        let resp: ApiData<AjResponse<CaptchaImage>> =
            self.get_public_json("/api/openjc/client/captcha/get").await?;
        let aj = resp
            .data
            .ok_or_else(|| AppError::Internal("后端未返回验证码".into()))?;
        if aj.rep_code != "0000" {
            return Err(AppError::Internal(
                aj.rep_msg.unwrap_or_else(|| "验证码生成失败".into()),
            ));
        }
        let img = aj.rep_data.ok_or_else(|| AppError::Internal("验证码数据缺失".into()))?;
        Ok(PhoneCaptchaData {
            original_image_base64: img.original_image_base64,
            jigsaw_image_base64: img.jigsaw_image_base64,
            token: img.token,
            secret_key: img.secret_key.unwrap_or_default(),
        })
    }

    async fn phone_captcha_check(
        &self,
        token: String,
        secret_key: String,
        x: u32,
    ) -> AppResult<String> {
        // 1) 坐标点（与前端 AJ-Captcha 习惯一致：y 固定为 5）。
        //    与官方 JS 自适应逻辑对齐：后端下发 secretKey（aj.captcha.aes-status=true）
        //    才做 AES-ECB 加密；未下发（aes 关闭）时按官方约定明文提交，
        //    否则后端会把 AES 密文当明文 JSON 解析 → "json解析异常"。
        let point_str = format!("{{\"x\":{},\"y\":5}}", x);
        let point_json = if secret_key.is_empty() {
            point_str.clone()
        } else {
            aes_ecb_base64(secret_key.as_bytes(), point_str.as_bytes())?
        };

        // 2) 提交后端校验滑块位置
        let body = serde_json::json!({ "token": token, "pointJson": point_json });
        let resp: ApiData<AjResponse<serde_json::Value>> =
            self.post_json("/api/openjc/client/captcha/check", body).await?;
        let aj = resp
            .data
            .ok_or_else(|| AppError::Internal("验证码校验无响应".into()))?;
        if aj.rep_code != "0000" {
            return Err(AppError::Internal(
                aj.rep_msg.unwrap_or_else(|| "验证失败，请重试".into()),
            ));
        }

        // 3) 校验通过后，自行计算发送短信所需的 captchaToken（captchaVerification）
        //    = token + "---" + 明文坐标（aes 开启时整体再 AES 加密），
        //    与后端二次校验 split("---") 的约定一致
        let verify_str = format!("{}---{}", token, point_str);
        if secret_key.is_empty() {
            Ok(verify_str)
        } else {
            aes_ecb_base64(secret_key.as_bytes(), verify_str.as_bytes())
        }
    }

    async fn phone_sms_send(&self, phone: String, captcha_token: String) -> AppResult<i64> {
        let body = serde_json::json!({
            "phone": phone,
            "scene": "LOGIN",
            "captchaToken": captcha_token,
        });
        let resp: ApiData<SmsCooldown> =
            self.post_json("/api/openjc/member/sms/send", body).await?;
        let cd = resp
            .data
            .ok_or_else(|| AppError::Internal("发送验证码失败".into()))?;
        Ok(cd.cooldown_seconds)
    }

    async fn phone_sms_cooldown(&self, phone: String) -> AppResult<i64> {
        let cfg = self.cfg_snapshot().await;
        let base = cfg.server_base_url.trim_end_matches('/');
        if base.is_empty() {
            return Err(AppError::Internal(
                "后端地址未配置 (server_base_url 为空). 请检查 .env 是否正确加载, 或在「设置 → 后端」中手动填写.".into(),
            ));
        }
        let url = format!(
            "{}/api/openjc/member/sms/cooldown?phone={}&scene=LOGIN",
            base, phone
        );
        let resp = self
            .inner
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("HTTP 请求失败: {e}")))?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(AppError::Internal(format!("HTTP {status}: {text}")));
        }
        let parsed: ApiData<SmsCooldown> = serde_json::from_str(&text)
            .map_err(|e| AppError::Internal(format!("响应解析失败({url}): {e}")))?;
        if parsed.code != 0 {
            return Err(AppError::Internal(
                parsed.message.unwrap_or_else(|| format!("业务失败 code={}", parsed.code)),
            ));
        }
        Ok(parsed.data.map(|d| d.cooldown_seconds).unwrap_or(0))
    }

    async fn phone_login(
        &self,
        phone: String,
        code: String,
        agreed_user_agreement_version: Option<String>,
        agreed_privacy_policy_version: Option<String>,
        project_code: String,
    ) -> AppResult<AuthState> {
        let mut body = serde_json::json!({
            "phone": phone,
            "code": code,
            "scene": "LOGIN",
            "projectCode": project_code,
        });
        if let Some(v) = agreed_user_agreement_version {
            body["agreedUserAgreementVersion"] = serde_json::Value::String(v);
        }
        if let Some(v) = agreed_privacy_policy_version {
            body["agreedPrivacyPolicyVersion"] = serde_json::Value::String(v);
        }
        let resp: ApiData<PhoneLoginVO> =
            self.post_json("/api/openjc/client/auth/phone-login", body).await?;
        let vo = resp
            .data
            .ok_or_else(|| AppError::Internal("登录失败".into()))?;
        let access_token = vo
            .access_token
            .ok_or_else(|| AppError::Internal("后端 phone-login 未返回 access_token，请联系后端确认 phone-login 已签发 token".into()))?;

        // 用 access_token 拉取完整用户信息（与 OAuth handle_callback 一致），验证通过后再持久化
        let user: UserInfo = self
            .get_json("/api/openjc/client/auth/me", &access_token)
            .await?
            .data
            .ok_or_else(|| AppError::Internal("获取用户信息失败".into()))?;

        // 手机号登录仅签发短期 access_token，无 refresh_token —— 过期后需重新走滑块→发码→登录
        OAuthClient::store_tokens(&access_token, "", vo.expires_in)?;
        self.cache_access_token(&access_token).await;

        Ok(AuthState::Authenticated { user })
    }
}

impl OAuthClient {
    /// 读取当前 access_token（供外部命令使用，如打开个人中心时传递给浏览器）
    pub fn access_token(&self) -> AppResult<Option<String>> {
        OAuthClient::load_access_token()
    }

    /// 确保拿到可用 access_token：token 缺失或即将过期时自动 refresh。
    /// refresh 失败时：旧 token 仍在有效期内则放行（网络抖动等瞬时故障）；
    /// 旧 token 已过期则清除缓存并提示重新登录（此时错误信息才是准确的）。
    pub async fn ensure_access_token(&self) -> AppResult<String> {
        // 使用内存缓存优先的读取方式，避免 KeyChain 不可靠导致刚登录就报"登录态已失效"
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let expiry = OAuthClient::load_token_expiry();
        let cached = self.load_access_token_cached().await?;
        let need_refresh = match (&cached, expiry) {
            (Some(_), Some(exp)) => now + TOKEN_REFRESH_LEAD_SECS >= exp,
            (Some(_), None) => false, // 有 token 但无过期记录，保守认为有效
            (None, _) => true,        // 无 token，需要 refresh
        };
        if !need_refresh {
            return Ok(cached.expect("已判定无需刷新时 token 必存在"));
        }
        match self.refresh_if_needed().await {
            Ok(()) => self
                .load_access_token_cached()
                .await?
                .ok_or_else(|| AppError::Unauthorized("登录态已失效, 请重新登录".into())),
            Err(e) => {
                tracing::warn!(%e, "access_token 刷新失败");
                // 旧 token 未过期：瞬时故障，先放行旧 token
                if let (Some(t), Some(exp)) = (&cached, expiry) {
                    if now < exp {
                        return Ok(t.clone());
                    }
                }
                // 旧 token 已过期或不存在：清掉缓存，避免后续反复用死 token
                self.clear_cached_token().await;
                Err(AppError::Unauthorized("登录态已失效, 请重新登录".into()))
            }
        }
    }
}

#[derive(Debug, Serialize)]
struct CallbackJsonResponse {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
}

fn cors_headers(origin: &str) -> String {
    let allow = if origin.is_empty() { "*".to_string() } else { origin.to_string() };
    format!("Access-Control-Allow-Origin: {allow}\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type, Accept\r\nAccess-Control-Max-Age: 86400\r\n")
}

fn parse_json_body(body: &str) -> (Option<String>, String, Option<String>) {
    let mut code = None;
    let mut state = String::new();
    let mut error = None;
    match serde_json::from_str::<serde_json::Value>(body) {
        Ok(serde_json::Value::Object(obj)) => {
            if let Some(v) = obj.get("error").and_then(|x| x.as_str()) { error = Some(v.to_string()); }
            if let Some(v) = obj.get("code").and_then(|x| x.as_str()) { code = Some(v.to_string()); }
            if let Some(v) = obj.get("state").and_then(|x| x.as_str()) { state = v.to_string(); }
        }
        Ok(_) => error = Some("非法请求体".into()),
        Err(e) => error = Some(format!("请求体解析失败: {e}")),
    }
    (code, state, error)
}

fn peek_header<'a>(req: &'a str, name: &str) -> Option<&'a str> {
    let lower = name.to_ascii_lowercase();
    for line in req.lines().skip(1) {
        let line = line.trim_end();
        if line.is_empty() { break; }
        let Some((k, v)) = line.split_once(':') else { continue };
        if k.trim().to_ascii_lowercase() == lower { return Some(v.trim()); }
    }
    None
}

fn peek_body_from(req: &str, content_length: Option<usize>) -> String {
    let sep = "\r\n\r\n";
    let idx = req.find(sep).map(|i| i + sep.len()).unwrap_or(req.len());
    let raw_body = &req[idx..];
    match content_length {
        Some(n) if n <= raw_body.len() => raw_body[..n].to_string(),
        _ => raw_body.to_string(),
    }
}

async fn handle_callback_stream(
    inner: Arc<Inner>,
    mut stream: tokio::net::TcpStream,
    expected_state: String,
    app: AppHandle,
) -> (bool, AppResult<()>) {
    async fn work(
        inner: Arc<Inner>,
        stream: &mut tokio::net::TcpStream,
        expected_state: String,
        app: AppHandle,
    ) -> AppResult<bool> {
        let mut buf = Vec::<u8>::with_capacity(4096);
        let mut tmp = [0u8; 4096];
        loop {
            let n = stream
                .read(&mut tmp)
                .await
                .map_err(|e| AppError::Internal(format!("读取回调请求失败: {e}")))?;
            if n == 0 { break; }
            buf.extend_from_slice(&tmp[..n]);
        let req_preview = String::from_utf8_lossy(&buf);
        if let Some(sep_idx) = req_preview.find("\r\n\r\n") {
            let headers = &req_preview[..sep_idx];
            let cl: Option<usize> = headers
                .lines()
                .filter_map(|l| l.split_once(':'))
                .find(|(k, _)| k.trim().eq_ignore_ascii_case("content-length"))
                .and_then(|(_, v)| v.trim().parse::<usize>().ok());
            let body_start = sep_idx + 4;
            let already = req_preview.len().saturating_sub(body_start);
            let need = cl.unwrap_or(0);
            if already >= need { break; }
            if need == 0 { break; }
        }
    }
    let req = String::from_utf8_lossy(&buf);
    let origin = peek_header(&req, "Origin").unwrap_or("").to_string();
    let first_line = req.lines().next().unwrap_or("");
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    let method = parts.first().copied().unwrap_or("");

        if method.eq_ignore_ascii_case("OPTIONS") {
            let resp = format!(
                "HTTP/1.1 204 No Content\r\n{access_control}Content-Length: 0\r\nConnection: close\r\n\r\n",
                access_control = cors_headers(&origin)
            );
            stream
                .write_all(resp.as_bytes())
                .await
                .map_err(|e| AppError::Internal(format!("写回调响应失败: {e}")))?;
            let _ = stream.shutdown().await;
            return Ok(false);
        }

    let cl = peek_header(&req, "Content-Length")
        .and_then(|v| v.parse::<usize>().ok());
    let accept_json = peek_header(&req, "Accept")
        .map(|v| v.to_ascii_lowercase().contains("application/json"))
        .unwrap_or(false);
    let is_post_json = method.eq_ignore_ascii_case("POST")
        && peek_header(&req, "Content-Type")
            .map(|v| v.to_ascii_lowercase().contains("application/json"))
            .unwrap_or(false);

    let (code, state, error) = if is_post_json {
        let body = peek_body_from(&req, cl);
        parse_json_body(&body)
    } else {
        parse_callback_query(first_line)
    };
    let code_value = code.clone();
    let state_value = state.clone();
    let error_value = error.clone();

    let prefer_json = accept_json || is_post_json;
    let (status_code, body) = if let Some(err) = error {
        tracing::warn!("OAuth 回调错误: {err}");
        if prefer_json {
            (400u16, serde_json::to_string(&CallbackJsonResponse { code: 400, message: err, data: None }).unwrap())
        } else {
            (500u16, format!("授权失败: {err}"))
        }
    } else if state != expected_state {
        tracing::warn!("OAuth 回调 state 不匹配");
        let msg = "state 不匹配";
        if prefer_json {
            (400, serde_json::to_string(&CallbackJsonResponse { code: 400, message: msg.into(), data: None }).unwrap())
        } else {
            (500, msg.to_string())
        }
    } else if let Some(code) = code {
        let client = OAuthClient { inner: inner.clone() };
        match client.handle_callback(code, state).await {
            Ok(new_state) => {
                let _ = app.emit("auth_state_changed", &new_state);
                if prefer_json {
                    let resp_obj = CallbackJsonResponse {
                        code: 0,
                        message: "授权完成，请回到 JC Shell 桌面端继续".into(),
                        data: None,
                    };
                    (200, serde_json::to_string(&resp_obj).unwrap())
                } else {
                    (200, CALLBACK_HTML.to_string())
                }
            }
            Err(e) => {
                let _ = app.emit("auth_login_error", e.to_string());
                let msg = e.to_string();
                if prefer_json {
                    (500, serde_json::to_string(&CallbackJsonResponse { code: 500, message: msg.clone(), data: None }).unwrap())
                } else {
                    (500, format!("授权失败: {msg}"))
                }
            }
        }
    } else {
        let msg = "缺少 code 参数";
        if prefer_json {
            (400, serde_json::to_string(&CallbackJsonResponse { code: 400, message: msg.into(), data: None }).unwrap())
        } else {
            (400, msg.to_string())
        }
    };

    let ctype = if prefer_json { "application/json; charset=utf-8" } else { "text/html; charset=utf-8" };
    let resp = format!(
        "HTTP/1.1 {status_code} OK\r\nContent-Type: {ctype}\r\nContent-Length: {len}\r\n{access_control}Connection: close\r\n\r\n{body}",
        len = body.len(),
        access_control = cors_headers(&origin)
    );
        let used = matches!((status_code, code_value.as_ref()), (200, Some(_))) && error_value.is_none() && state_value == expected_state;
        stream
            .write_all(resp.as_bytes())
            .await
            .map_err(|e| AppError::Internal(format!("写回调响应失败: {e}")))?;
        let _ = stream.shutdown().await;
        Ok(used)
    }
    match work(inner, &mut stream, expected_state, app).await {
        Ok(used) => (used, Ok(())),
        Err(e) => (false, Err(e)),
    }
}

fn parse_callback_query(request_line: &str) -> (Option<String>, String, Option<String>) {
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 {
        return (None, String::new(), Some("请求格式错误".into()));
    }
    let path = parts[1];
    let query = path.split_once('?').map(|(_, q)| q).unwrap_or("");
    let mut code = None;
    let mut state = String::new();
    let mut error = None;
    for pair in query.split('&') {
        let (k, v) = match pair.split_once('=') {
            Some((k, v)) => (k, v),
            None => continue,
        };
        match k {
            "code" => code = Some(url_decode(v)),
            "state" => state = url_decode(v),
            "error" => error = Some(url_decode(v)),
            _ => {}
        }
    }
    (code, state, error)
}

fn url_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'+' {
            out.push(' ');
            i += 1;
        } else if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(
                std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""),
                16,
            ) {
                out.push(b as char);
                i += 3;
                continue;
            }
            out.push('%');
            i += 1;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

fn random_base64url(len: usize) -> String {
    let mut buf = vec![0u8; len];
    rand::thread_rng().fill_bytes(&mut buf);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
}

fn sha256_base64url(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let hash = hasher.finalize();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hash)
}

#[derive(Debug, Deserialize)]
struct ApiData<T> {
    pub code: i32,
    pub message: Option<String>,
    pub data: Option<T>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartLoginData {
    login_url: String,
}

// ===== 手机号登录（APP 内，移动端） =====
// 后端响应统一信封：外层 R{code,message,data}；其中验证码接口 data 又包了一层
// AJ-Captcha 的 AjResponse{repCode,repMsg,repData}。

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AjResponse<T> {
    rep_code: String,
    rep_msg: Option<String>,
    rep_data: Option<T>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaptchaImage {
    original_image_base64: String,
    jigsaw_image_base64: String,
    token: String,
    /// 后端 aj.captcha.aes-status=false 时不下发（JSON 里为 null）
    secret_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SmsCooldown {
    cooldown_seconds: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PhoneLoginVO {
    id: i64,
    phone: Option<String>,
    nickname: Option<String>,
    avatar: Option<String>,
    access_token: Option<String>,
    expires_in: u64,
    token_type: Option<String>,
    new_user: Option<bool>,
    invite_bound: Option<bool>,
}

/// AES-ECB（PKCS7 填充）加密并返回标准 base64。
/// 与后端 AJ-Captcha 的 `AesUtil.encrypt` 对齐：密钥按 UTF-8 取字节，
/// 支持 16/24/32 字节（AES-128/192/256），ECB 模式，PKCS7 填充，base64 带 padding。
fn aes_ecb_base64(key: &[u8], plaintext: &[u8]) -> AppResult<String> {
    use aes::cipher::{BlockEncrypt, KeyInit};
    use aes::cipher::generic_array::GenericArray;

    let mut buf = plaintext.to_vec();
    let pad = 16 - (buf.len() % 16);
    buf.extend(std::iter::repeat(pad as u8).take(pad));

    match key.len() {
        16 => {
            let c = aes::Aes128::new(GenericArray::from_slice(key));
            for chunk in buf.chunks_mut(16) {
                c.encrypt_block(GenericArray::from_mut_slice(chunk));
            }
        }
        24 => {
            let c = aes::Aes192::new(GenericArray::from_slice(key));
            for chunk in buf.chunks_mut(16) {
                c.encrypt_block(GenericArray::from_mut_slice(chunk));
            }
        }
        32 => {
            let c = aes::Aes256::new(GenericArray::from_slice(key));
            for chunk in buf.chunks_mut(16) {
                c.encrypt_block(GenericArray::from_mut_slice(chunk));
            }
        }
        _ => return Err(AppError::Internal("验证码密钥长度非法".into())),
    }
    Ok(base64::engine::general_purpose::STANDARD.encode(&buf))
}