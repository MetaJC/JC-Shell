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

// 统一提升 JC Shell 版本号：以 package.json 为唯一数据源，
// 自动同步到 Cargo.toml / tauri.conf.json / tauri.conf.system.json。
//
// 用法（从仓库根 jc-shell/jc-shell-community 执行）：
//   仅把四个文件对齐到 package.json 当前版本（校验/修复不一致）：
//     node scripts/bump-version.mjs
//   提升版本号并同步全部文件：
//     node scripts/bump-version.mjs 0.1.2
//
// 作者：JC
import { readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

const root = process.cwd()
const pkgPath = join(root, 'package.json')
const cargoPath = join(root, 'src-tauri', 'Cargo.toml')
const confPaths = [
  join(root, 'src-tauri', 'tauri.conf.json'),
  join(root, 'src-tauri', 'tauri.conf.system.json'),
]

const SEMVER = /^\d+\.\d+\.\d+([.-][0-9A-Za-z.-]+)?$/

function read(p) {
  return readFileSync(p, 'utf8')
}
function write(p, s) {
  writeFileSync(p, s, 'utf8')
}

// 解析参数
const arg = process.argv[2]
let target = null
if (arg) {
  if (!SEMVER.test(arg)) {
    console.error(`[bump] 非法版本号: "${arg}"（需形如 x.y.z）`)
    process.exit(1)
  }
  target = arg
}

// 1) package.json
const pkg = JSON.parse(read(pkgPath))
if (target && target !== pkg.version) {
  pkg.version = target
  write(pkgPath, JSON.stringify(pkg, null, 2) + '\n')
}
const version = pkg.version
console.log(`[bump] 目标版本 = ${version}`)

// 2) Cargo.toml  -> version = "x.y.z"
let cargo = read(cargoPath)
if (!cargo.includes(`version = "${version}"`)) {
  const next = cargo.replace(/^version\s*=\s*"[^"]*"\s*$/m, `version = "${version}"`)
  if (next === cargo) {
    console.error('[bump] 未在 Cargo.toml 找到 package.version，请检查文件结构')
    process.exit(1)
  }
  cargo = next
  write(cargoPath, cargo)
  console.log('[bump] 已更新 src-tauri/Cargo.toml')
} else {
  console.log('[bump] src-tauri/Cargo.toml 已一致，跳过')
}

// 3) tauri.conf*.json -> "version": "x.y.z"
for (const p of confPaths) {
  let s = read(p)
  if (!s.includes(`"version": "${version}"`)) {
    const next = s.replace(/"version":\s*"[^"]*"/, `"version": "${version}"`)
    if (next === s) {
      console.error(`[bump] 未在 ${p} 找到 version 字段`)
      process.exit(1)
    }
    s = next
    write(p, s)
    console.log(`[bump] 已更新 ${p.replace(root + '\\', '')}`)
  } else {
    console.log(`[bump] ${p.replace(root + '\\', '')} 已一致，跳过`)
  }
}

console.log(`\n[bump] 完成：四个文件均为 ${version}。`)
console.log('[bump] 下一步：npm run tauri build')
