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

// 生产 Android 包签名（兼容鸿蒙卓易通等需 v1/JAR 签名的解析层）
// ------------------------------------------------------------------
// 流程：Gradle 已产出带 v2/v3 签名的 app-universal-release.apk，
// 这里对其 zipalign 后用 apksigner 一次性签出 v1+v2+v3（三方案并存），
// 再用 jarsigner 权威验证 v1 有效性（apksigner verify 的 v1 行会误报 false）。
//
// 签名口令：依次尝试 KS_PASS → JC_ANDROID_KEYSTORE_PASSWORD → keystore.properties
// （由 android-signing.mjs 写入，password 字段）；都没有才在终端交互输入（不回显）。
// keystore 路径/别名：JC_ANDROID_KEYSTORE / JC_ANDROID_KEY_ALIAS → keystore.properties → 默认值。
// ------------------------------------------------------------------
import { spawnSync } from 'node:child_process'
import { existsSync, readFileSync, readdirSync } from 'node:fs'
import { join } from 'node:path'
import { homedir } from 'node:os'
import { stdin, stdout } from 'node:process'

const ANDROID_HOME = process.env.ANDROID_HOME || 'D:\\Android'

// 控制字符码值（避免源文件里出现裸控制字节）
const CH_ENTER = 0x0a
const CH_CR = 0x0d
const CH_EOT = 0x04 // Ctrl+D 结束
const CH_ETX = 0x03 // Ctrl+C 中止
const CH_BS = 0x08 // 退格
const CH_DEL = 0x7f // DEL

/** 选最高版本的 build-tools（如 36.0.0） */
function findBuildTools() {
  const btDir = join(ANDROID_HOME, 'build-tools')
  if (!existsSync(btDir)) {
    throw new Error(`找不到 Android build-tools：${btDir}`)
  }
  const versions = readdirSync(btDir)
    .filter(name => /^\d+\.\d+\.\d+$/.test(name))
    .sort((a, b) => {
      const pa = a.split('.').map(Number)
      const pb = b.split('.').map(Number)
      for (let i = 0; i < 3; i++) {
        if (pb[i] !== pa[i]) return pb[i] - pa[i]
      }
      return 0
    })
  if (versions.length === 0) {
    throw new Error(`build-tools 目录下无任何 x.y.z 版本：${btDir}`)
  }
  return join(btDir, versions[versions.length - 1])
}

/** 终端隐藏输入读取口令 */
function promptPassword(promptText) {
  return new Promise(resolve => {
    stdout.write(promptText)
    stdin.resume()
    stdin.setRawMode(true)
    let pw = ''
    const onData = chunk => {
      const code = chunk[0]
      if (code === CH_ENTER || code === CH_CR || code === CH_EOT) {
        stdout.write('\n')
        stdin.setRawMode(false)
        stdin.removeListener('data', onData)
        stdin.pause()
        resolve(pw)
      } else if (code === CH_ETX) {
        stdout.write('\n')
        stdin.setRawMode(false)
        stdin.pause()
        process.exit(1)
      } else if (code === CH_BS || code === CH_DEL) {
        pw = pw.slice(0, -1)
      } else {
        pw += chunk.toString()
      }
    }
    stdin.on('data', onData)
  })
}

/** 解析 gen/android/keystore.properties（android-signing.mjs 生成，Gradle 签名配置来源） */
function readKeystoreProps() {
  const propsFile = join(process.cwd(), 'src-tauri', 'gen', 'android', 'keystore.properties')
  if (!existsSync(propsFile)) return {}
  const props = {}
  for (const line of readFileSync(propsFile, 'utf8').split(/\r?\n/)) {
    const t = line.trim()
    if (!t || t.startsWith('#') || t.startsWith('!')) continue
    const i = t.indexOf('=')
    if (i === -1) continue
    // Java properties 反斜杠转义还原（写入时对 \ 做了转义）
    props[t.slice(0, i).trim()] = t.slice(i + 1).trim().replace(/\\(.)/g, '$1')
  }
  return props
}

function fail(msg) {
  console.error(`[android-sign-prod] ${msg}`)
  process.exit(1)
}

async function main() {
  const buildTools = findBuildTools()
  const zipalign = join(buildTools, 'zipalign.exe')
  const apksigner = join(buildTools, 'apksigner.bat')
  if (!existsSync(zipalign)) fail(`缺少 zipalign：${zipalign}`)
  if (!existsSync(apksigner)) fail(`缺少 apksigner：${apksigner}`)

  const genAndroid = join(process.cwd(), 'src-tauri', 'gen', 'android')
  const releaseDir = join(genAndroid, 'app', 'build', 'outputs', 'apk', 'universal', 'release')
  const src = join(releaseDir, 'app-universal-release.apk')
  const out = join(releaseDir, 'aligned.apk')
  if (!existsSync(src)) {
    fail(`找不到待签名 APK：${src}\n请先执行 npm run tauri:android:build:apk 生成它。`)
  }

  // keystore 信息与口令：环境变量优先，其次 keystore.properties，最后交互输入
  const props = readKeystoreProps()
  const keystore = process.env.JC_ANDROID_KEYSTORE || props.storeFile
    || join(homedir(), 'jc-shell-android.jks')
  const alias = process.env.JC_ANDROID_KEY_ALIAS || props.keyAlias || 'upload'
  let ksPass = process.env.KS_PASS || process.env.JC_ANDROID_KEYSTORE_PASSWORD || props.password
  if (!ksPass) {
    ksPass = await promptPassword('请输入 Android 签名口令（不回显）: ')
  }
  if (!ksPass) fail('未提供签名口令，已中止。')
  process.env.KS_PASS = ksPass
  if (!existsSync(keystore)) fail(`找不到 keystore：${keystore}`)

  // 1) 对齐
  console.log('[android-sign-prod] zipalign ...')
  const zr = spawnSync(zipalign, ['-f', '-p', '4', src, out], { stdio: 'inherit' })
  if (zr.status !== 0) fail(`zipalign 失败（exit ${zr.status}）`)

  // 2) 一次性签 v1+v2+v3（.bat 需 shell:true）
  console.log('[android-sign-prod] apksigner sign v1+v2+v3 ...')
  const sr = spawnSync(
    apksigner,
    [
      'sign',
      '--v1-signing-enabled', 'true',
      '--v2-signing-enabled', 'true',
      '--v3-signing-enabled', 'true',
      '--ks', keystore,
      '--ks-key-alias', alias,
      '--ks-pass', 'env:KS_PASS',
      '--key-pass', 'env:KS_PASS',
      out,
    ],
    { stdio: 'inherit', shell: true }
  )
  if (sr.status !== 0) fail(`apksigner 失败（exit ${sr.status}）`)

  // 3) jarsigner 权威验证 v1
  console.log('[android-sign-prod] jarsigner -verify（v1 权威）...')
  const vr = spawnSync('jarsigner', ['-verify', out], { stdio: 'inherit' })
  if (vr.status !== 0) fail(`jarsigner 验证失败（exit ${vr.status}）`)

  console.log(`\n[android-sign-prod] 完成 生产包（v1+v2+v3）: ${out}`)
}

main().catch(e => fail(e && e.stack ? e.stack : String(e)))
