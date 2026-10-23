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

/** 终端 Ctrl 批量操作（分屏同步）
 * 作者：JC
 * 分屏状态下：
 * - 按住 Ctrl：临时批量，松开即关；双击 Ctrl：切换常驻批量模式（再次双击关闭）
 * - 键盘输入：源终端 onData 的数据同步发送到本窗口所有其他会话
 * - 鼠标操作：源终端区内的真实事件（点击/拖拽/双击/滚轮）按相对坐标
 *   复制分发到其他分屏的 xterm 屏幕元素
 * 退出分屏/窗口失焦时自动关闭。
 */
import { reactive } from 'vue'

/** 全局同步状态（当前 webview 内共享）：active=当前生效；sticky=双击 Ctrl 进入的常驻模式 */
export const termSync = reactive({ active: false, sticky: false })

// ===== 键盘输入广播 =====
type SyncInputHandler = (data: string, sourceSessionId: string) => void
const inputHandlers = new Set<SyncInputHandler>()

/** 注册输入广播处理（工作区负责转发到其他会话），返回注销函数 */
export function onSyncInput(fn: SyncInputHandler): () => void {
  inputHandlers.add(fn)
  return () => { inputHandlers.delete(fn) }
}

/** 源终端发出输入：同步开启时广播给所有注册方 */
export function emitSyncInput(data: string, sourceSessionId: string) {
  if (!termSync.active) return
  inputHandlers.forEach(fn => fn(data, sourceSessionId))
}

// ===== 文件系统折叠/尺寸广播 =====
/** 分屏内文件系统状态：折叠态 + 高度/宽度比例（相对值，便于跨分屏套用） */
export interface FsSyncPayload {
  sessionId: string
  collapsed: boolean
  heightRatio: number
  widthRatio: number
  /** 视图模式（list/column/tree/icon）：随工具栏视图按钮同步 */
  viewMode?: string
  /** fs↔终端联动开关 */
  syncEnabled?: boolean
  /** 自动刷新开关 */
  autoRefreshEnabled?: boolean
  /** 触发同步的工具栏按钮 key（用于刷新/缩放/导航等无状态动作的回放） */
  toolbarKey?: string
  /** 内容区横向滚动比例 0..1（相对值，跨分屏套用） */
  scrollX?: number
  /** 内容区纵向滚动比例 0..1 */
  scrollY?: number
}
/** 工具栏按钮同步描述：携带触发按钮 key 与当前相关状态快照，供其它分屏精确复现 */
export interface FsToolbarSync {
  key: string
  viewMode: string
  syncEnabled: boolean
  autoRefreshEnabled: boolean
}
type SyncFsHandler = (data: FsSyncPayload, sourceSessionId: string) => void
const fsHandlers = new Set<SyncFsHandler>()

/** 注册 fs 状态广播处理（工作区负责转发到其他分屏），返回注销函数 */
export function onSyncFs(fn: SyncFsHandler): () => void {
  fsHandlers.add(fn)
  return () => { fsHandlers.delete(fn) }
}

/** 源分屏发出 fs 状态：仅 Ctrl 批量模式（termSync.active）开启时广播给所有注册方 */
export function emitSyncFs(data: FsSyncPayload) {
  if (!termSync.active) return
  fsHandlers.forEach(fn => fn(data, data.sessionId))
}

// ===== 终端区鼠标事件复制 =====
const termAreaRegistry = new Map<string, HTMLElement>()

export function registerTermArea(id: string, el: HTMLElement) {
  termAreaRegistry.set(id, el)
}

export function unregisterTermArea(id: string) {
  termAreaRegistry.delete(id)
}

const REPLICATE_TYPES = ['mousedown', 'mousemove', 'mouseup', 'dblclick', 'wheel'] as const

/** 为终端区安装捕获监听：Ctrl 批量开启时把真实事件复制到其他分屏 */
export function attachTermAreaSync(id: string, el: HTMLElement): () => void {
  const handlers: Array<[string, EventListener]> = []
  for (const type of REPLICATE_TYPES) {
    const h = (e: Event) => replicateEvent(id, type, e as MouseEvent | WheelEvent)
    el.addEventListener(type, h, { capture: true })
    handlers.push([type, h])
  }
  return () => {
    handlers.forEach(([type, h]) => el.removeEventListener(type, h, { capture: true } as EventListenerOptions))
  }
}

/** 按相对坐标把事件复制到其它分屏的 xterm 元素 */
function replicateEvent(sourceId: string, type: string, e: MouseEvent | WheelEvent) {
  // 仅处理同步开启时的真实用户事件（合成事件不重复复制）
  if (!termSync.active || !e.isTrusted) return
  const srcEl = termAreaRegistry.get(sourceId)
  if (!srcEl) return
  const srcScreen = srcEl.querySelector('.xterm-screen')
  if (!srcScreen) return
  const srcRect = srcScreen.getBoundingClientRect()
  if (srcRect.width <= 0 || srcRect.height <= 0) return
  // 仅复制落在终端屏幕内的事件
  if (e.clientX < srcRect.left || e.clientX > srcRect.right ||
      e.clientY < srcRect.top || e.clientY > srcRect.bottom) return
  const relX = (e.clientX - srcRect.left) / srcRect.width
  const relY = (e.clientY - srcRect.top) / srcRect.height

  termAreaRegistry.forEach((el, id) => {
    if (id === sourceId) return
    const screen = el.querySelector('.xterm-screen')
    if (!screen) return
    const r = screen.getBoundingClientRect()
    const clientX = r.left + relX * r.width
    const clientY = r.top + relY * r.height
    if (type === 'wheel') {
      const we = e as WheelEvent
      screen.dispatchEvent(new WheelEvent('wheel', {
        clientX, clientY, deltaX: we.deltaX, deltaY: we.deltaY,
        deltaMode: we.deltaMode, bubbles: true, cancelable: true
      }))
      // 视口元素可能单独监听滚轮，双发兜底
      const viewport = el.querySelector('.xterm-viewport')
      viewport?.dispatchEvent(new WheelEvent('wheel', {
        clientX, clientY, deltaX: we.deltaX, deltaY: we.deltaY,
        deltaMode: we.deltaMode, bubbles: true, cancelable: true
      }))
    } else {
      const me = e as MouseEvent
      // 不带 ctrlKey 转发：避免 web-links 插件把 Ctrl+点击解释为打开链接；
      // 修饰键语义通过键盘同步链路（按住 Ctrl 时输入本身已广播）体现
      screen.dispatchEvent(new MouseEvent(type, {
        clientX, clientY, button: me.button, buttons: me.buttons,
        bubbles: true, cancelable: true
      }))
    }
  })
}
