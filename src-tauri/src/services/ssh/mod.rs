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

//! SSH Service —— russh 实现
//! 作者：JC

pub mod russh_impl;
pub mod transfer;

pub use russh_impl::RusshSessionHandle;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use async_trait::async_trait;
use tauri::AppHandle;

use crate::common::AppResult;

/// SSH 会话事件（通过 Tauri Event 推给前端）
#[derive(Debug, Clone)]
pub enum SshSessionEvent {
    Connected,
    Disconnected,
    Output(Vec<u8>),
    Error(String),
}

/// SSH 会话句柄（本地终端会话也实现此 trait，文件系统操作按 is_local 分流）
#[async_trait]
pub trait SshSessionHandle: Send + Sync {
    async fn write(&self, data: &[u8]) -> AppResult<()>;
    async fn resize(&self, cols: u16, rows: u16) -> AppResult<()>;
    async fn close(&self) -> AppResult<()>;
    /// 执行远程命令并返回输出（不检查退出码）
    async fn exec_command(&self, command: &str) -> AppResult<String>;
    /// 获取交互式 Shell 的当前工作目录（通过 /proc/<pid>/cwd）
    async fn shell_cwd(&self) -> AppResult<String>;
    /// 上传本地文件到远程路径（remote_path 为完整文件路径）
    /// transfer_id 用于进度事件（transfer-progress）关联；cancel 置位后尽快中止并清理残留
    async fn upload_file(&self, app: &AppHandle, transfer_id: &str, local_path: &str, remote_path: &str, cancel: Arc<AtomicBool>) -> AppResult<()>;
    /// 下载远程文件到本地路径（可取消）
    async fn download_file(&self, app: &AppHandle, transfer_id: &str, remote_path: &str, local_path: &str, cancel: Arc<AtomicBool>) -> AppResult<()>;
    /// 上传本地路径（文件或目录；目录递归上传，进度按总字节聚合）
    async fn upload_path(&self, app: &AppHandle, transfer_id: &str, local_path: &str, remote_path: &str, cancel: Arc<AtomicBool>) -> AppResult<()>;
    /// 下载远程路径（文件或目录；目录递归下载）
    async fn download_path(&self, app: &AppHandle, transfer_id: &str, remote_path: &str, local_path: &str, cancel: Arc<AtomicBool>) -> AppResult<()>;
    /// 读取文件原始字节（文本编辑器用；超过 max_bytes 报错）
    async fn read_file_bytes(&self, path: &str, max_bytes: u64) -> AppResult<Vec<u8>>;
    /// 以指定编码（WHATWG 标签，如 utf-8/gbk/big5）把文本写回文件；空串 = 保持原编码语义由调用方决定，这里空串按 utf-8
    async fn write_file_text(&self, path: &str, content: &str, encoding: &str) -> AppResult<()>;
    /// 在目标机器执行 shell 命令并检查退出码（非 0 返回输出作为错误；压缩/解压等用）
    async fn run_shell_checked(&self, command: &str) -> AppResult<String>;
    /// 列出单个远程目录（文件浏览器用）。
    /// 走 SFTP read_dir，**不经过 shell**，因此不与终端 exec 通道争用：
    /// 跑部署脚本 / 大文件上传时也不会周期性失败。SFTP 不可用时返回 Err，由调用方回退 exec 的 `ls -la`。
    async fn list_dir(&self, path: &str) -> AppResult<Vec<crate::services::ssh::transfer::RemoteDirEntry>>;
    /// 是否本地会话（本地终端：文件操作直接走本机文件系统）
    fn is_local(&self) -> bool { false }
}
