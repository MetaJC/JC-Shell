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

// tauri 命令包装器（仅修复 iOS 构建链路的环境变量丢失）
// ------------------------------------------------------------------
// 背景:
//   Xcode 生成的 iOS 工程里有一个 shell 构建阶段:
//     npm run -- tauri ios xcode-script -v --platform ... --configuration ${CONFIGURATION:?} ...
//   iOS 的 Rust 就是在这个阶段里编译的，而 Xcode 会净化脚本阶段的环境变量，
//   导致 with-env.mjs 注入的 OPENJC_SERVER_BASE_URL 传不进 cargo，
//   编译期常量 OAUTH_DEFAULT_SERVER_BASE_URL 为空（打出的包里没有任何服务地址）。
//
// 作用范围（刻意做到最小）:
//   仅当参数里出现 `xcode-script`（即 Xcode 内部编译 Rust 那一次调用）时，
//   按 --configuration 选择 env 文件并注入：
//     Release -> .env.production     Debug -> .env.development
//   其余所有调用（npm run tauri dev/build/icon ...）原样透传给 tauri 二进制，
//   行为与改动前完全一致，不做任何环境变量注入。
// ------------------------------------------------------------------
import { spawn } from 'node:child_process'
import { existsSync } from 'node:fs'
import { join } from 'node:path'

const args = process.argv.slice(2)

/** 是否为 Xcode 内部编译 Rust 的 xcode-script 调用 */
function isXcodeScript() {
  return args.includes('xcode-script')
}

/** 取 --configuration 的值（Xcode 传入 Debug / Release） */
function configurationOf() {
  const i = args.indexOf('--configuration')
  if (i !== -1 && args[i + 1] && !args[i + 1].startsWith('-')) {
    return args[i + 1]
  }
  return ''
}

/** 按构建配置选 env 文件；缺省/未知回退到开发环境（最安全） */
function envFileFor(config) {
  return config === 'Release' ? '.env.production' : '.env.development'
}

function run(cmd, cmdArgs, options = {}) {
  const child = spawn(cmd, cmdArgs, { stdio: 'inherit', env: process.env, ...options })
  child.on('exit', code => process.exit(code ?? 0))
  child.on('error', err => {
    console.error(`[tauri-xcode-env] failed to spawn \`${cmd}\`: ${err.message}`)
    process.exit(1)
  })
}

if (isXcodeScript()) {
  const config = configurationOf()
  const envFile = envFileFor(config)
  console.log(
    `[tauri-xcode-env] xcode-script 调用 (configuration=${config || 'unknown'}) -> 注入 ${envFile}`
  )
  // 复用 with-env.mjs：它负责加载 env 文件 + 把 tools/bin(xcodebuild wrapper) 放进 PATH
  const withEnv = new URL('./with-env.mjs', import.meta.url).pathname
  run(process.execPath, [withEnv, envFile, '--', 'tauri', ...args])
} else {
  // 非 Xcode 构建阶段：与改动前完全一致，直接调 tauri 二进制，不注入环境变量。
  // 显式解析 node_modules/.bin/tauri，不依赖调用方是否把 .bin 放进 PATH。
  // Windows 上 .bin/tauri 是 shell 脚本（CreateProcess 无法直接执行），须用 tauri.cmd。
  const localTauri = join(process.cwd(), 'node_modules', '.bin', 'tauri')
  const localTauriBin = process.platform === 'win32' ? localTauri + '.cmd' : localTauri
  const localTauriOpts = process.platform === 'win32' ? { shell: true } : undefined
  run(existsSync(localTauriBin) ? localTauriBin : 'tauri', args, localTauriOpts)
}
