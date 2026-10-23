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
 * 终端 Store
 * 作者：JC
 * 每个窗口拥有独立实例（Pinia 随 WebView 隔离）；后端会话全局共享，
 * Tab 跨窗口迁移仅传递元数据 + sessionId。
 */
import { defineStore } from 'pinia'
import { ref } from 'vue'

/** 分屏模式：none=单 Tab 视图；h=左右；v=上下；grid=网格 */
export type SplitMode = 'none' | 'h' | 'v' | 'grid'

export interface TerminalTab {
  id: string
  title: string
  host?: string
  /** 打开该 Tab 的会话条目 ID。重连/复制必须按它对号入座：
   *  同一个地址可能有多个会话（密码/密钥不同），按地址找会串用别人的凭据 */
  hostId?: string
  /** 上次连接是否因「认证失败」结束。为真时重连必须重新确认密码，
   *  避免静默复用会话里保存的旧密码，出现「密码不对却连上了」 */
  authFailed?: boolean
  /** 登录账户 */
  username?: string
  /** 服务器名称（主机别名） */
  hostname?: string
  /** 后端 SSH 会话 ID（连接成功后回填） */
  sessionId?: string
  connected: boolean
  createdAt: number
  /** 会话类型：本地 PTY / SSH（决定输入链路与 convertEol） */
  kind: 'local' | 'ssh'
}

export const useTerminalStore = defineStore('terminal', () => {
  const tabs = ref<TerminalTab[]>([])
  const activeTabId = ref<string>('')
  /** 分屏模式（铺排当前窗口全部 Tab） */
  const splitMode = ref<SplitMode>('none')

  /** 正在「连接中」的 Tab id 列表（连接成功/失败后移除）。驱动移动端连接过渡遮罩 */
  const connectingIds = ref<string[]>([])
  function addConnecting(id: string) {
    if (!connectingIds.value.includes(id)) connectingIds.value = [...connectingIds.value, id]
  }
  function removeConnecting(id: string) {
    if (connectingIds.value.includes(id)) connectingIds.value = connectingIds.value.filter(x => x !== id)
  }

  function createTab(title: string = '本地终端', host?: string, kind: 'local' | 'ssh' = 'local'): string {
    const id = 'tab-' + Date.now() + '-' + Math.random().toString(36).slice(2, 6)
    tabs.value.push({
      id,
      title,
      host,
      connected: false,
      createdAt: Date.now(),
      kind
    })
    activeTabId.value = id
    return id
  }

  /** 接管跨窗口迁移来的 Tab（保留原 id，sessionTabMap 与后端会话对应） */
  function adoptTab(tab: Omit<TerminalTab, 'createdAt'>): void {
    if (tabs.value.some(t => t.id === tab.id)) return
    tabs.value.push({ ...tab, createdAt: Date.now() })
    activeTabId.value = tab.id
  }

  function closeTab(id: string) {
    const idx = tabs.value.findIndex(t => t.id === id)
    if (idx === -1) return
    tabs.value.splice(idx, 1)
    if (activeTabId.value === id) {
      activeTabId.value = tabs.value[idx]?.id || tabs.value[idx - 1]?.id || ''
    }
  }

  /** 不关闭后端会话，仅从本窗口移除 Tab（拖出到独立窗口时用） */
  function detachTab(id: string): TerminalTab | null {
    const idx = tabs.value.findIndex(t => t.id === id)
    if (idx === -1) return null
    const [tab] = tabs.value.splice(idx, 1)
    if (activeTabId.value === id) {
      activeTabId.value = tabs.value[idx]?.id || tabs.value[idx - 1]?.id || ''
    }
    return tab
  }

  function setActive(id: string) {
    activeTabId.value = id
  }

  function updateTab(id: string, patch: Partial<TerminalTab>) {
    const tab = tabs.value.find(t => t.id === id)
    if (tab) Object.assign(tab, patch)
  }

  /** 拖拽排序：把 Tab 移到目标下标 */
  function reorderTab(id: string, toIndex: number) {
    const from = tabs.value.findIndex(t => t.id === id)
    if (from === -1) return
    const [tab] = tabs.value.splice(from, 1)
    const to = Math.max(0, Math.min(tabs.value.length, toIndex))
    tabs.value.splice(to, 0, tab)
  }

  function setSplitMode(mode: SplitMode) {
    splitMode.value = mode
  }

  /** 按 sessionId 查找 tab */
  function findBySessionId(sessionId: string): TerminalTab | undefined {
    return tabs.value.find(t => t.sessionId === sessionId)
  }

  return {
    tabs, activeTabId, splitMode, connectingIds, addConnecting, removeConnecting,
    createTab, adoptTab, closeTab, detachTab, setActive, updateTab,
    reorderTab, setSplitMode, findBySessionId
  }
})
