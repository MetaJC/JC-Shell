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
 * 移动端软键盘检测：键盘弹出时给根元素加 .jc-kb-open 类，
 * 并写入 CSS 变量 --jc-kb-overlap（键盘遮挡高度）、--jc-vv-top（可视视口顶部偏移）
 * 与 --jc-vv-bottom（可视底边，键盘上沿），
 * 供移动端弹窗避让（mobile.css 第 13 节）与终端页避让（第 21 节）使用。
 * 作者：JC
 *
 * 三条来源（取或，任一命中即认为键盘弹出）：
 * 1. visualViewport 底边上移（adjustPan 类机型：布局视口不变，可视视口被压缩）；
 * 2. innerHeight 变小（adjustResize 类机型：布局视口同步缩小）；
 * 3. 原生上报的键盘高度（见下）——**部分 Android WebView 键盘弹出时视口一个数都不变**，
 *    前两条完全不成立，只有原生能拿到 IME 状态。
 *
 * 事件兜底：个别 WebView 键盘弹出时数值变了却不发 resize/scroll 事件，
 * 键盘只可能因输入焦点弹出，因此在「输入类元素持有焦点」期间以 200ms 轮询数值兜底，
 * 保证这类机型上 .jc-kb-open 与 jc-kb-toggle 事件依然可靠。
 */
import { isMobileWebview } from './platform'
import { tracePinBlack } from './pinBlackTrace'

/** 键盘状态变化事件名（detail: { open: boolean }），终端页/布局据此同步避让 */
export const KB_TOGGLE_EVENT = 'jc-kb-toggle'

// ===== 原生上报的键盘高度 =====
/**
 * 由 MainActivity 通过 evaluateJavascript 调 window.__jcOnKeyboardInset(imePx, framePx, kbPx) 写入。
 *
 * 为什么必须走原生：实测部分机型 WebView 视口完全不随键盘变化
 * （innerHeight / visualViewport / clientHeight 一个数都不动），前端再怎么测也测不到键盘；
 * 原生侧能拿到 IME 的 insets / 可见显示区，直接把高度报进来最可靠。
 * 传入三个数（insets / 可见区差值 / 生效值）便于排查，这里只用生效值；
 * 数值是物理像素，转 CSS px 时除以 devicePixelRatio（见 nativeKeyboardCssPx）。
 */
let nativeKbPx = 0

/** 原生报告的键盘高度（CSS px，已按 devicePixelRatio 换算；0 表示键盘未弹出） */
export function nativeKeyboardCssPx(): number {
  if (nativeKbPx <= 0) return 0
  const dpr = window.devicePixelRatio || 1
  return Math.round(nativeKbPx / dpr)
}

/**
 * 订阅「键盘几何数值已刷新」。
 * 必须由 apply() 统一回调，而不是让使用方自己监听 visualViewport 事件：
 * 事件在个别机型上根本不发（见文件头注释），只有跟着轮询兜底一起走才可靠。
 * 回调里只做轻量调度（如 rAF 合并），不要在回调里直接读写布局。
 */
type KbMetricsListener = () => void
const kbMetricsListeners = new Set<KbMetricsListener>()

export function onKeyboardMetricsChange(cb: KbMetricsListener): () => void {
  kbMetricsListeners.add(cb)
  return () => { kbMetricsListeners.delete(cb) }
}

/** 键盘遮挡判定阈值（px）：双指缩放 / 轻微滚动造成的视口抖动不超过它，避免误判 */
const KB_OVERLAP_MIN = 80
/** 输入焦点持有期间的兜底轮询间隔（毫秒） */
const KB_POLL_MS = 200

/** 目标元素是否输入类控件（xterm 的辅助输入框也是 textarea，天然覆盖） */
function isEditableEl(el: EventTarget | null): boolean {
  return el instanceof HTMLElement && el.closest('input, textarea, [contenteditable="true"]') !== null
}

export function installMobileKeyboardShift(): void {
  if (!isMobileWebview()) return
  const vv = window.visualViewport
  if (!vv) return
  const root = document.documentElement

  /** 完整视口高度基线：键盘弹出会使 innerHeight / vv.height 变小，取历史最大值 */
  let fullH = 0
  /** 当前判定出的键盘状态（仅在状态翻转时派发事件，避免轮询重复轰炸） */
  let kbOpen = false

  const apply = () => {
    const visibleBottom = vv.height + vv.offsetTop
    fullH = Math.max(fullH, window.innerHeight, visibleBottom)
    const overlap = Math.max(0, fullH - visibleBottom)
    // 两条判定取或：视觉视口被压缩（overlap）或 原生直接报到键盘高度（nativeKbPx）
    const open = overlap > KB_OVERLAP_MIN || nativeKbPx > 0
    root.classList.toggle('jc-kb-open', open)
    root.style.setProperty('--jc-kb-overlap', `${overlap}px`)
    root.style.setProperty('--jc-vv-top', `${vv.offsetTop}px`)
    // 可视底边（布局视口坐标系）：键盘弹出时它就是键盘上沿的位置。
    // 终端页直接按它算高度，不依赖 innerHeight / adjustResize / adjustPan 的差异 ——
    // 实测部分机型 WebView 在「布局不重排（adjustPan）」时也会把 innerHeight 报小，
    // 用 innerHeight 判断是否已让出高度会算成 0，补偿失效。
    root.style.setProperty('--jc-vv-bottom', `${visibleBottom}px`)
    // 「前端还需自行让出的底部高度」= 可视区被键盘占掉的高度 − 布局已经让出的高度。
    // 用途：把弹窗贴到键盘上沿之上（mobile.css 第 13 节）。
    // 必须扣掉已让出的部分：原生把 IME 高度做成内容视图内边距（adjustResize）时，
    // 布局视口已经缩短、遮罩层本来就只到键盘上沿，再按 overlap 让一次会重复扣成两个键盘高，
    // 弹窗会被顶到屏幕中间偏上。adjustPan 机型 layoutShrink 为 0，直接等于键盘高度。
    const layoutShrink = Math.max(0, fullH - window.innerHeight)
    // 再按「可视高度的一半」收敛：兼容层 / 个别 ROM 上偶发把整屏差值当键盘（原生侧
    // MainActivity 有同名收敛逻辑），不收敛会算出比屏幕还高的让位量，把弹窗顶出屏幕外。
    const avoid = Math.min(Math.max(0, overlap - layoutShrink), Math.max(0, visibleBottom) / 2)
    root.style.setProperty('--jc-kb-avoid', `${avoid}px`)
    // 通知订阅方（终端页图层避让等）：无论本次是事件触发还是轮询兜底触发
    kbMetricsListeners.forEach(cb => cb())
    if (open !== kbOpen) {
      kbOpen = open
      // 【临时】黑屏诊断：记录键盘状态翻转及其时的视口几何（见 utils/pinBlackTrace.ts）
      tracePinBlack(open ? 'kb:open' : 'kb:close')
      document.dispatchEvent(new CustomEvent(KB_TOGGLE_EVENT, { detail: { open } }))
    }
  }

  vv.addEventListener('resize', apply)
  vv.addEventListener('scroll', apply)
  window.addEventListener('resize', apply)
  // 旋转屏后视口全高变化，重置基线避免旧 max 造成误判
  window.addEventListener('orientationchange', () => { fullH = 0 })

  // 原生 → 前端：MainActivity 把键盘高度直接报进来（见其 applyImeResize）。
  // 视口不随键盘变化的机型上这是唯一的键盘来源；报到 > 0 即视为「键盘弹出」，
  // 与 vv 判定取或，`open` 一翻转就会派发 KB_TOGGLE_EVENT / 通知避让订阅方。
  ;(window as unknown as Record<string, unknown>).__jcOnKeyboardInset =
    (_imePx: number, _framePx: number, kbPx: number) => {
      nativeKbPx = Math.max(0, Math.round(Number(kbPx) || 0))
      apply()
    }

  // 兜底轮询：只在输入焦点持有期间运行（无焦点不可能有软键盘，轮询无意义）。
  // 注意 Android 上收起键盘不一定触发 blur，因此仅靠 vv 事件可能漏掉「收起」，
  // 轮询同样覆盖这一返程路径。
  let pollTimer: number | null = null
  const stopPoll = () => {
    if (pollTimer !== null) { clearInterval(pollTimer); pollTimer = null }
  }
  document.addEventListener('focusin', (e) => {
    if (!isEditableEl(e.target)) return
    apply()
    if (pollTimer === null) pollTimer = window.setInterval(apply, KB_POLL_MS)
  })
  document.addEventListener('focusout', () => {
    // 焦点可能转移到另一个输入框：稍后确认无输入焦点再停轮询
    setTimeout(() => {
      if (!isEditableEl(document.activeElement)) stopPoll()
      apply()
    }, 0)
  })
  apply()
}
