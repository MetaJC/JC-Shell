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

//! 应用运行时语言（启动从系统 locale 初始化，set_app_locale 命令运行时更新）
//! 作者：JC

use std::sync::{Mutex, OnceLock};

use tauri::{AppHandle, Emitter, Runtime};
use tauri_plugin_os::locale;

use crate::menu;

/// 全局当前 locale（BCP-47，例如 "zh-CN" / "en"）
static APP_LOCALE: OnceLock<Mutex<String>> = OnceLock::new();

/// 启动时从系统 locale 初始化全局语言；只首次生效，后续 set_app_locale 覆盖
pub fn init_from_system() {
    let detected = locale().unwrap_or_else(|| "en".to_string());
    tracing::info!(locale = %detected, "检测到系统语言");
    let _ = APP_LOCALE.set(Mutex::new(detected));
}

fn get_locale() -> String {
    APP_LOCALE
        .get()
        .and_then(|m| m.lock().ok().map(|s| s.clone()))
        .unwrap_or_else(|| "en".to_string())
}

/// 供 setup 阶段读当前 locale（tray 菜单等场景）
pub fn current_locale() -> String {
    get_locale()
}

fn set_locale_inner(new_locale: &str) {
    if let Some(m) = APP_LOCALE.get() {
        if let Ok(mut s) = m.lock() {
            *s = new_locale.to_string();
        }
    }
}

/// Tauri command：返回当前应用语言（前端启动时用于同步 i18n store）
#[tauri::command]
pub fn get_app_locale() -> String {
    get_locale()
}

/// 切换应用语言（重建 macOS 菜单 + 通知前端）
/// `locale` 形如 "zh-CN" / "en"（BCP-47）；传 "default" 重新从系统读取
#[tauri::command]
pub fn set_app_locale<R: Runtime>(app: AppHandle<R>, locale: String) -> Result<(), String> {
    let resolved = if locale == "default" {
        tauri_plugin_os::locale().unwrap_or_else(|| "en".to_string())
    } else {
        locale
    };
    tracing::info!(locale = %resolved, "切换应用语言");

    set_locale_inner(&resolved);

    // 重建 macOS 系统菜单栏
    menu::build_app_menu(&app, &resolved).map_err(|e| {
        tracing::warn!(error = %e, "重建 macOS 菜单失败");
        e.to_string()
    })?;

    // 通知前端（主窗口 + settings 窗口）i18n store 立即同步
    let _ = app.emit("app:locale-changed", &resolved);
    Ok(())
}

/// 重新构建 macOS 菜单（启动 setup 阶段调用）
pub fn apply_initial_menu<R: Runtime>(app: &AppHandle<R>) {
    if let Err(e) = menu::build_app_menu(app, &get_locale()) {
        tracing::warn!(error = %e, "初始化 macOS 菜单失败");
    }
}
