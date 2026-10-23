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

// 生成「App Store 专用（白底不透明）」图标，完全隔离，不影响 PC 与移动端包。
//
// 为什么需要它：App Store Connect 要求提交一张不透明图标，与包内（透明）图标冲突。
// 做法：用同一张透明母图 app-icon.png，仅把 iOS 背景填成白色，且产物只导出到隔离目录，
//       绝不写回 PC 桌面图标（icons/ 根目录）或真实移动端包（gen/apple、icons/android）。
//
// 产物：src-tauri/icons/store/app-store-icon-1024.png（直接上传 App Store Connect 用）
// 用法:
//   npm run icon:store
//   JC_STORE_COLOR="#ffffff" npm run icon:store   # 自定义 App Store 底色（默认白）
import { spawnSync } from 'node:child_process'
import { copyFileSync, existsSync, mkdirSync, rmSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { tmpdir } from 'node:os'

const __dirname = dirname(fileURLToPath(import.meta.url))
const frontendDir = resolve(__dirname, '..')
const tauriDir = join(frontendDir, 'src-tauri')
const iconsDir = join(tauriDir, 'icons')
const sourceIcon = join(tauriDir, 'app-icon.png')
const storeColor = process.env.JC_STORE_COLOR || '#ffffff'
const storeDir = join(iconsDir, 'store')
const out1024 = join(storeDir, 'app-store-icon-1024.png')

function fail(msg) {
  console.error(`[icon-store] ${msg}`)
  process.exit(1)
}
if (!existsSync(sourceIcon)) fail(`找不到母图: ${sourceIcon}`)

const tmp = join(tmpdir(), `jc-icons-store-${Date.now()}`)
mkdirSync(tmp, { recursive: true })

const tauriCli = join(frontendDir, 'node_modules', '@tauri-apps', 'cli', 'tauri.js')
if (!existsSync(tauriCli)) fail(`找不到 Tauri CLI: ${tauriCli}`)

const res = spawnSync(
  process.execPath,
  [tauriCli, 'icon', sourceIcon, '--ios-color', storeColor, '--output', tmp],
  { cwd: frontendDir, stdio: 'inherit', env: process.env },
)
if (res.status !== 0) {
  rmSync(tmp, { recursive: true, force: true })
  fail(`tauri icon 执行失败（exit=${res.status}）`)
}

// 只取 iOS 1024（= AppIcon-512@2x.png）作为 App Store 提交图，其余全部丢弃，确保不影响工程其它图标
const src1024 = join(tmp, 'ios', 'AppIcon-512@2x.png')
if (!existsSync(src1024)) {
  rmSync(tmp, { recursive: true, force: true })
  fail(`未找到生成的 1024 图标: ${src1024}`)
}
mkdirSync(storeDir, { recursive: true })
copyFileSync(src1024, out1024)
rmSync(tmp, { recursive: true, force: true })

console.log(`[icon-store] 完成：App Store 白底图标已生成 -> ${out1024}`)
console.log('[icon-store] 该文件独立于 PC / 移动端包，上传 App Store Connect 时单独使用即可。')
