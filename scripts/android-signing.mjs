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

// 安卓 release 签名一键配置
// 作者: JC
//
// 作用（幂等，可重复执行）：
//   1. 生成 keystore（若不存在且提供了密码）
//   2. 写入 src-tauri/gen/android/keystore.properties
//   3. 给 src-tauri/gen/android/app/build.gradle.kts 打签名补丁（v2+v3；v1 在本工具链不出，见下方说明）
//
// 为什么需要：`gen/` 被 gitignore，`tauri android init` 重建后会丢掉签名配置；
// Tauri 在缺少 keystore.properties 时不签名 → 产出 app-*-unsigned.apk，
// 装到手机上（含鸿蒙卓易通）会报「解析包时出现问题」。
//
// 用法：
//   $env:JC_ANDROID_KEYSTORE_PASSWORD = "你的密码"
//   npm run android:sign:setup
//   可选：--keystore=<jks 路径>  --alias=<别名>  --password=<密码>
//   等价环境变量：JC_ANDROID_KEYSTORE / JC_ANDROID_KEY_ALIAS / JC_ANDROID_KEYSTORE_PASSWORD
//
// 说明：密码只写入本机 keystore.properties（gen/ 内，不入库），脚本不回显密码。
import { existsSync, readFileSync, writeFileSync, mkdirSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { homedir } from 'node:os'
import { execFileSync } from 'node:child_process'

const here = dirname(fileURLToPath(import.meta.url))
const srcFrontend = resolve(here, '..')
const androidDir = join(srcFrontend, 'src-tauri', 'gen', 'android')
const gradleFile = join(androidDir, 'app', 'build.gradle.kts')
const propsFile = join(androidDir, 'keystore.properties')

const isWin = process.platform === 'win32'
const args = process.argv.slice(2)
const argOf = (name) => {
  const hit = args.find((a) => a.startsWith(`--${name}=`))
  return hit ? hit.slice(name.length + 3) : ''
}

const alias = process.env.JC_ANDROID_KEY_ALIAS || argOf('alias') || 'upload'
const password = process.env.JC_ANDROID_KEYSTORE_PASSWORD || argOf('password') || ''
const keystorePath = resolve(
  process.env.JC_ANDROID_KEYSTORE || argOf('keystore') || join(homedir(), 'jc-shell-android.jks')
)

const log = (msg) => console.log(`[android-sign] ${msg}`)
const warn = (msg) => console.warn(`[android-sign] ${msg}`)

if (!existsSync(gradleFile)) {
  warn(`找不到 ${gradleFile}`)
  warn('先执行 npm run tauri:android:init 生成安卓工程，再跑本脚本。')
  process.exit(0) // 不阻断调用方（如 init 后自动执行）
}

// ---------------------------------------------------------------------------
// 1. keystore：不存在则用 keytool 生成
// ---------------------------------------------------------------------------
function findKeytool() {
  const bin = isWin ? 'keytool.exe' : 'keytool'
  const candidates = []
  if (process.env.JAVA_HOME) candidates.push(join(process.env.JAVA_HOME, 'bin', bin))
  candidates.push(bin)
  for (const cmd of candidates) {
    if (cmd.includes('/') || cmd.includes('\\')) {
      if (existsSync(cmd)) return cmd
      continue
    }
    try {
      // 走 PATH：能跑起来即视为可用（找不到会抛 ENOENT）
      execFileSync(cmd, ['-help'], { stdio: 'ignore' })
      return cmd
    } catch (e) {
      if (e && e.code !== 'ENOENT') return cmd // 非 0 退出也算存在
    }
  }
  return ''
}

if (!existsSync(keystorePath)) {
  if (!password) {
    warn(`keystore 不存在：${keystorePath}`)
    warn('请设置 JC_ANDROID_KEYSTORE_PASSWORD（或 --password=）后重跑，脚本会自动生成。')
  } else {
    const keytool = findKeytool()
    if (!keytool) {
      warn('未找到 keytool：请确认 JAVA_HOME 指向 JDK，或把 keytool 加入 PATH。')
    } else {
      log(`生成 keystore：${keystorePath}`)
      execFileSync(keytool, [
        '-genkeypair', '-v',
        '-keystore', keystorePath,
        '-storetype', 'JKS',
        '-keyalg', 'RSA',
        '-keysize', '2048',
        '-validity', '10000',
        '-alias', alias,
        '-storepass', password,
        '-keypass', password,
        '-dname', 'CN=JC Shell, OU=Mobile, O=OpenJC, L=Beijing, ST=Beijing, C=CN',
      ], { stdio: 'inherit' })
    }
  }
} else {
  log(`keystore 已存在：${keystorePath}`)
}

// ---------------------------------------------------------------------------
// 2. keystore.properties（Tauri 靠它决定是否签名 release 包）
// ---------------------------------------------------------------------------
if (existsSync(keystorePath) && password) {
  // ⚠️ .properties 是 Java 的格式：反斜杠是转义符，`D:\Users\...` 里的 `\U`/`\j` 属于
  // 「未知转义」，反斜杠会被直接丢弃 → Java 读到 `D:UsersJC...` → AGP 报找不到 keystore。
  // 直接把路径写成正斜杠（Windows 下 Java/AGP 同样认），彻底绕开转义问题。
  const toPropValue = (v) => String(v).replace(/\\/g, '/')
  const escapeProp = (v) => String(v).replace(/\\/g, '\\\\')
  const content = [
    '# 由 scripts/android-signing.mjs 生成（gen/ 不入库，勿提交）',
    `keyAlias=${escapeProp(alias)}`,
    `password=${escapeProp(password)}`,
    `storeFile=${toPropValue(keystorePath)}`,
    '',
  ].join('\n')
  writeFileSync(propsFile, content, 'utf8')
  log(`已写入 ${propsFile}`)
} else if (!password) {
  warn('未提供密码 → 跳过 keystore.properties（release 包仍会是 unsigned）')
}

// ---------------------------------------------------------------------------
// 3. gradle 补丁：release 签名 + v1/v2/v3
//    （init 重建工程后自动补回；已打过补丁则跳过）
// ---------------------------------------------------------------------------
const MARK = 'JC_ANDROID_SIGNING_BEGIN'

function ensureGradlePatch() {
  let text = readFileSync(gradleFile, 'utf8')
  if (text.includes(MARK)) {
    log('build.gradle.kts 已有签名补丁，跳过')
    return
  }

  const importBlock = [
    'import java.io.FileInputStream',
    '',
  ].join('\n')
  const propsBlock = [
    '',
    `// [${MARK}]`,
    '// release 签名：读取 gen/android/keystore.properties（由 npm 脚本 android:sign:setup 生成）。',
    '// 缺文件时保持未签名构建（不阻断），存在则 release 包带 v2+v3 签名（AGP 8.11 默认，覆盖现代 Android 7.0+）：',
    '//   注：minSdk>=24 时 AGP 8.11 与 apksigner 36 均不产出 v1(JAR) 签名；',
    '//   仅老解析器 / 鸿蒙卓易通等兼容层需要 v1，须用 jarsigner 补 v1 再 apksigner 补 v2/v3（见文档 2.4）。',
    'val keystorePropertiesFile = rootProject.file("keystore.properties")',
    'val keystoreProperties = Properties()',
    'val hasReleaseKeystore = keystorePropertiesFile.exists()',
    'if (hasReleaseKeystore) {',
    '    FileInputStream(keystorePropertiesFile).use { keystoreProperties.load(it) }',
    '}',
    `// [${MARK}_END]`,
    '',
    'android {',
  ].join('\n')
  const signingBlock = [
    `    // [${MARK}] 仅在 keystore.properties 存在时注册，否则 AGP 会因 storeFile 为空而构建失败`,
    '    signingConfigs {',
    '        if (hasReleaseKeystore) {',
    '            create("release") {',
    '                keyAlias = keystoreProperties.getProperty("keyAlias")',
    '                keyPassword = keystoreProperties.getProperty("password")',
    '                storeFile = file(keystoreProperties.getProperty("storeFile"))',
    '                storePassword = keystoreProperties.getProperty("password")',
    '                // enableV1Signing = true  // AGP 8.11 对 minSdk>=24 不产出 v1，留作意图记录（无副作用）',
    '                enableV2Signing = true',
    '                enableV3Signing = true',
    '            }',
    '        }',
    '    }',
    `    // [${MARK}_END]`,
    '    buildTypes {',
  ].join('\n')
  const releaseSigning = [
    '        getByName("release") {',
    `            // [${MARK}]`,
    '            if (hasReleaseKeystore) {',
    '                signingConfig = signingConfigs.getByName("release")',
    '            }',
    `            // [${MARK}_END]`,
  ].join('\n')

  const before = text

  // a) import
  if (!text.includes('import java.io.FileInputStream')) {
    text = text.replace(/^import java\.util\.Properties[ \t]*\r?\n/m, (m) => m + importBlock)
  }
  // b) android 之前插入 keystore.properties 读取
  text = text.replace(/\r?\nandroid \{/, propsBlock)
  // c) buildTypes 之前插入 signingConfigs（此时 buildTypes 前已多出 android { 行，仍唯一）
  text = text.replace(/\r?\n(\s*)buildTypes \{/, `\n${signingBlock}`)
  // d) release 里挂 signingConfig
  text = text.replace(/(\r?\n\s*)getByName\("release"\) \{/, `\n${releaseSigning}`)

  if (text === before) {
    warn('未能自动插入补丁（gradle 模板结构可能已变），请按 docs/安卓调试与打包.md 2.4 手工添加')
    return
  }
  writeFileSync(gradleFile, text, 'utf8')
  log('已给 app/build.gradle.kts 打上签名补丁（v2+v3）')
}

ensureGradlePatch()

// ---------------------------------------------------------------------------
// 4. 收尾提示
// ---------------------------------------------------------------------------
console.log('')
if (existsSync(propsFile)) {
  log('签名已配置完成。重新打包即可（产物名不再带 unsigned）：')
  console.log('    npm run tauri:android:build:apk -- -t aarch64')
  console.log('  验签：')
  console.log('    & "$env:ANDROID_HOME\\build-tools\\36.0.0\\apksigner.bat" verify -v <apk 路径>')
} else {
  log('尚未写入 keystore.properties（缺密码/keystore）→ release 包仍会是未签名。')
  console.log('    $env:JC_ANDROID_KEYSTORE_PASSWORD = "你的密码"; npm run android:sign:setup')
}
