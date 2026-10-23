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

//! 跨软件会话导入：文件/目录选择、文本读取、已安装 SSH 客户端探测
//! 作者：JC
//!
//! 设计取舍：解析逻辑全部放在前端 TS（各客户端格式版本差异大、迭代快，前端更好维护），
//! 这里只提供"webview 做不到"的三件原语：
//!   1. 选文件/选目录（rfd 原生对话框）；
//!   2. 读任意本地文本（webview 无权读取，且 Xshell .xsh 常是 UTF-16LE，需要解码）；
//!   3. 列目录 + 探测已安装客户端（依赖平台路径，放 Rust 更合适）。

use std::path::{Path, PathBuf};

use tauri::command;

use crate::common::{AppError, AppResult};

/// 导入时读取的最大文件体积（8MB）：正常会话文件远小于此，超限几乎必是选错文件
const MAX_IMPORT_BYTES: usize = 8 * 1024 * 1024;
/// 列目录最多返回的文件数，防止误选超大目录把前端卡死
const MAX_LIST_FILES: usize = 5000;

/// 目录下的一个待导入文件
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportFileEntry {
    /// 文件显示名
    pub name: String,
    /// 绝对路径
    pub path: String,
}

/// 探测到的已安装 SSH 客户端（及其会话存储位置）
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedClient {
    /// 稳定标识：xshell / mobaxterm / electerm / termius / finalshell
    pub id: String,
    /// 显示名
    pub name: String,
    /// 存储形态：folder（会话目录） / file（单个配置文件） / db（数据库）
    pub kind: String,
    /// 会话存储绝对路径
    pub path: String,
    /// 提示信息（如密码是否可迁移）
    pub note: String,
}

// ===== 原语 1：选择文件 / 目录 =====

/// 弹出系统「打开文件」对话框，返回选定绝对路径；取消返回 None。
/// filters 为扩展名列表（不带点），空则不过滤。
#[command]
pub fn pick_import_file(title: String, filters: Option<Vec<String>>) -> Option<String> {
    #[cfg(desktop)]
    {
        let mut dlg = rfd::FileDialog::new().set_title(&title);
        if let Some(exts) = filters {
            let exts: Vec<String> = exts
                .into_iter()
                .map(|e| e.trim_start_matches('.').to_ascii_lowercase())
                .filter(|e| !e.is_empty())
                .collect();
            if !exts.is_empty() {
                let refs: Vec<&str> = exts.iter().map(|s| s.as_str()).collect();
                dlg = dlg.add_filter("会话文件", &refs);
            }
        }
        // 不追加 "所有文件" 过滤器：rfd 会把 "*" 当作扩展名解析，
        // macOS 上会导致全部文件被置灰、无法选中。不调用 add_filter 即为"不过滤"。
        dlg.pick_file().map(|p| p.to_string_lossy().to_string())
    }
    #[cfg(mobile)]
    {
        let _ = (title, filters);
        None
    }
}

/// 弹出系统「选择文件夹」对话框，返回选定绝对路径；取消返回 None
#[command]
pub fn pick_import_folder(title: String) -> Option<String> {
    #[cfg(desktop)]
    {
        rfd::FileDialog::new()
            .set_title(&title)
            .pick_folder()
            .map(|p| p.to_string_lossy().to_string())
    }
    #[cfg(mobile)]
    {
        let _ = title;
        None
    }
}

// ===== 原语 2：读取本地文本 =====

/// 读取本地文本文件（webview 无权读取任意路径，必须经由此命令）。
/// 自动处理 BOM 与 Xshell 常见的 UTF-16LE 编码。
#[command]
pub fn read_local_text_file(path: String) -> AppResult<String> {
    let p = PathBuf::from(&path);
    if !p.is_file() {
        return Err(AppError::Validation(format!("文件不存在或不是文件：{path}")));
    }
    let meta = std::fs::metadata(&p)
        .map_err(|e| AppError::Validation(format!("无法读取文件信息：{e}")))?;
    if meta.len() as usize > MAX_IMPORT_BYTES {
        return Err(AppError::Validation(
            "文件过大（上限 8MB），请选择正确的会话文件".into(),
        ));
    }
    let bytes = std::fs::read(&p).map_err(|e| AppError::Validation(format!("读取文件失败：{e}")))?;
    decode_text(bytes).map_err(|e| AppError::Validation(format!("解析文件编码失败：{e}")))
}

// ===== 原语 3：列目录 + 探测已安装客户端 =====

/// 列出目录下（含子目录）匹配扩展名的文件。
/// extensions 为空表示不限扩展名；结果按名称排序，最多 MAX_LIST_FILES 条。
#[command]
pub fn list_import_files(
    dir: String,
    extensions: Option<Vec<String>>,
) -> AppResult<Vec<ImportFileEntry>> {
    let root = PathBuf::from(&dir);
    if !root.is_dir() {
        return Err(AppError::Validation(format!("目录不存在：{dir}")));
    }
    let exts: Vec<String> = extensions
        .unwrap_or_default()
        .into_iter()
        .map(|e| e.trim_start_matches('.').to_ascii_lowercase())
        .collect();

    let mut out = Vec::new();
    collect_files(&root, &exts, &mut out);
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// 探测本机已安装的 SSH 客户端及其会话存储位置（只返回确实存在的）
#[command]
pub fn scan_installed_clients() -> Vec<DetectedClient> {
    let mut out: Vec<DetectedClient> = Vec::new();
    let home = home_dir().unwrap_or_default();

    // Xshell（仅 Windows）：Documents\NetSarang*\<版本>\Xshell\Sessions
    #[cfg(target_os = "windows")]
    if let Some(p) = find_xshell_sessions_dir(&home) {
        out.push(DetectedClient {
            id: "xshell".into(),
            name: "Xshell".into(),
            kind: "folder".into(),
            path: p,
            note: "导入连接信息；Xshell 密码为密文，导入后需手动补全".into(),
        });
    }

    // MobaXterm（仅 Windows 有官方版）：%APPDATA%\MobaXterm\MobaXterm.ini
    #[cfg(target_os = "windows")]
    if let Some(p) = find_mobaxterm_ini(&home, &app_data_dir().unwrap_or_default()) {
        out.push(DetectedClient {
            id: "mobaxterm".into(),
            name: "MobaXterm".into(),
            kind: "file".into(),
            path: p,
            note: "从 MobaXterm.ini 的书签导入；口令为密文，需手动补全".into(),
        });
    }

    // Electerm：~/.electerm/data（旧版 electerm.json，新版 SQLite）
    if let Some(p) = first_existing_dir(&[
        home.join(".electerm").join("data"),
        home.join(".electerm"),
        app_data_dir().unwrap_or_default().join("electerm"),
    ]) {
        let is_sqlite = std::fs::read_dir(&p)
            .map(|rd| {
                rd.flatten()
                    .any(|e| e.path().extension().map(|x| x == "db").unwrap_or(false))
            })
            .unwrap_or(false);
        out.push(DetectedClient {
            id: "electerm".into(),
            name: "Electerm".into(),
            kind: if is_sqlite { "db".into() } else { "folder".into() },
            note: if is_sqlite {
                "新版为 SQLite 存储，无法直接读取；请在 Electerm 内导出 JSON 后用「从文件导入」".into()
            } else {
                "从 electerm.json 导入；设过主密码的口令无法迁移".into()
            },
            path: p,
        });
    }

    // Termius：数据在 Electron 的 IndexedDB(LevelDB) 中且为加密存储 —— 能探测到，
    // 但无法像文本类客户端那样直接解析，故标记为 db 并在 UI 里明确告知用户。
    if let Some(p) = find_termius_data_dir(&home) {
        out.push(DetectedClient {
            id: "termius".into(),
            name: "Termius".into(),
            kind: "db".into(),
            note: "本地为加密的 IndexedDB 存储，无法直接读取；如需迁移请先导出为 JSON 再用「从文件导入」".into(),
            path: p,
        });
    }

    // FinalShell：Windows 版把会话数据放在「安装目录\conn」下（安装盘可自定义，
    // 常见于 D:\app\finalshell），所以优先从注册表卸载项拿安装目录，再退回常见/用户目录。
    if let Some(p) = find_finalshell_conn(&home) {
        out.push(DetectedClient {
            id: "finalshell".into(),
            name: "FinalShell".into(),
            kind: "folder".into(),
            note: "从 conn 目录导入；口令为密文，需手动补全".into(),
            path: p,
        });
    }

    out
}

// ===== 内部辅助 =====

fn home_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE").ok().map(PathBuf::from)
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
}

/// 定位 FinalShell 的会话目录（优先 `conn` 子目录，其次安装目录本身）。
/// Windows 版会话数据在「安装目录\conn」：安装盘可自定义，故先查注册表卸载项；
/// 查不到再退回各盘常见安装位置与用户目录。macOS/Linux 走 ~/.finalshell 一类路径。
fn find_finalshell_conn(home: &Path) -> Option<String> {
    let mut cands: Vec<PathBuf> = Vec::new();

    #[cfg(target_os = "windows")]
    {
        for root in [
            r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
            r"HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
            r"HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        ] {
            if let Some(dir) = finalshell_install_dir_from_reg(root) {
                cands.push(dir.join("conn"));
                cands.push(dir);
            }
        }
        // 常见安装位置兜底（覆盖 C/D 等盘）
        for drive in b'C'..=b'Z' {
            let root = PathBuf::from(format!("{}:\\", drive as char));
            if !root.exists() {
                continue;
            }
            cands.push(root.join("Program Files").join("FinalShell").join("conn"));
            cands.push(root.join("Program Files (x86)").join("FinalShell").join("conn"));
            cands.push(root.join("FinalShell").join("conn"));
        }
    }

    cands.push(home.join(".finalshell").join("conn"));
    cands.push(app_data_dir().unwrap_or_default().join("finalshell").join("conn"));
    #[cfg(target_os = "windows")]
    cands.push(local_app_data_dir().unwrap_or_default().join("finalshell").join("conn"));
    cands.push(home.join(".finalshell"));
    cands.push(app_data_dir().unwrap_or_default().join("finalshell"));

    if let Some(p) = first_existing_dir(&cands) {
        return Some(p);
    }

    // 兜底：绿色版 / 自定义安装目录（如 D:\software\finalshell）既不在注册表也不在常见位置，
    // 因此浅层扫描各盘二级目录，找「名字含 finalshell 且带 conn 子目录」的目录。
    #[cfg(target_os = "windows")]
    if let Some(p) = find_finalshell_by_scan() {
        return Some(p.to_string_lossy().to_string());
    }
    None
}

/// 浅层扫描所有盘符（根目录 + 一层子目录）定位 FinalShell 数据目录。
/// 深度 2 足以覆盖 `D:\software\finalshell` 这类布局，同时用预算上限避免误扫超大目录卡死。
#[cfg(target_os = "windows")]
fn find_finalshell_by_scan() -> Option<PathBuf> {
    const SKIP: &[&str] = &[
        "windows",
        "$recycle.bin",
        "system volume information",
        "perflogs",
        "recovery",
        "msocache",
        "node_modules",
        ".git",
        "$windows.~bt",
        "$windows.~ws",
        "$sysreset",
    ];
    // 目录访问预算：正常磁盘远用不到，极端情况直接放弃（用户仍可用「从文件夹导入」）
    let mut budget: usize = 30_000;

    for drive in b'C'..=b'Z' {
        let root = PathBuf::from(format!("{}:\\", drive as char));
        let entries = match std::fs::read_dir(&root) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for e1 in entries.flatten() {
            if budget == 0 {
                return None;
            }
            let p1 = e1.path();
            if !p1.is_dir() {
                continue;
            }
            let n1 = e1.file_name().to_string_lossy().to_lowercase();
            if SKIP.contains(&n1.as_str()) {
                continue;
            }
            if n1.contains("finalshell") {
                if let Some(d) = finalshell_data_dir(&p1) {
                    return Some(d);
                }
            }
            // 再下探一层
            if let Ok(entries2) = std::fs::read_dir(&p1) {
                for e2 in entries2.flatten() {
                    if budget == 0 {
                        return None;
                    }
                    budget -= 1;
                    let p2 = e2.path();
                    if !p2.is_dir() {
                        continue;
                    }
                    let n2 = e2.file_name().to_string_lossy().to_lowercase();
                    if n2.contains("finalshell") {
                        if let Some(d) = finalshell_data_dir(&p2) {
                            return Some(d);
                        }
                    }
                }
            }
        }
    }
    None
}

/// FinalShell 数据目录：优先 `conn` 子目录，否则目录本身若含 *_connect_config.json 也可用
#[cfg(target_os = "windows")]
fn finalshell_data_dir(dir: &Path) -> Option<PathBuf> {
    let conn = dir.join("conn");
    if conn.is_dir() {
        return Some(conn);
    }
    let has_cfg = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten().any(|e| {
                e.file_name()
                    .to_string_lossy()
                    .to_lowercase()
                    .ends_with("connect_config.json")
            })
        })
        .unwrap_or(false);
    if has_cfg {
        return Some(dir.to_path_buf());
    }
    None
}

/// 从某个卸载注册表根键里找 DisplayName 含 "finalshell" 的项，返回其安装目录
#[cfg(target_os = "windows")]
fn finalshell_install_dir_from_reg(root: &str) -> Option<PathBuf> {
    let dump = reg_query_recursive(root)?;
    parse_uninstall_entries(&dump)
}

/// `reg query <key> /s`（隐藏控制台窗口，避免 GUI 进程闪黑框）
#[cfg(target_os = "windows")]
fn reg_query_recursive(key: &str) -> Option<String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("reg")
        .args(["query", key, "/s"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).to_string())
}

/// 解析卸载项 dump，取第一个 DisplayName 含 finalshell 的安装目录
#[cfg(target_os = "windows")]
fn parse_uninstall_entries(text: &str) -> Option<PathBuf> {
    let mut display = String::new();
    let mut loc = String::new();
    let mut uninst = String::new();
    let mut icon = String::new();
    let mut matched = false;

    for line in text.lines() {
        let trimmed = line.trim_start();
        // 子键路径行：结算上一个子键
        if trimmed.starts_with("HKEY") {
            if matched {
                if let Some(d) = resolve_uninstall_dir(&loc, &uninst, &icon) {
                    return Some(d);
                }
            }
            matched = false;
            display.clear();
            loc.clear();
            uninst.clear();
            icon.clear();
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("displayname") {
            display = reg_value_tail(trimmed);
            matched = display.to_ascii_lowercase().contains("finalshell");
        } else if lower.starts_with("installlocation") {
            loc = reg_value_tail(trimmed);
        } else if lower.starts_with("uninstallstring") {
            uninst = reg_value_tail(trimmed);
        } else if lower.starts_with("displayicon") {
            icon = reg_value_tail(trimmed);
        }
    }
    if matched {
        return resolve_uninstall_dir(&loc, &uninst, &icon);
    }
    None
}

/// 取 REG_SZ / REG_EXPAND_SZ 后面的值部分
#[cfg(target_os = "windows")]
fn reg_value_tail(line: &str) -> String {
    let idx = line
        .find("REG_EXPAND_SZ")
        .or_else(|| line.find("REG_SZ"));
    match idx {
        Some(i) => {
            let after = &line[i..];
            match after.find(|c: char| c.is_whitespace()) {
                Some(sp) => after[sp..].trim().to_string(),
                None => String::new(),
            }
        }
        None => String::new(),
    }
}

/// 依次尝试 InstallLocation → UninstallString 上一级 → DisplayIcon 上一级
#[cfg(target_os = "windows")]
fn resolve_uninstall_dir(loc: &str, uninst: &str, icon: &str) -> Option<PathBuf> {
    let clean = |s: &str| s.trim().trim_matches('"').trim().to_string();

    let l = clean(loc);
    if !l.is_empty() {
        return Some(PathBuf::from(l));
    }
    for raw in [uninst, icon] {
        let s = clean(raw);
        if s.is_empty() {
            continue;
        }
        // DisplayIcon 可能是 "D:\app\finalshell\fs.exe,0"，去掉 ,0
        let s = s.split(',').next().unwrap_or("").trim();
        if let Some(p) = PathBuf::from(s).parent() {
            if !p.as_os_str().is_empty() {
                return Some(p.to_path_buf());
            }
        }
    }
    None
}

/// Windows 本地应用数据目录（%LOCALAPPDATA%，FinalShell 等客户端用）
#[cfg(target_os = "windows")]
fn local_app_data_dir() -> Option<PathBuf> {
    std::env::var("LOCALAPPDATA").ok().map(PathBuf::from)
}

/// 应用数据目录：Windows=%APPDATA%，macOS=~/Library/Application Support，Linux=$XDG_CONFIG_HOME 或 ~/.config
fn app_data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA").ok().map(PathBuf::from)
    }
    #[cfg(target_os = "macos")]
    {
        home_dir().map(|h| h.join("Library").join("Application Support"))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::env::var("XDG_CONFIG_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| home_dir().map(|h| h.join(".config")))
    }
}

/// 从候选里取第一个存在的目录
fn first_existing_dir(cands: &[PathBuf]) -> Option<String> {
    cands
        .iter()
        .find(|p| !p.as_os_str().is_empty() && p.is_dir())
        .map(|p| p.to_string_lossy().to_string())
}

/// 从候选里取第一个存在的文件
#[allow(dead_code)] // 目前仅 Windows 的 MobaXterm 探测用到，其它平台编译需豁免
fn first_existing_file(cands: &[PathBuf]) -> Option<String> {
    cands
        .iter()
        .find(|p| !p.as_os_str().is_empty() && p.is_file())
        .map(|p| p.to_string_lossy().to_string())
}

/// 定位 Termius 数据目录（Electron 应用数据根，必须带 databases / IndexedDB 子目录才算命中）
fn find_termius_data_dir(home: &Path) -> Option<String> {
    let mut cands: Vec<PathBuf> = Vec::new();
    if let Some(app) = app_data_dir() {
        cands.push(app.join("Termius"));
    }
    cands.push(home.join("Library").join("Application Support").join("Termius"));
    cands.push(home.join(".config").join("Termius"));
    cands.push(home.join(".local").join("share").join("Termius"));
    cands
        .into_iter()
        .find(|d| d.is_dir() && (d.join("databases").is_dir() || d.join("IndexedDB").is_dir()))
        .map(|d| d.to_string_lossy().to_string())
}

/// Xshell 会话目录：Documents\NetSarang*\<版本>\Xshell\Sessions
/// 版本目录（6/7/8…）不固定，故用"遍历 NetSarang* 往下找两层"的方式定位。
#[cfg(target_os = "windows")]
fn find_xshell_sessions_dir(home: &Path) -> Option<String> {
    let docs = home.join("Documents");
    if !docs.is_dir() {
        return None;
    }
    for e1 in std::fs::read_dir(&docs).ok()?.flatten() {
        let p1 = e1.path();
        if !p1.is_dir() {
            continue;
        }
        let nm = p1.file_name()?.to_string_lossy().to_lowercase();
        if !nm.starts_with("netsarang") {
            continue;
        }
        // 形态 A：NetSarang\Xshell\Sessions
        let direct = p1.join("Xshell").join("Sessions");
        if direct.is_dir() {
            return Some(direct.to_string_lossy().to_string());
        }
        // 形态 B：NetSarang Computer\<版本>\Xshell\Sessions
        for e2 in std::fs::read_dir(&p1).ok()?.flatten() {
            let p2 = e2.path();
            if !p2.is_dir() {
                continue;
            }
            let s = p2.join("Xshell").join("Sessions");
            if s.is_dir() {
                return Some(s.to_string_lossy().to_string());
            }
        }
    }
    None
}

/// MobaXterm.ini：便携版放在安装目录，常规版在 %APPDATA%\MobaXterm
#[cfg(target_os = "windows")]
fn find_mobaxterm_ini(home: &Path, appdata: &Path) -> Option<String> {
    first_existing_file(&[
        appdata.join("MobaXterm").join("MobaXterm.ini"),
        home.join("MobaXterm.ini"),
        home.join("Documents").join("MobaXterm").join("MobaXterm.ini"),
    ])
}

/// 递归收集文件（exts 为空表示不限扩展名）
fn collect_files(dir: &Path, exts: &[String], out: &mut Vec<ImportFileEntry>) {
    if out.len() >= MAX_LIST_FILES {
        return;
    }
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return,
    };
    for entry in rd.flatten() {
        if out.len() >= MAX_LIST_FILES {
            return;
        }
        let p = entry.path();
        if p.is_dir() {
            collect_files(&p, exts, out);
            continue;
        }
        if !exts.is_empty() {
            let matched = p
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| exts.contains(&e.to_ascii_lowercase()))
                .unwrap_or(false);
            if !matched {
                continue;
            }
        }
        out.push(ImportFileEntry {
            name: p
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default(),
            path: p.to_string_lossy().to_string(),
        });
    }
}

/// 文本解码：优先按 BOM 判断，其次按 UTF-8，最后回退到 UTF-16LE（Xshell .xsh 常见且常无 BOM）
fn decode_text(bytes: Vec<u8>) -> Result<String, String> {
    // UTF-16LE BOM
    if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
        return Ok(decode_utf16(&bytes[2..], true));
    }
    // UTF-16BE BOM
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        return Ok(decode_utf16(&bytes[2..], false));
    }
    // UTF-8 BOM
    if bytes.len() >= 3 && bytes[0] == 0xEF && bytes[1] == 0xBB && bytes[2] == 0xBF {
        return Ok(String::from_utf8_lossy(&bytes[3..]).to_string());
    }
    // 无 BOM：先试严格 UTF-8
    if let Ok(s) = String::from_utf8(bytes.clone()) {
        return Ok(s);
    }
    // 回退 UTF-16LE（.xsh 常见：偶数长度 + 大量 0x00）
    if bytes.len() >= 2 && bytes.len() % 2 == 0 {
        let zeros = bytes.iter().filter(|b| **b == 0).count();
        if zeros * 2 > bytes.len() / 3 {
            return Ok(decode_utf16(&bytes, true));
        }
    }
    // 最后宽松解析，保证不会因为个别乱码字节导致整个导入失败
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

fn decode_utf16(bytes: &[u8], little: bool) -> String {
    let mut u16s: Vec<u16> = Vec::with_capacity(bytes.len() / 2 + 1);
    let mut i = 0;
    while i + 1 < bytes.len() {
        let v = if little {
            u16::from_le_bytes([bytes[i], bytes[i + 1]])
        } else {
            u16::from_be_bytes([bytes[i], bytes[i + 1]])
        };
        u16s.push(v);
        i += 2;
    }
    String::from_utf16_lossy(&u16s)
}
