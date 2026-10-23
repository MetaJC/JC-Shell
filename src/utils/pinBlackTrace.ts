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
 * 输入密码时黑屏的临时诊断线索（定位后应删除）。
 * 作者：JC
 *
 * 为什么需要它：黑屏发生时页面很可能已经不在绘制（露出的正是窗口背景色 #1f1f23），
 * 控制台 / DOM 都读不到；而鸿蒙 + 卓易通环境下也不一定能挂 adb 看日志。
 * 所以这里把关键事件写进 localStorage（跨「杀进程重开」保留），在设置里回显，
 * 用来判断黑屏到底落在「键盘避让」还是「生物识别原生页」这条路径上：
 *
 *   kb:open / kb:close      键盘状态翻转（带视口几何：ih=innerHeight vvh=可视高度 off=视口顶偏移）
 *   bio:start / bio:end     生物识别弹窗（原生页）开始 / 结束；只有 start 没有 end = 原生页没回来
 *   pin:open / pin:focus    PIN 弹窗打开 / 输入框拿到焦点（键盘即将弹出）
 *   err:*                   JS 异常
 *
 * 上限 10 条、只保留最近记录，正常使用时对性能无影响。
 */
import { isMobileWebview } from './platform'

const KEY = 'jc:pin-black-trace'
const MAX = 10

/** 追加一条线索（非移动端不记录；任何异常都不外抛） */
export function tracePinBlack(evt: string): void {
  if (!isMobileWebview()) return
  try {
    const vv = window.visualViewport
    const geo = vv
      ? ` ih=${window.innerHeight} vvh=${Math.round(vv.height)} off=${Math.round(vv.offsetTop)}`
      : ` ih=${window.innerHeight}`
    const t = new Date()
    const pad = (n: number) => String(n).padStart(2, '0')
    const ts = `${pad(t.getHours())}:${pad(t.getMinutes())}:${pad(t.getSeconds())}`
    const list = readPinBlackTrace()
    list.push(`${ts} ${evt}${geo}`)
    window.localStorage.setItem(KEY, JSON.stringify(list.slice(-MAX)))
  } catch { /* ignore */ }
}

/** 读取线索（最早 → 最新） */
export function readPinBlackTrace(): string[] {
  try {
    const raw = window.localStorage.getItem(KEY)
    if (!raw) return []
    const parsed: unknown = JSON.parse(raw)
    return Array.isArray(parsed) ? parsed.filter((x): x is string => typeof x === 'string') : []
  } catch {
    return []
  }
}

/** 清空线索（设置页提供，方便干净地再复现一次） */
export function clearPinBlackTrace(): void {
  try { window.localStorage.removeItem(KEY) } catch { /* ignore */ }
}

/** 装全局异常兜底：JS 异常（尤其 WebView 重建前的最后一个错误）是最值得留的一条线索 */
export function installPinBlackTrace(): void {
  if (!isMobileWebview()) return
  window.addEventListener('error', (e) => {
    tracePinBlack(`err:${e.message || 'unknown'}@${e.filename || ''}:${e.lineno || 0}`)
  })
  window.addEventListener('unhandledrejection', (e) => {
    const r = (e as PromiseRejectionEvent).reason
    tracePinBlack(`rej:${String(r && (r as Error).message ? (r as Error).message : r).slice(0, 80)}`)
  })
}
