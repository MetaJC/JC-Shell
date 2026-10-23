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

//! JC Shell 可执行文件入口
//! 作者：JC

// Windows 子系统设置：
// - debug 模式（tauri dev）：保留 console，方便在终端看 Rust 启动日志
// - release 模式（tauri build）：切到 windows（GUI）子系统，**不弹黑窗口**
//   关键：不开 console → 不创建默认 console 窗口 → 用户启动 JC Shell 时直接看到主窗口
//   排查 release 模式问题时，临时注释掉 cfg_attr 即可重新打开 console 看日志
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    jc_shell_lib::run();
}
