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

//! 把阻塞型工作挪出 UI 主线程
//! 作者：JC
//!
//! 背景（务必阅读，新增 command 时容易踩）：
//! Tauri 的**同步** `#[tauri::command] pub fn` 是在主线程（UI 线程）上执行的，
//! 而同一个进程内 main / term-* / settings / tab-ghost 所有窗口的消息泵、
//! WebView2 事件分发与渲染触发都共用这一条线程。
//! 因此同步命令里任何耗时操作（读盘、列目录、枚举磁盘、spawn 进程、sleep、
//! 递归删除、等锁）都会让**所有窗口一起卡住**，且界面完全无响应。
//!
//! 规则：凡是有 IO / 进程 / 睡眠 / 大计算的 command，一律写成
//! `async fn` + 本模块的 `run(...)`（底层是 tokio 的阻塞线程池），
//! 或者干脆把结果放进后台常驻线程的快照里只做内存读取。

use crate::common::{AppError, AppResult};

/// 在阻塞线程池执行闭包，避免占用 Tauri 的 UI 主线程。
///
/// 用法：
/// ```ignore
/// #[tauri::command]
/// pub async fn list_local_dir(path: String) -> AppResult<Vec<LocalFileEntry>> {
///     crate::common::blocking::run(move || list_local_dir_inner(path)).await
/// }
/// ```
pub async fn run<F, T>(f: F) -> AppResult<T>
where
    F: FnOnce() -> AppResult<T> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| AppError::Internal(format!("后台任务执行失败: {e}")))?
}
