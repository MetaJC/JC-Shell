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

// 加载 .env 文件到 process.env，然后启动指定命令
// 用法: node scripts/with-env.mjs <env-file> -- <command> [args...]
// 作者: JC
import { spawn } from 'node:child_process'
import { readFileSync, existsSync } from 'node:fs'
import { join, delimiter } from 'node:path'

const args = process.argv.slice(2)
const sepIdx = args.indexOf('--')
if (sepIdx === -1) {
  console.error('Usage: node scripts/with-env.mjs <env-file> -- <command> [args...]')
  process.exit(1)
}

const envFile = args[0]
const cmdArgs = args.slice(sepIdx + 1)
const cmd = cmdArgs[0]
const cmdRest = cmdArgs.slice(1)

/**
 * 加载 .env 文件到 process.env（不覆盖已存在的环境变量）
 * 支持 KEY=VALUE 与 KEY="VALUE" 格式，跳过空行和 # 注释
 */
function loadEnvFile(file) {
  if (!existsSync(file)) {
    console.warn(`[with-env] file not found: ${file} (keep existing process.env)`)
    return
  }
  const text = readFileSync(file, 'utf8')
  let count = 0
  for (const rawLine of text.split(/\r?\n/)) {
    const line = rawLine.trim()
    if (!line || line.startsWith('#')) continue
    const m = line.match(/^([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)$/)
    if (!m) continue
    const [, key, valueRaw] = m
    const value = valueRaw.replace(/^['"]|['"]$/g, '').trim()
    if (process.env[key] === undefined) {
      process.env[key] = value
      count++
    }
  }
  console.log(`[with-env] loaded ${count} keys from ${file}`)
}

loadEnvFile(envFile)
console.log(`[with-env] OPENJC_SERVER_BASE_URL=${process.env.OPENJC_SERVER_BASE_URL || '(unset)'}`)

// ---------------------------------------------------------------------------
// 关键修复：把 ./node_modules/.bin 加到 PATH，并显式解析 Windows 下的 .cmd 包装。
//
// 问题：npm run 执行脚本时虽然会自动把 node_modules/.bin 加到 PATH，
//       但 Node.js 在 Windows 下 spawn() 不会自动加 .cmd 后缀（CreateProcessW
//       只补 .exe 不补 .cmd），导致 spawn('tauri', ...) 报 ENOENT。
// 方案：
//   1. prepend node_modules/.bin 到 PATH（跨平台一致）
//   2. Windows 下若 cmd 不带扩展名且 PATH 某目录存在 <cmd>.cmd，则显式用 .cmd
// 副作用：跨平台一致、不依赖 shell、零命令注入风险。
// ---------------------------------------------------------------------------
const localBin = join(process.cwd(), 'node_modules', '.bin')
// ---------------------------------------------------------------------------
// Xcode 26 + Intel 出 iOS 包：cargo-mobile2 在真机(iphoneos)构建时不传 -destination，
// Xcode 26 不再自动回退到 "Any iOS Device" → `Found no destinations`。
// 仓库根 tools/bin/xcodebuild 是个 wrapper，会给这类调用补上
// `-destination generic/platform=iOS`（详见 docs/iOS上架AppStore.md §4）。
// 把它放在 PATH 最前，让 tauri 调用的 xcodebuild 命中 wrapper；
// 目录不存在时透明跳过（Windows/Linux 不受影响）。
// ---------------------------------------------------------------------------
const wrapperBin = join(process.cwd(), '..', 'tools', 'bin')
const pathEntries = []
if (existsSync(wrapperBin)) {
  pathEntries.push(wrapperBin)
  console.log(`[with-env] PATH += ${wrapperBin} (xcodebuild wrapper)`)
}
pathEntries.push(localBin)
const enhancedEnv = {
  ...process.env,
  PATH: `${pathEntries.join(delimiter)}${delimiter}${process.env.PATH || ''}`,
}
if (existsSync(localBin)) {
  console.log(`[with-env] PATH += ${localBin}`)
}

let actualCmd = cmd
let actualArgs = cmdRest
const isWin = process.platform === 'win32'
if (isWin && !/\.(cmd|exe|bat|ps1)$/i.test(cmd)) {
  // 仅当 cmd 是纯名（无路径分隔符、无扩展名）才需要补 .cmd；
  // 已经是绝对路径或带扩展名时，原样透传
  if (!cmd.includes('/') && !cmd.includes('\\')) {
    // 优先找 .cmd（npm scripts 生成的是 .cmd 包装）
    const cmdDirs = enhancedEnv.PATH.split(delimiter)
    for (const dir of cmdDirs) {
      if (!dir) continue
      const candidate = join(dir, `${cmd}.cmd`)
      if (existsSync(candidate)) {
        actualCmd = candidate
        // Windows 下 .cmd / .bat 必须通过 cmd.exe /c 调用，
        // 否则 Node 24 报 EINVAL（不开启 shell: true，避免参数未转义的 DEP0190 警告）
        actualArgs = ['/c', candidate, ...cmdRest]
        actualCmd = process.env.ComSpec || 'cmd.exe'
        console.log(`[with-env] resolved \`${cmd}\` -> ${candidate}`)
        break
      }
    }
  }
}

const child = spawn(actualCmd, actualArgs, {
  stdio: 'inherit',
  env: enhancedEnv,
})
child.on('exit', (code) => process.exit(code ?? 0))
child.on('error', (err) => {
  console.error(`[with-env] failed to spawn \`${cmd}\`: ${err.message}`)
  console.error('[with-env] 排查：1) 是否执行了 npm install？ 2) 确认 @tauri-apps/cli 已写入 package.json devDependencies')
  process.exit(1)
})
