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

//! 自动更新相关 Tauri Commands
//! 作者：JC

use std::sync::Arc;

use tauri::{Manager, State};
use tokio::sync::Mutex;

use crate::common::{AppError, AppResult, OAUTH_DEFAULT_SERVER_BASE_URL};
use crate::services::auth::OAuthClient;
use crate::services::update::{
    AssetInfo, CheckUpdateResponse, DownloadInfo, DownloadProgress, UpdateService, UpdateStatus,
};

/// 全局 UpdateService 状态
pub struct UpdateServiceState(pub Arc<Mutex<UpdateService>>);

impl UpdateServiceState {
    pub fn new(service: UpdateService) -> Self {
        Self(Arc::new(Mutex::new(service)))
    }
}

/// 检查更新
#[tauri::command]
pub async fn check_update(
    state: State<'_, UpdateServiceState>,
    oauth: State<'_, Arc<OAuthClient>>,
    current_version: String,
    channel: Option<String>,
    platform: Option<String>,
) -> AppResult<CheckUpdateResponse> {
    // 防御：编译期常量 (tauri.conf.json#version) 与前端传入的 currentVersion 必须一致，
    // 否则版本比对会误判（已踩坑：package.json 单独改了 0.2.0 但 tauri.conf.json 是 0.1.0，
    // 前端 currentVersion 来自 Vite 注入 = 0.2.0，Rust 端 app 包内 version = 0.1.0，
    // 主站发布 0.2.0 时客户端永远查不到新版本）。
    // 此处只记录警告，不阻断（开发态可能临时不一致）。
    const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
    if APP_VERSION != current_version {
        tracing::warn!(
            "版本号不一致：Rust 包内 (tauri.conf.json) = {}，前端传入 (package.json) = {}。发版时两处必须同步改。",
            APP_VERSION,
            current_version
        );
    }

    let cfg = oauth.cfg_snapshot().await;
    let base_url = if cfg.server_base_url.trim().is_empty() {
        if OAUTH_DEFAULT_SERVER_BASE_URL.trim().is_empty() {
            return Err(AppError::Internal(
                "后端地址未配置 (server_base_url 为空). 请检查 .env 是否正确加载, 或在「设置 → 后端」中手动填写.".into(),
            ));
        }
        OAUTH_DEFAULT_SERVER_BASE_URL.to_string()
    } else {
        cfg.server_base_url
    };
    // 原地更新后端地址（不重建服务：重建会清空 download_info，导致下载完成后安装报“没有可用的安装包”），
    // 然后克隆一份（内部状态均为 Arc）释放 state 锁，避免网络请求期间阻塞其他命令。
    let svc = {
        let mut guard = state.0.lock().await;
        guard.set_server_base_url(&base_url);
        guard.clone()
    };
    tracing::info!(
        "检查更新开始: server={} currentVersion={} channel={:?} platform={:?}",
        base_url,
        current_version,
        channel,
        platform
    );
    match svc.check_update(&current_version, channel.as_deref(), platform.as_deref()).await {
        Ok(resp) => {
            tracing::info!(
                "检查更新完成: hasUpdate={} latest={}",
                resp.has_update,
                resp.latest.as_ref().map(|l| l.version_code.as_str()).unwrap_or("-")
            );
            Ok(resp)
        }
        Err(e) => {
            tracing::warn!("检查更新失败: {e}");
            Err(e)
        }
    }
}

/// 获取当前更新状态
#[tauri::command]
pub async fn get_update_status(
    state: State<'_, UpdateServiceState>,
) -> AppResult<UpdateStatus> {
    let svc = state.0.lock().await;
    Ok(svc.get_status().await)
}

/// 下载更新包（克隆服务后释放 state 锁，下载期间不阻塞 cancel/查询等命令）
#[tauri::command]
pub async fn download_update(
    state: State<'_, UpdateServiceState>,
    asset: AssetInfo,
    version_code: String,
) -> AppResult<DownloadProgress> {
    let svc = state.0.lock().await.clone();
    svc.download_update(&asset, &version_code).await
}

/// 静默下载更新（同上，下载期间不持 state 锁）
#[tauri::command]
pub async fn silent_download_update(
    state: State<'_, UpdateServiceState>,
    asset: AssetInfo,
    version_code: String,
) -> AppResult<()> {
    let svc = state.0.lock().await.clone();
    svc.silent_download(&asset, &version_code).await
}

/// 取消下载
#[tauri::command]
pub async fn cancel_update_download(
    state: State<'_, UpdateServiceState>,
) -> AppResult<()> {
    let svc = state.0.lock().await;
    svc.cancel_download().await;
    Ok(())
}

/// 获取下载信息
#[tauri::command]
pub async fn get_download_info(
    state: State<'_, UpdateServiceState>,
) -> AppResult<Option<DownloadInfo>> {
    let svc = state.0.lock().await;
    Ok(svc.get_download_info().await)
}

/// 启动安装程序并退出
#[tauri::command]
pub async fn install_update(
    app: tauri::AppHandle,
    state: State<'_, UpdateServiceState>,
) -> AppResult<()> {
    let svc = state.0.lock().await;
    let info = svc.get_download_info().await
        .ok_or_else(|| AppError::NotFound("没有可用的安装包".to_string()))?;
    let path = info.file_path.clone();

    #[cfg(windows)]
    {
        // 静默安装参数（依据 Tauri 官方 NSIS 模板 installer.nsi）：
        //   NSIS(.exe) → /S /UPDATE /R
        //     /S      完全静默，不显示任何页面
        //     /UPDATE 更新模式：复用旧安装、跳过「先卸载」流程、不动快捷方式与用户数据、跳过 WebView2
        //     /R      安装成功后自动以当前用户身份重启应用（助手脚本也会重启，单实例插件负责去重）
        //   .msi       → msiexec /i <file> /qn /norestart（无界面）
        //   .bat/.cmd  → cmd /c
        let lower = path.to_lowercase();
        let is_msi = lower.ends_with(".msi");
        let is_script = lower.ends_with(".bat") || lower.ends_with(".cmd");
        let (program, args): (String, Vec<String>) = if is_msi {
            (
                "msiexec".to_string(),
                vec![
                    "/i".to_string(),
                    path.clone(),
                    "/qn".to_string(),
                    "/norestart".to_string(),
                ],
            )
        } else if is_script {
            ("cmd".to_string(), vec!["/c".to_string(), path.clone()])
        } else {
            (
                path.clone(),
                vec!["/S".to_string(), "/UPDATE".to_string(), "/R".to_string()],
            )
        };

        // 统一走助手脚本：**先等本进程真正退出，再静默安装，最后拉起应用**。
        // 关键点：正式版安装时，被替换的 exe 就是正在运行的这个进程；若安装程序在进程退出前动手，
        // 会因文件锁失败（开发态跑的是 target/debug 下另一个 exe，没有这个冲突，所以"本地 OK、线上挂"）。
        match spawn_update_helper(&program, &args) {
            Ok(bat) => tracing::info!("已拉起更新助手（等待本进程退出后静默安装）: {bat}"),
            Err(e) => {
                tracing::warn!("更新助手不可用，回退为直接拉起安装程序 {path}: {e}");
                match spawn_detached(&program, &args) {
                    Ok(_) => tracing::info!("已拉起安装程序(回退): {}", path),
                    Err(e2) => tracing::warn!("拉起安装程序失败 {path}: {e2}"),
                }
            }
        }
        // 留出时间让助手脱离并启动，避免父进程退出时被一起终止
        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(&path).spawn()?;
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(&path).spawn()?;
    }

    app.exit(0);
    Ok(())
}

/// 以「脱离父进程」的方式拉起一个进程（主进程随后 exit 不会把它一起带走）
#[cfg(windows)]
fn spawn_detached(program: &str, args: &[String]) -> AppResult<()> {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    std::process::Command::new(program)
        .args(args)
        .creation_flags(DETACHED_PROCESS)
        .spawn()
        .map_err(|e| AppError::Internal(format!("启动 {program} 失败: {e}")))?;
    Ok(())
}

/// 生成并拉起「等本进程退出 → 静默安装 → 重启应用」的 PowerShell 助手，返回脚本路径。
///
/// 为什么必须等本进程退出：
///  - 正式版更新时，安装程序要替换的 exe 正是**当前正在运行的这个进程**；Windows 会锁住正在运行的
///    镜像文件，安装程序在进程退出前动手就会失败（开发态跑的是 `target/debug` 下的另一个 exe，
///    没有这个冲突，所以表现为「本地能过、线上挂」）；
///  - 主进程马上要 `exit(0)`，自己没法等，所以交给脱离父进程的 PowerShell 脚本等本进程消失。
///
/// 使用 PowerShell 而非 .bat 的原因：
///  - `-WindowStyle Hidden` 能真正隐藏黑窗口；
///  - `Start-Process -Wait -PassThru` 可拿到安装程序精确退出码；
///  - 日志里能记录旧进程是否还在、安装是否失败、重启是否成功。
#[cfg(windows)]
fn spawn_update_helper(program: &str, args: &[String]) -> AppResult<String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let app_exe = std::env::current_exe()
        .map_err(|e| AppError::Internal(format!("获取当前程序路径失败: {e}")))?;
    let pid = std::process::id();

    let ps_path = std::env::temp_dir().join(format!("jc-shell-update-{pid}.ps1"));
    let log_path = std::env::temp_dir().join("jc-shell-update.log");

    let ps_program = ps_escape(program);
    let ps_args = args
        .iter()
        .map(|s| ps_escape(s))
        .collect::<Vec<_>>()
        .join(",");
    let ps_app = ps_escape(&app_exe.to_string_lossy());
    let ps_log = ps_escape(&log_path.to_string_lossy());

    // 注意：脚本内容必须全部为 ASCII。
    // Windows PowerShell 5.1 对「无 BOM 的 UTF-8」会按系统 ANSI(GBK) 解析，
    // 脚本里若含中文会导致语法错误、一行都不执行（表现为：脚本生成了但日志为空）。
    let content = format!(
        "$ErrorActionPreference = 'Stop'\n\
$Log = {log}\n\
$PidToWait = {pid}\n\
$Program = {program}\n\
$InstallerArgs = @({args})\n\
$App = {app}\n\
\n\
function Write-Log($msg) {{\n\
    Add-Content -Path $Log -Value \"$((Get-Date -Format 'yyyy-MM-dd HH:mm:ss')) $msg\" -Encoding UTF8 -ErrorAction SilentlyContinue\n\
}}\n\
\n\
Write-Log \"helper start (wait pid $PidToWait)\"\n\
\n\
# Wait for the old app process to exit. The update flow already called app.exit(0);\n\
# normally it is gone within 1s. Give 8s grace, then force-kill it so the exe file\n\
# lock is released and the installer can replace the running binary.\n\
$grace = 8\n\
$sw = [System.Diagnostics.Stopwatch]::StartNew()\n\
while ($sw.Elapsed.TotalSeconds -lt $grace) {{\n\
    $proc = Get-Process -Id $PidToWait -ErrorAction SilentlyContinue\n\
    if (-not $proc) {{\n\
        Write-Log \"old process $PidToWait has exited\"\n\
        break\n\
    }}\n\
    Start-Sleep -Milliseconds 300\n\
}}\n\
$proc = Get-Process -Id $PidToWait -ErrorAction SilentlyContinue\n\
if ($proc) {{\n\
    Write-Log \"WARN: old process $PidToWait still alive after ${{grace}}s, force killing\"\n\
    try {{ Stop-Process -Id $PidToWait -Force -ErrorAction Stop }} catch {{ Write-Log \"force kill failed: $_\" }}\n\
    Start-Sleep -Milliseconds 800\n\
}}\n\
\n\
Write-Log \"run installer: $Program $InstallerArgs\"\n\
try {{\n\
    $p = Start-Process -FilePath $Program -ArgumentList $InstallerArgs -Wait -PassThru\n\
    Write-Log \"installer exit code=$($p.ExitCode)\"\n\
}} catch {{\n\
    Write-Log \"installer failed: $_\"\n\
}}\n\
\n\
try {{\n\
    Start-Process -FilePath $App\n\
    Write-Log \"app relaunched: $App\"\n\
}} catch {{\n\
    Write-Log \"app relaunch failed: $_\"\n\
}}\n\
\n\
Remove-Item -Path $PSCommandPath -Force -ErrorAction SilentlyContinue\n",
        log = ps_log,
        pid = pid,
        program = ps_program,
        args = ps_args,
        app = ps_app
    );

    std::fs::write(&ps_path, content)
        .map_err(|e| AppError::Internal(format!("写入更新助手脚本失败: {e}")))?;

    // CREATE_NO_WINDOW + -WindowStyle Hidden：助手无可见窗口
    std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            &ps_path.to_string_lossy(),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| AppError::Internal(format!("拉起更新助手失败: {e}")))?;
    Ok(ps_path.to_string_lossy().to_string())
}

/// 把字符串转义成 PowerShell 单引号字符串（路径中的单引号需转义为两个单引号）。
#[cfg(windows)]
fn ps_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// 当前安装级别：`user`(用户级) / `system`(系统级) / `unknown`(无法判断，如开发态)。
///
/// 用于挑选与「当前安装」一致的安装包：用户级装的程序若拿系统级包升级，
/// 会装成第二份或被要求先卸载；反之亦然。
#[tauri::command]
pub fn get_install_scope() -> String {
    let exe = match std::env::current_exe() {
        Ok(p) => p.to_string_lossy().replace('\\', "/").to_lowercase(),
        Err(_) => return "unknown".to_string(),
    };
    // Tauri NSIS：currentUser → %LOCALAPPDATA%\Programs\...；perMachine → C:\Program Files\...
    if exe.contains("/program files/") || exe.contains("/program files (x86)/") {
        return "system".to_string();
    }
    if exe.contains("/appdata/local/programs/") {
        return "user".to_string();
    }
    "unknown".to_string()
}

/// 真正退出整个 Tauri App（不仅销毁 WebviewWindow）
/// 用于「设置-退出」或关闭按钮确认后调用，修复 win.destroy() 进程残留的 bug。
#[tauri::command]
pub async fn app_exit(app: tauri::AppHandle) -> AppResult<()> {
    app.exit(0);
    Ok(())
}

/// 隐藏主窗口（精准：固定 label="main"），不依赖前端 webview context。
/// 用于「设置-关闭按钮最小化到托盘」：在 onCloseRequested handler 里调，
/// 走 AppHandle.get_webview_window("main") 拿真窗口并 hide，避开前端 getCurrentWindow
/// 在多 webview 场景命中错窗口的问题。
#[tauri::command]
pub async fn hide_main_window(app: tauri::AppHandle) -> AppResult<()> {
    let w = app
        .get_webview_window("main")
        .ok_or_else(|| AppError::Ssh("找不到主窗口 main".to_string()))?;
    w.hide()
        .map_err(|e| AppError::Ssh(format!("隐藏主窗口失败: {e}")))?;
    Ok(())
}