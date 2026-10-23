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

//! JC Shell 自动更新服务
//! 作者：JC

pub mod version;

use std::collections::HashSet;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::common::{AppError, AppResult};

/// 下载进度事件载荷（前端 stores/update.ts 的 update-download-progress 监听对齐）
#[derive(Debug, Clone, Serialize)]
struct ProgressPayload {
    downloaded: u64,
    total: u64,
}

/// 资源信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetInfo {
    pub platform: String,
    #[serde(rename = "fileName")]
    pub file_name: String,
    #[serde(rename = "fileSize")]
    pub file_size: Option<u64>,
    #[serde(rename = "fileSha256")]
    pub file_sha256: Option<String>,
    #[serde(rename = "downloadUrls")]
    pub download_urls: Vec<String>,
}

/// 产品信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductInfo {
    pub key: String,
    pub name: String,
}

/// 最新版本信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatestVersionInfo {
    #[serde(rename = "versionCode")]
    pub version_code: String,
    #[serde(rename = "versionName")]
    pub version_name: String,
    pub changelog: Option<String>,
    #[serde(rename = "releaseNotesUrl")]
    pub release_notes_url: Option<String>,
    #[serde(rename = "isMandatory")]
    pub is_mandatory: Option<bool>,
    #[serde(rename = "minVersion")]
    pub min_version: Option<String>,
    #[serde(rename = "publishTime")]
    pub publish_time: Option<String>,
    pub assets: Vec<AssetInfo>,
}

/// 检查更新响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckUpdateResponse {
    #[serde(rename = "hasUpdate")]
    pub has_update: bool,
    pub product: Option<ProductInfo>,
    pub latest: Option<LatestVersionInfo>,
}

/// 通用 API 响应包装（对齐后端 cc.openjc.website.common.R<T>）
/// 成功码 = 0（不是 200）；message 字段是 String（不是 Option）。
#[derive(Debug, Deserialize)]
struct ApiResponse<T> {
    code: i32,
    message: String,
    data: Option<T>,
}

/// 下载进度
#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgress {
    pub bytes_downloaded: u64,
    pub total_bytes: Option<u64>,
    pub progress_percent: f64,
}

/// 更新状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum UpdateStatus {
    Idle,
    Checking,
    NoUpdate,
    Available,
    Downloading,
    Downloaded,
    Error,
}

/// 下载任务信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadInfo {
    pub file_path: String,
    pub file_size: u64,
    pub file_sha256: Option<String>,
    pub version_code: String,
}

/// UpdateService
/// Clone 为浅拷贝：内部状态均为 Arc，command 层可克隆一份后释放 state 锁，
/// 避免下载期间长时间持锁阻塞 cancel / get_download_info 等其他命令。
#[derive(Clone)]
pub struct UpdateService {
    http_client: Client,
    server_base_url: String,
    product_key: String,
    status: Arc<Mutex<UpdateStatus>>,
    download_info: Arc<Mutex<Option<DownloadInfo>>>,
    cancel_flag: Arc<Mutex<bool>>,
    /// 用于向前端 emit 下载进度事件（可选，测试场景可缺省）
    app_handle: Option<tauri::AppHandle>,
    /// 最近一次 check_update 的更新渠道（上报更新量时回传给后台）
    last_channel: Arc<Mutex<Option<String>>>,
    /// 已上报过更新量的目标版本（持久化：避免重复点击/重启后重复累加导致后台数字虚高）
    reported_versions: Arc<Mutex<HashSet<String>>>,
    /// 登录态（可选）：更新上报带上会员 token，后台才能把更新记录关联到用户
    oauth: Option<Arc<crate::services::auth::OAuthClient>>,
}

impl UpdateService {
    pub fn new(server_base_url: String, product_key: String) -> Self {
        Self {
            // 注意：不能设置总超时（ClientBuilder::timeout 作用于整个请求含响应体读取），
            // 上百 MB 安装包下载耗时远超该值会导致下载必然中断；仅保留连接超时。
            http_client: crate::common::http::apply_client_defaults(
                Client::builder().connect_timeout(std::time::Duration::from_secs(15)),
            )
            .build()
            .unwrap_or_default(),
            server_base_url,
            product_key,
            status: Arc::new(Mutex::new(UpdateStatus::Idle)),
            // 启动时恢复上次已下载好的安装包信息：
            // 「新版本已就绪」提示必须跨重启保留（用户没点之前一直是当前版本 + 提示常驻）
            download_info: Arc::new(Mutex::new(load_download_info())),
            cancel_flag: Arc::new(Mutex::new(false)),
            app_handle: None,
            last_channel: Arc::new(Mutex::new(None)),
            reported_versions: Arc::new(Mutex::new(Self::load_reported_versions())),
            oauth: None,
        }
    }

    /// 注入登录态（setup 阶段）：用于更新上报携带会员 token（未登录则为 None，不影响上报）
    pub fn set_oauth(&mut self, oauth: Arc<crate::services::auth::OAuthClient>) {
        self.oauth = Some(oauth);
    }

    /// 注入 AppHandle（setup 阶段），用于 emit 下载进度事件
    pub fn set_app_handle(&mut self, handle: tauri::AppHandle) {
        self.app_handle = Some(handle);
    }

    /// 原地更新后端地址（不重建服务，避免清空已下载包信息）
    pub fn set_server_base_url(&mut self, url: &str) {
        if self.server_base_url != url {
            self.server_base_url = url.to_string();
        }
    }

    pub async fn get_status(&self) -> UpdateStatus {
        self.status.lock().await.clone()
    }

    pub async fn get_download_info(&self) -> Option<DownloadInfo> {
        self.download_info.lock().await.clone()
    }

    async fn set_status(&self, status: UpdateStatus) {
        *self.status.lock().await = status;
    }

    pub async fn check_update(
        &self,
        current_version: &str,
        channel: Option<&str>,
        platform: Option<&str>,
    ) -> AppResult<CheckUpdateResponse> {
        self.set_status(UpdateStatus::Checking).await;
        // 记住渠道，供后续「点击更新」上报时回传（后台按渠道统计更新量）
        *self.last_channel.lock().await = channel.map(|c| c.to_string());

        let mut params = vec![("currentVersion", current_version)];
        if let Some(ch) = channel {
            params.push(("channel", ch));
        }
        if let Some(pl) = platform {
            params.push(("platform", pl));
        }

        let url = format!(
            "{}/api/openjc/client/projects/{}/versions/latest",
            self.server_base_url, self.product_key
        );

        // 检查接口单独加 30s 总超时（下载不加，见 http_client 构造注释）
        let resp = tokio::time::timeout(std::time::Duration::from_secs(30), self.http_client
            .get(&url)
            .query(&params)
            .send())
            .await
            .map_err(|_| AppError::Http("检查更新超时(30s)".to_string()))?
            .map_err(|e| {
                // 底层错误（含后端地址等内部信息）只在后端日志记录，不返回给客户端
                tracing::warn!("检查更新请求失败: {e}");
                AppError::Http("检查更新失败，请检查网络后重试".to_string())
            })?;

        if !resp.status().is_success() {
            self.set_status(UpdateStatus::Error).await;
            return Err(AppError::Http(format!("HTTP {}", resp.status())));
        }

        let api_resp: ApiResponse<CheckUpdateResponse> = resp
            .json()
            .await
            .map_err(|e| AppError::Http(format!("解析响应失败: {e}")))?;

        if api_resp.code != 0 {
            self.set_status(UpdateStatus::Error).await;
            return Err(AppError::Http(if api_resp.message.is_empty() { format!("业务码 {}", api_resp.code) } else { api_resp.message }));
        }

        let result = api_resp.data.unwrap_or(CheckUpdateResponse {
            has_update: false,
            product: None,
            latest: None,
        });

        if result.has_update {
            self.set_status(UpdateStatus::Available).await;
        } else {
            self.set_status(UpdateStatus::NoUpdate).await;
        }

        Ok(result)
    }

    pub async fn download_update(
        &self,
        asset: &AssetInfo,
        version_code: &str,
    ) -> AppResult<DownloadProgress> {
        // 用户点击「更新」→ 上报一次更新量（后台「更新量」统计来源）。
        // 后台上报是旁路：异步发出、失败/超时均不影响下载与安装。
        {
            let svc = self.clone();
            let target = version_code.to_string();
            let platform = asset.platform.clone();
            tauri::async_runtime::spawn(async move {
                svc.report_update(&target, env!("CARGO_PKG_VERSION"), Some(&platform)).await;
            });
        }
        self.set_status(UpdateStatus::Downloading).await;
        *self.cancel_flag.lock().await = false;
        // 开始新下载：清掉上一份「已就绪」记录，避免残留旧版本的待安装提示
        clear_download_info();
        *self.download_info.lock().await = None;

        let download_dir = self.get_update_dir()?;
        std::fs::create_dir_all(&download_dir)?;
        let file_path = download_dir.join(&asset.file_name);
        let mut downloaded: u64 = 0;
        let total_size = asset.file_size;

        let mut last_error = None;
        for url in &asset.download_urls {
            if *self.cancel_flag.lock().await {
                self.set_status(UpdateStatus::Idle).await;
                return Err(AppError::Cancelled("下载已取消".to_string()));
            }
            match self.try_download_single(url, &file_path, &mut downloaded, total_size).await {
                Ok(()) => {
                    let info = DownloadInfo {
                        file_path: file_path.to_string_lossy().to_string(),
                        file_size: downloaded,
                        file_sha256: asset.file_sha256.clone(),
                        version_code: version_code.to_string(),
                    };
                    save_download_info(&info);
                    *self.download_info.lock().await = Some(info);
                    self.set_status(UpdateStatus::Downloaded).await;
                    return Ok(DownloadProgress {
                        bytes_downloaded: downloaded,
                        total_bytes: total_size,
                        progress_percent: 100.0,
                    });
                }
                Err(e) => {
                    last_error = Some(e);
                    downloaded = 0;
                    continue;
                }
            }
        }

        self.set_status(UpdateStatus::Error).await;
        Err(last_error.unwrap_or_else(|| AppError::Http("所有下载源均失败".to_string())))
    }

    async fn try_download_single(
        &self,
        url: &str,
        file_path: &PathBuf,
        downloaded: &mut u64,
        fallback_total: Option<u64>,
    ) -> AppResult<()> {
        // 后端返回的 downloadUrl 可能是相对路径（以 / 开头），此时拼接 server_base_url
        let full_url = if url.starts_with('/') {
            format!("{}{}", self.server_base_url.trim_end_matches('/'), url)
        } else {
            url.to_string()
        };
        let resp = self.http_client
            .get(&full_url)
            .send()
            .await
            .map_err(|e| AppError::Http(format!("下载失败: {e}")))?;

        if !resp.status().is_success() {
            return Err(AppError::Http(format!("HTTP {}", resp.status())));
        }

        let total = resp
            .content_length()
            .or(fallback_total)
            .unwrap_or(0);

        let mut file = std::fs::File::create(file_path)?;
        let mut stream = resp.bytes_stream();
        let mut last_emit = Instant::now();

        // 起始即发一次（0%），确保前端进度条立刻出现
        self.emit_progress(0, total);

        while let Some(chunk) = stream.next().await {
            if *self.cancel_flag.lock().await {
                drop(file);
                let _ = std::fs::remove_file(file_path);
                return Err(AppError::Cancelled("下载已取消".to_string()));
            }
            let chunk = chunk.map_err(|e| AppError::Http(format!("读取失败: {e}")))?;
            file.write_all(&chunk)?;
            *downloaded += chunk.len() as u64;
            // 节流：约 250ms 一次，避免事件风暴拖慢下载
            if last_emit.elapsed().as_millis() >= 250 {
                self.emit_progress(*downloaded, total);
                last_emit = Instant::now();
            }
        }
        // 收尾 100%
        self.emit_progress(*downloaded, total.max(*downloaded));
        Ok(())
    }

    /// 向前端 emit 下载进度（失败仅记日志，不影响下载主流程）
    fn emit_progress(&self, downloaded: u64, total: u64) {
        use tauri::Emitter;
        if let Some(h) = &self.app_handle {
            if let Err(e) = h.emit("update-download-progress", ProgressPayload { downloaded, total }) {
                tracing::warn!("发送下载进度事件失败: {e}");
            }
        }
    }

    pub async fn cancel_download(&self) {
        *self.cancel_flag.lock().await = true;
    }

    pub async fn silent_download(
        &self,
        asset: &AssetInfo,
        version_code: &str,
    ) -> AppResult<()> {
        let p = self.download_update(asset, version_code).await?;
        if p.progress_percent >= 100.0 {
            Ok(())
        } else {
            Err(AppError::Http("下载未完成".to_string()))
        }
    }

    // ===== 更新量上报（后台「更新量」统计） =====

    /// 已上报版本的持久化文件（与更新包同目录：卸载重装视为新客户端，重新计数）
    fn reported_versions_path() -> PathBuf {
        crate::common::paths::app_data_dir()
            .join("updates")
            .join("reported-updates.json")
    }

    fn load_reported_versions() -> HashSet<String> {
        match std::fs::read_to_string(Self::reported_versions_path()) {
            Ok(s) => serde_json::from_str::<Vec<String>>(&s)
                .ok()
                .map(|v| v.into_iter().collect())
                .unwrap_or_default(),
            Err(_) => HashSet::new(),
        }
    }

    fn save_reported_versions(set: &HashSet<String>) {
        let path = Self::reported_versions_path();
        if let Some(dir) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(dir) {
                tracing::warn!("创建更新目录失败: {e}");
                return;
            }
        }
        match serde_json::to_string(set) {
            Ok(s) => {
                if let Err(e) = std::fs::write(&path, s) {
                    tracing::warn!("写入已上报版本记录失败: {e}");
                }
            }
            Err(e) => tracing::warn!("序列化已上报版本记录失败: {e}"),
        }
    }

    /**
     * 上报「用户点击更新」到后端 —— 后台「更新量」的唯一来源。
     * 后端落库三处：openjc_version.update_count +1、openjc_version_asset.update_count +1、
     * openjc_software_stat(stat_type='update') 事件明细（仪表盘按时间/版本统计）。
     *
     * 去重：同一客户端对同一目标版本只上报一次（持久化到磁盘，跨重启生效），
     * 避免用户重复点击或反复重启导致后台数字虚高；上报失败会撤回标记，允许下次重试。
     */
    pub async fn report_update(&self, target_version: &str, from_version: &str, platform: Option<&str>) {
        {
            let mut set = self.reported_versions.lock().await;
            if set.contains(target_version) {
                tracing::debug!("更新量已上报过，跳过: target={}", target_version);
                return;
            }
            set.insert(target_version.to_string());
            Self::save_reported_versions(&set);
        }

        let url = format!(
            "{}/api/openjc/client/projects/{}/versions/report-update",
            self.server_base_url, self.product_key
        );
        let mut body = serde_json::Map::new();
        body.insert("fromVersion".to_string(), serde_json::json!(from_version));
        if let Some(p) = platform {
            body.insert("platform".to_string(), serde_json::json!(p));
        }
        if let Some(ch) = self.last_channel.lock().await.as_deref() {
            body.insert("channel".to_string(), serde_json::json!(ch));
        }
        // 明细扩展：设备ID / 系统版本（来自遥测 start 时的进程级快照）+ 登录 token
        // 后台据此统计「从哪个版本更新到哪个版本」「哪个设备/用户更新的」。
        let (cid, osv) = crate::services::telemetry::get_client_info().await;
        if let Some(c) = cid {
            body.insert("clientId".to_string(), serde_json::json!(c));
        }
        if let Some(o) = osv {
            body.insert("osVersion".to_string(), serde_json::json!(o));
        }
        if let Some(oauth) = &self.oauth {
            if let Ok(Some(token)) = oauth.access_token() {
                body.insert("token".to_string(), serde_json::json!(token));
            }
        }

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.http_client.post(&url).json(&body).send(),
        )
        .await;

        match result {
            Ok(Ok(resp)) => tracing::info!(
                "更新量上报成功: target={} from={} platform={:?} status={}",
                target_version,
                from_version,
                platform,
                resp.status()
            ),
            Ok(Err(e)) => {
                tracing::warn!("更新量上报失败(不影响更新): {e}");
                self.rollback_reported(target_version).await;
            }
            Err(_) => {
                tracing::warn!("更新量上报超时(不影响更新): target={}", target_version);
                self.rollback_reported(target_version).await;
            }
        }
    }

    /// 撤回去重标记（上报失败时调用，允许下次重试）
    async fn rollback_reported(&self, target_version: &str) {
        let mut set = self.reported_versions.lock().await;
        set.remove(target_version);
        Self::save_reported_versions(&set);
    }

    fn get_update_dir(&self) -> AppResult<PathBuf> {
        Ok(update_dir())
    }
}

/// 更新包下载目录（数据目录下，避开安装目录被卸载清空的问题）
fn update_dir() -> PathBuf {
    crate::common::paths::app_data_dir().join("updates")
}

/// 待安装信息落盘位置（用于跨重启保留「新版本已就绪」状态）
fn download_meta_path() -> PathBuf {
    update_dir().join("download-info.json")
}

/// 读取上次下载好的安装包信息；安装包文件已不存在（被清理/已安装过）时视为无效
fn load_download_info() -> Option<DownloadInfo> {
    let raw = std::fs::read_to_string(download_meta_path()).ok()?;
    let info: DownloadInfo = serde_json::from_str(&raw).ok()?;
    if !PathBuf::from(&info.file_path).is_file() {
        return None;
    }
    Some(info)
}

fn save_download_info(info: &DownloadInfo) {
    let dir = update_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    if let Ok(raw) = serde_json::to_string(info) {
        let _ = std::fs::write(download_meta_path(), raw);
    }
}

fn clear_download_info() {
    let _ = std::fs::remove_file(download_meta_path());
}
