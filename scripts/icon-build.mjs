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

// 生成「透明背景」的应用图标：PC 桌面 + 移动端包全部透明，且 PC 桌面图标绝不被改动。
//
// 设计目标（与 App Store 白底图标彻底隔离）：
//   - 透明母图 src-tauri/app-icon.png（PC 与移动端共用，背景透明）
//   - tauri icon 会一次性产出「桌面 + iOS + Android」三套，且默认 iOS 是白底；
//     所以用 --ios-color rgba(0,0,0,0) 让 iOS 也透明，并把产物先写到临时目录；
//   - 只把「移动端」部分（ios / android）拷回真实工程，PC 桌面图标（icons/ 根目录）原样不动。
//
// 用法:
//   npm run icon:build
//   JC_IOS_COLOR="rgba(0,0,0,0)" npm run icon:build   # 自定义 iOS 背景（默认透明）
import { spawnSync } from 'node:child_process'
import { copyFileSync, existsSync, mkdirSync, readdirSync, rmSync, statSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { tmpdir } from 'node:os'

const __dirname = dirname(fileURLToPath(import.meta.url))
const frontendDir = resolve(__dirname, '..')
const tauriDir = join(frontendDir, 'src-tauri')
const iconsDir = join(tauriDir, 'icons')
const sourceIcon = join(tauriDir, 'app-icon.png') // 透明母图：PC + 移动端共用
const iosColor = process.env.JC_IOS_COLOR || 'rgba(0,0,0,0)'

function fail(msg) {
  console.error(`[icon-build] ${msg}`)
  process.exit(1)
}
if (!existsSync(sourceIcon)) fail(`找不到透明母图: ${sourceIcon}`)

const tmp = join(tmpdir(), `jc-icons-build-${Date.now()}`)
mkdirSync(tmp, { recursive: true })

const tauriCli = join(frontendDir, 'node_modules', '@tauri-apps', 'cli', 'tauri.js')
if (!existsSync(tauriCli)) fail(`找不到 Tauri CLI: ${tauriCli}`)

const res = spawnSync(
  process.execPath,
  [tauriCli, 'icon', sourceIcon, '--ios-color', iosColor, '--output', tmp],
  { cwd: frontendDir, stdio: 'inherit', env: process.env },
)
if (res.status !== 0) {
  rmSync(tmp, { recursive: true, force: true })
  fail(`tauri icon 执行失败（exit=${res.status}）`)
}

// 移动端 iOS -> gen/apple AppIcon.appiconset（真实包用）
const iosSrc = join(tmp, 'ios')
const iosDst = join(tauriDir, 'gen', 'apple', 'Assets.xcassets', 'AppIcon.appiconset')
if (existsSync(iosSrc) && existsSync(iosDst)) {
  let n = 0
  for (const name of readdirSync(iosSrc)) {
    if (name === 'Contents.json') continue
    const f = join(iosSrc, name)
    if (statSync(f).isFile()) {
      copyFileSync(f, join(iosDst, name))
      n++
    }
  }
  console.log(`[icon-build] 已更新 ${n} 个 iOS 图标 -> ${iosDst}`)
}

// 移动端 Android -> icons/android（真实包用）
const andSrc = join(tmp, 'android')
const andDst = join(iconsDir, 'android')
if (existsSync(andSrc) && existsSync(andDst)) {
  let n = 0
  for (const dir of readdirSync(andSrc)) {
    const sd = join(andSrc, dir)
    if (!statSync(sd).isDirectory()) continue
    const dd = join(andDst, dir)
    mkdirSync(dd, { recursive: true })
    for (const name of readdirSync(sd)) {
      const f = join(sd, name)
      if (statSync(f).isFile()) {
        copyFileSync(f, join(dd, name))
        n++
      }
    }
  }
  console.log(`[icon-build] 已更新 ${n} 个 Android 图标 -> ${andDst}`)
}

// 兼容旧版 CLI 的 icons/ios 残留目录
const iosLegacy = join(iconsDir, 'ios')
if (existsSync(iosLegacy) && existsSync(iosSrc)) {
  let n = 0
  for (const name of readdirSync(iosSrc)) {
    if (name === 'Contents.json') continue
    const f = join(iosSrc, name)
    if (statSync(f).isFile()) {
      copyFileSync(f, join(iosLegacy, name))
      n++
    }
  }
  console.log(`[icon-build] 已同步 ${n} 个文件 -> icons/ios`)
}

rmSync(tmp, { recursive: true, force: true })
console.log('[icon-build] 完成：PC 桌面图标未改动，移动端（iOS/Android）已更新为透明背景')
