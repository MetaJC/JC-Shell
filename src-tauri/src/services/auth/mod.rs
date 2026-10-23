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

//! 桌面端 OAuth 认证服务（与 jc-official 后端配合的 OAuth + PKCE + 本地回调流程）
//! 作者：JC

pub mod oauth_impl;
pub use oauth_impl::*;

use crate::common::AppResult;
use serde::{Deserialize, Serialize};

/// OAuth 客户端配置（运行时可被 set_oauth_config 命令覆盖）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthClientConfig {
    /// 后端 server base url 如 http://localhost:8080
    pub server_base_url: String,
    /// 客户端标识
    pub client_id: String,
    /// 授权范围 逗号分隔
    pub scopes: String,
}

impl Default for AuthClientConfig {
    fn default() -> Self {
        Self {
            server_base_url: crate::common::OAUTH_DEFAULT_SERVER_BASE_URL.to_string(),
            client_id: crate::common::OAUTH_CLIENT_ID.to_string(),
            scopes: crate::common::OAUTH_DEFAULT_SCOPES.to_string(),
        }
    }
}

/// Token 集合（后端 Jackson 默认 camelCase，serde rename_all 统一映射）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: u64,
    pub scope: Option<String>,
}

/// 当前用户信息（后端 Jackson 默认 camelCase）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInfo {
    pub id: i64,
    pub username: Option<String>,
    pub nickname: Option<String>,
    pub email: Option<String>,
    pub avatar: Option<String>,
    pub phone: Option<String>,
}

/// 滑块验证码（AJ-Captcha）拉取结果：两张 base64 拼图 + 校验用的 token / secretKey
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhoneCaptchaData {
    pub original_image_base64: String,
    pub jigsaw_image_base64: String,
    pub token: String,
    pub secret_key: String,
}

/// 登录态（前端根据 status 字段渲染 UI）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum AuthState {
    #[serde(rename = "anonymous")]
    Anonymous,
    #[serde(rename = "logging_in")]
    LoggingIn { login_url: String },
    #[serde(rename = "authenticated")]
    Authenticated { user: UserInfo },
}

/// 认证服务抽象
#[async_trait::async_trait]
pub trait AuthService: Send + Sync {
    /// 发起登录: 启动本地回调服务 + 打开浏览器
    async fn start_login(&self, app: tauri::AppHandle) -> AppResult<AuthState>;
    /// 取消当前登录: 清除 pending 会话并广播匿名态，供用户主动中断浏览器授权流程
    async fn cancel_login(&self, app: tauri::AppHandle) -> AppResult<()>;
    /// 处理回调 (本地 HTTP server 收到 code 后内部调用)
    async fn handle_callback(&self, code: String, state: String) -> AppResult<AuthState>;
    /// 查询当前登录态
    async fn current_state(&self) -> AppResult<AuthState>;
    /// 登出: 吊销 token + 清理 KeyChain
    async fn logout(&self) -> AppResult<()>;
    /// 刷新 access_token (401 时自动调用)
    async fn refresh_if_needed(&self) -> AppResult<()>;

    // ===== APP 内手机号登录（移动端，无需跳转浏览器） =====
    /// 拉取滑块验证码（AJ-Captcha）：返回拼图底图/滑块图 + 校验用的 token/secretKey
    async fn phone_captcha_get(&self) -> AppResult<PhoneCaptchaData>;
    /// 校验滑块位置：用 secretKey 对坐标做 AES-ECB 加密并提交后端，
    /// 校验通过则返回发送短信所需的 captchaToken（即 captchaVerification）
    async fn phone_captcha_check(&self, token: String, secret_key: String, x: u32) -> AppResult<String>;
    /// 发送短信验证码（需 captchaToken）
    async fn phone_sms_send(&self, phone: String, captcha_token: String) -> AppResult<i64>;
    /// 查询发送冷却剩余秒数（用于刷新后恢复倒计时）
    async fn phone_sms_cooldown(&self, phone: String) -> AppResult<i64>;
    /// 手机号 + 验证码一步登录；成功后持久化 access_token（无 refresh_token），返回已认证态。
    /// agreed_*_version：客户端同意门禁采集的已同意版本，随登录上送后端用于强制校验/记录。
    async fn phone_login(
        &self,
        phone: String,
        code: String,
        agreed_user_agreement_version: Option<String>,
        agreed_privacy_policy_version: Option<String>,
        project_code: String,
    ) -> AppResult<AuthState>;
}
