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
 * 窗口注册与跨窗口 Tab 迁移
 * 作者：JC
 * 源窗口拖出 Tab 后广播 jc:tab-transfer-request（含屏幕物理坐标的落点），
 * 其他窗口判断落点是否在自己范围内，命中则回应 ack 并接管该 Tab；
 * 源窗口收到 ack 后移除 Tab（后端会话全局共享，不中断）。
 */
import { getCurrentWindow, getAllWindows } from '@tauri-apps/api/window'
import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event'
import { isTauriEnv } from '@/api'

/** 迁移 Tab 的元数据（后端会话由 sessionId 全局共享） */
export interface TransferredTab {
  id: string
  title: string
  host?: string
  username?: string
  hostname?: string
  sessionId?: string
  kind: 'local' | 'ssh'
  connected: boolean
}

interface TabTransferPayload {
  sourceLabel: string
  tab: TransferredTab
  /** 落点（屏幕物理像素） */
  screenX: number
  screenY: number
}

interface AckPayload {
  tabId: string
  targetLabel: string
}

let myLabel = 'main'
let unlistenReq: UnlistenFn | null = null
let unlistenAck: UnlistenFn | null = null
let pendingTabId = ''
let pendingResolve: ((accepted: boolean) => void) | null = null

/** 当前窗口 label（主窗口为 main，独立窗口为 term-xxx） */
export function windowLabel(): string {
  return myLabel
}

/** 初始化窗口注册：监听跨窗口 Tab 迁移请求/应答 */
export async function initWindowRegistry(onAcceptTab: (tab: TransferredTab) => void): Promise<void> {
  if (!isTauriEnv()) return
  const win = getCurrentWindow()
  myLabel = win.label

  unlistenReq?.()
  unlistenReq = await listen<TabTransferPayload>('jc:tab-transfer-request', async ev => {
    const p = ev.payload
    if (p.sourceLabel === myLabel) return
    // 落点必须命中本窗口的 Tab 栏区域才合并（统一语义：Tab 区域外 = 独立窗口/跨窗口迁移）
    const pos = await win.outerPosition()
    const scale = await win.scaleFactor()
    const clientX = (p.screenX - pos.x) / scale
    const clientY = (p.screenY - pos.y) / scale
    const bar = document.querySelector('.terminal-tabbar') as HTMLElement | null
    if (!bar) return
    const rect = bar.getBoundingClientRect()
    if (clientX >= rect.left && clientX <= rect.right &&
        clientY >= rect.top - 20 && clientY <= rect.bottom + 20) {
      await emit('jc:tab-transfer-ack', { tabId: p.tab.id, targetLabel: myLabel } satisfies AckPayload)
      onAcceptTab(p.tab)
    }
  })

  unlistenAck?.()
  unlistenAck = await listen<AckPayload>('jc:tab-transfer-ack', ev => {
    if (ev.payload.tabId === pendingTabId && ev.payload.targetLabel !== myLabel && pendingResolve) {
      const resolve = pendingResolve
      pendingResolve = null
      pendingTabId = ''
      resolve(true)
    }
  })
}

/**
 * 请求把 Tab 迁移到鼠标落点所在的其他窗口
 * @returns 是否被其他窗口接管（false 表示无接收方，调用方应新开窗口）
 */
export async function requestTabTransfer(tab: TransferredTab, clientX: number, clientY: number): Promise<boolean> {
  if (!isTauriEnv()) return false
  const win = getCurrentWindow()
  // 快速路径：不存在其他终端窗口（main/term-*）时直接返回，免去 450ms 的 ack 等待
  try {
    const others = (await getAllWindows()).filter(w =>
      w.label !== win.label && (w.label === 'main' || w.label.startsWith('term-')))
    if (others.length === 0) return false
  } catch { /* 权限/环境异常时走原广播流程 */ }
  const pos = await win.outerPosition()
  const scale = await win.scaleFactor()
  const payload: TabTransferPayload = {
    sourceLabel: myLabel,
    tab,
    screenX: Math.round(pos.x + clientX * scale),
    screenY: Math.round(pos.y + clientY * scale)
  }
  return new Promise<boolean>(resolve => {
    pendingTabId = tab.id
    pendingResolve = resolve
    emit('jc:tab-transfer-request', payload)
    setTimeout(() => {
      if (pendingResolve === resolve) {
        pendingResolve = null
        pendingTabId = ''
        resolve(false)
      }
    }, 450)
  })
}

/** 关闭当前窗口 */
export function closeCurrentWindow(): void {
  if (!isTauriEnv()) return
  getCurrentWindow().close().catch(() => {})
}
