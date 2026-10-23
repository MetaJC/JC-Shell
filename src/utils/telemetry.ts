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

/**
 * 客户端使用遥测（启动 / 心跳 / 退出）
 * 作者：JC
 *
 * 后台据此统计 DAU / MAU、每日使用时长、24 小时在线曲线。
 * 设计约束：
 *  1. 全程静默 —— 任何失败只 console.warn，不影响软件正常使用；
 *  2. 非 Tauri 环境（纯浏览器开发）直接跳过；
 *  3. 只上传设备标识 / 版本 / 操作系统类别 / 系统版本 / 设备型号 / 登录态，不采集用户内容。
 */
import { invoke, isTauriEnv } from '@/api'
import { deviceId } from '@/utils/deviceId'
import { detectPlatform } from '@/stores/update'
import { platformType } from '@/utils/platform'

let timer: number | null = null
let started = false

/** 当前版本号（与更新弹窗口径一致：Vite 注入的 __APP_VERSION__） */
function currentVersion(): string {
  const injected: unknown = __APP_VERSION__
  if (typeof injected === 'string' && injected.trim()) return injected.trim()
  const viteEnv: unknown = import.meta.env.VITE_APP_VERSION
  if (typeof viteEnv === 'string' && viteEnv.trim()) return viteEnv.trim()
  return '0.0.0'
}

async function osVersion(): Promise<string | undefined> {
  try {
    const mod = await import('@tauri-apps/plugin-os')
    return await mod.version()
  } catch {
    return undefined
  }
}

/**
 * 操作系统类别：WINDOWS / MACOS / LINUX / ANDROID / IOS / HARMONY
 * 优先用 platformType()（Tauri OS 插件，最可靠）；鸿蒙经 UA 特征兜底识别
 * （卓易通等兼容层 UA 常带 HarmonyOS/OpenHarmony 字样）。
 */
function osFamily(): string {
  try {
    const ua = typeof navigator !== 'undefined' ? navigator.userAgent : ''
    if (/harmony|openharmony/i.test(ua)) return 'HARMONY'
  } catch { /* 非浏览器环境，忽略 */ }
  switch (platformType()) {
    case 'windows': return 'WINDOWS'
    case 'macos': return 'MACOS'
    case 'linux': return 'LINUX'
    case 'android': return 'ANDROID'
    case 'ios': return 'IOS'
    default: return 'UNKNOWN'
  }
}

/** 软件启动：建会话 + 启动心跳 */
export async function startTelemetry(): Promise<void> {
  if (!isTauriEnv() || started) return
  started = true
  try {
    const intervalSec = await invoke<number>(
      'telemetry_session_start',
      {
        clientId: deviceId(),
        platform: detectPlatform(),
        osFamily: osFamily(),
        osVersion: await osVersion(),
        appVersion: currentVersion(),
      },
      { timeoutMs: 8000 }
    )
    // 间隔以后端下发为准（默认 300s，见 ClientTelemetryServiceImpl.HEARTBEAT_INTERVAL_SECONDS）
    const secs = Math.max(60, Number(intervalSec) || 300)
    // 抖动 ±10%：海量客户端若同秒打齐会形成脉冲峰值（服务端 QPS ≈ 在线数 / 间隔）。
    // 抖动后请求在时间轴上摊平，峰值显著下降，且不影响时长统计精度（按实际间距累加）。
    const jittered = Math.round(secs * (0.9 + Math.random() * 0.2))
    stopHeartbeat()
    timer = window.setInterval(() => {
      void invoke('telemetry_session_heartbeat').catch(() => {})
    }, jittered * 1000)
  } catch (e) {
    // 遥测不可达不能影响启动：仅记日志
    console.warn('[jc-shell] telemetry start 失败（忽略）:', e)
  }
}

function stopHeartbeat(): void {
  if (timer !== null) {
    window.clearInterval(timer)
    timer = null
  }
}

/** 软件退出：结束会话（退出前调用，让最后一次时长落库） */
export async function stopTelemetry(): Promise<void> {
  stopHeartbeat()
  if (!started) return
  started = false
  try {
    await invoke('telemetry_session_end', undefined, { timeoutMs: 2500 })
  } catch (e) {
    console.warn('[jc-shell] telemetry end 失败（忽略）:', e)
  }
}
