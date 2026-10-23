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
 * 文件传输状态 Store
 * 作者：JC
 * 跟踪上传/下载队列：后端 transfer-progress 事件驱动进度；
 * 每个 fs 面板按 ownerSessionId（目标会话）过滤显示属于自己的传输。
 */
import { defineStore } from 'pinia'
import { ref, computed, type ComputedRef } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { isTauriEnv } from '@/api'

export type TransferDirection = 'upload' | 'download'
export type TransferStatus = 'active' | 'done' | 'error'

export interface TransferItem {
  id: string
  name: string
  direction: TransferDirection
  /** 目标会话（进度显示在该会话对应的 fs 面板） */
  ownerSessionId: string
  transferred: number
  total: number
  done: boolean
  error: string
  /** 瞬时速率（EMA 平滑），字节/秒 */
  speed: number
  /** 目标完整路径（用于"打开所在目录"） */
  dstPath: string
  startedAt: number
}

interface ProgressPayload {
  transfer_id: string
  transferred: number
  total: number
  done: boolean
  error: string | null
}

let listening = false

export const useTransferStore = defineStore('transfer', () => {
  const items = ref<TransferItem[]>([])
  const lastSample = new Map<string, { t: number; bytes: number }>()

  /**
   * 全局传输队列弹窗显示态：
   * - 顶部菜单「工具-传输队列」直接打开/关闭全局队列；
   * - 文件系统组件 watch 此标志，匹配当前激活会话时显示本地队列弹窗。
   */
  const globalVisible = ref(false)
  /** 全局队列请求序号：用于"一次性打开"语义，避免双向 watch 形成回环 */
  const globalRequestSeq = ref(0)
  /** 全局请求的目标会话：菜单触发时由调用方写入，FileSystemBrowser 据此判断是否响应 */
  const targetSessionId = ref('')
  function openGlobal(sessionId: string = ''): void {
    targetSessionId.value = sessionId
    globalVisible.value = true
    globalRequestSeq.value++
  }
  function closeGlobal(): void {
    globalVisible.value = false
  }

  function ensureListener(): void {
    if (listening || !isTauriEnv()) return
    listening = true
    listen<ProgressPayload>('transfer-progress', (ev) => {
      const p = ev.payload
      const it = items.value.find(i => i.id === p.transfer_id)
      if (!it) return
      const now = Date.now()
      const last = lastSample.get(it.id)
      if (last && now > last.t) {
        const inst = (p.transferred - last.bytes) / ((now - last.t) / 1000)
        it.speed = it.speed * 0.6 + Math.max(0, inst) * 0.4
      }
      lastSample.set(it.id, { t: now, bytes: p.transferred })
      it.transferred = p.transferred
      if (p.total > 0) it.total = p.total
      if (p.done) {
        it.done = true
        it.speed = 0
        if (p.error) it.error = p.error
      }
    })
  }

  function begin(t: { id: string; name: string; direction: TransferDirection; ownerSessionId: string; dstPath: string }): void {
    ensureListener()
    items.value.push({
      ...t,
      transferred: 0,
      total: 0,
      done: false,
      error: '',
      speed: 0,
      startedAt: Date.now()
    })
    // 队列上限，避免长时间运行无限增长
    if (items.value.length > 100) items.value.splice(0, items.value.length - 100)
  }

  /** 传输启动即失败（后端未来得及发进度事件）时手动标记 */
  function markError(id: string, err: string): void {
    const it = items.value.find(i => i.id === id)
    if (it) { it.done = true; it.error = err; it.speed = 0 }
  }

  function clearFinished(sessionId: string): void {
    items.value = items.value.filter(i => !(i.ownerSessionId === sessionId && i.done))
  }

  function statusOf(it: TransferItem): TransferStatus {
    if (it.error) return 'error'
    return it.done ? 'done' : 'active'
  }

  /** 某会话 fs 面板的传输列表（新→旧） */
  function bySession(sessionId: string): ComputedRef<TransferItem[]> {
    return computed(() => items.value.filter(i => i.ownerSessionId === sessionId).slice().reverse())
  }

  /** 某会话进行中的传输聚合（状态栏总进度） */
  function activeSummary(sessionId: string): ComputedRef<{ count: number; transferred: number; total: number; percent: number }> {
    return computed(() => {
      const act = items.value.filter(i => i.ownerSessionId === sessionId && !i.done)
      const transferred = act.reduce((s, i) => s + i.transferred, 0)
      const total = act.reduce((s, i) => s + i.total, 0)
      // total 未知（尚未收到首个进度事件）时 percent=-1 → 不定态
      const percent = total > 0 ? Math.min(100, Math.round((transferred / total) * 100)) : (act.length ? -1 : 0)
      return { count: act.length, transferred, total, percent }
    })
  }

  return {
    items, begin, markError, clearFinished, statusOf, bySession, activeSummary,
    globalVisible, globalRequestSeq, targetSessionId, openGlobal, closeGlobal
  }
})
