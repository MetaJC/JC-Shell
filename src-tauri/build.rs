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

fn main() {
    // 编译期常量 OAUTH_DEFAULT_SERVER_BASE_URL 来自 option_env!("OPENJC_SERVER_BASE_URL")。
    // 必须显式声明该环境变量为构建依赖，否则改 .env 后 cargo 不会重编 common crate，
    // 导致常量停留在上一次（可能为空）的缓存值，运行时报 "server_base_url 为空"。
    println!("cargo:rerun-if-env-changed=OPENJC_SERVER_BASE_URL");
    // 图标是编译期由 tauri::generate_context! 嵌入的，但 cargo / tauri-build 默认都不跟踪图标文件，
    // 导致改了 icons/ 后 dev 二进制仍内嵌旧图标（例如旧的“白底”图标）。
    // 这里显式声明依赖，任何图标变化都会触发重编译，保证 Dock / 任务栏图标与 icons/ 一致。
    println!("cargo:rerun-if-changed=icons/icon.icns");
    println!("cargo:rerun-if-changed=icons/icon.ico");
    println!("cargo:rerun-if-changed=icons/icon.png");
    println!("cargo:rerun-if-changed=icons/32x32.png");
    println!("cargo:rerun-if-changed=icons/128x128.png");
    println!("cargo:rerun-if-changed=icons/128x128@2x.png");
    println!("cargo:rerun-if-changed=icons/taskbar.ico");
    tauri_build::build()
}
