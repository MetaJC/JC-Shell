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

//! 桌面端登录认证 Tauri Commands
//! 作者：JC

use std::sync::Arc;

use tauri::{AppHandle, Emitter};

use crate::common::AppResult;
use crate::repositories::setting_repo::SettingRepository;
use crate::services::auth::{AuthClientConfig, AuthState, AuthService, OAuthClient, PhoneCaptchaData};
use crate::AppState;

/// 持久化 OAuth 配置的 setting key
const SETTING_KEY_OAUTH: &str = "oauth_config";

/// 发起登录 (启动本地回调 + 打开浏览器)
#[tauri::command]
pub async fn start_login(
    app: AppHandle,
    oauth: tauri::State<'_, Arc<OAuthClient>>,
) -> AppResult<AuthState> {
    oauth.start_login(app).await
}

/// 查询当前登录态
#[tauri::command]
pub async fn get_auth_state(
    oauth: tauri::State<'_, Arc<OAuthClient>>,
) -> AppResult<AuthState> {
    oauth.current_state().await
}

/// 登出 (吊销 token + 清理 KeyChain)；广播匿名态，保证所有窗口登录态同步
#[tauri::command]
pub async fn logout(
    app: AppHandle,
    oauth: tauri::State<'_, Arc<OAuthClient>>,
) -> AppResult<()> {
    oauth.logout().await?;
    let _ = app.emit("auth_state_changed", AuthState::Anonymous);
    Ok(())
}

/// 取消登录：清除 pending 会话并广播匿名态
#[tauri::command]
pub async fn cancel_login(
    app: AppHandle,
    oauth: tauri::State<'_, Arc<OAuthClient>>,
) -> AppResult<()> {
    oauth.cancel_login(app).await
}

/// 读取 OAuth 配置
#[tauri::command]
pub async fn get_oauth_config(
    oauth: tauri::State<'_, Arc<OAuthClient>>,
) -> AppResult<AuthClientConfig> {
    Ok(oauth.cfg_snapshot().await)
}

/// 保存 OAuth 配置 (内存 + 持久化到 app_setting 表)
#[tauri::command]
pub async fn set_oauth_config(
    cfg: AuthClientConfig,
    oauth: tauri::State<'_, Arc<OAuthClient>>,
    state: tauri::State<'_, Arc<AppState>>,
) -> AppResult<()> {
    oauth.update_config(cfg.clone()).await;
    let json = serde_json::to_string(&cfg)?;
    let repo = SettingRepository::new(&state.db);
    repo.upsert(SETTING_KEY_OAUTH, &json).await
}

// ===== APP 内手机号登录（移动端，无需跳转浏览器） =====

/// 拉取滑块验证码（AJ-Captcha）
#[tauri::command]
pub async fn phone_captcha_get(
    oauth: tauri::State<'_, Arc<OAuthClient>>,
) -> AppResult<PhoneCaptchaData> {
    oauth.phone_captcha_get().await
}

/// 校验滑块位置：返回发送短信所需的 captchaToken
#[tauri::command]
pub async fn phone_captcha_check(
    token: String,
    secret_key: String,
    x: u32,
    oauth: tauri::State<'_, Arc<OAuthClient>>,
) -> AppResult<String> {
    oauth.phone_captcha_check(token, secret_key, x).await
}

/// 发送短信验证码（需 captchaToken）
#[tauri::command]
pub async fn phone_sms_send(
    phone: String,
    captcha_token: String,
    oauth: tauri::State<'_, Arc<OAuthClient>>,
) -> AppResult<i64> {
    oauth.phone_sms_send(phone, captcha_token).await
}

/// 查询发送冷却剩余秒数（用于恢复倒计时）
#[tauri::command]
pub async fn phone_sms_cooldown(
    phone: String,
    oauth: tauri::State<'_, Arc<OAuthClient>>,
) -> AppResult<i64> {
    oauth.phone_sms_cooldown(phone).await
}

/// 手机号 + 验证码一步登录
#[tauri::command]
pub async fn phone_login(
    phone: String,
    code: String,
    agreed_user_agreement_version: Option<String>,
    agreed_privacy_policy_version: Option<String>,
    oauth: tauri::State<'_, Arc<OAuthClient>>,
) -> AppResult<AuthState> {
    oauth
        .phone_login(
            phone,
            code,
            agreed_user_agreement_version,
            agreed_privacy_policy_version,
            crate::services::telemetry::PROJECT_CODE.to_string(),
        )
        .await
}