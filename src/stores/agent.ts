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

import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@/api'
import { friendlyError } from '@/utils/errorHandler'

/** Agent 模式下终端光标处的浅灰幽灵提示文案（输入时消失，清空后复现） */
export const AGENT_GHOST_HINT = '输入自然语言，交给 Agent处理，按 Ctrl+Shift+I 切换 Shell 模式'

/** 终端内 Agent 确认/控制动作 */
export type AgentAction = 'execute' | 'modify' | 'reject' | 'abort'

/** AI 建议的单条命令 */
export interface AgentCommand {
  cmd: string
  purpose?: string
  consequence?: string
}

/** 服务端 Agent 响应 + 本地引擎合并结果 */
export interface AgentResult {
  requestId?: string
  explanation?: string
  riskLevel?: string
  commands?: AgentCommand[]
  needMoreInfo?: boolean
  question?: string
  balanceRemaining?: number
  /** 本地引擎与服务端取更严后的最终级别: safe/caution/danger/blocked */
  finalRiskLevel: string
  guardReason?: string
}

/** 待确认的 Agent 请求状态 */
export interface AgentPending {
  tabId: string
  prompt: string
  loading: boolean
  executing: boolean
  result: AgentResult | null
  /** 展示给用户的错误文案（已脱敏，绝不含技术细节） */
  error: string
  /** 原始错误（仅内部判断用，如鉴权错误识别，不展示） */
  rawError?: string
  /** 本次请求的客户端 ID（用于 Ctrl+C 时真正中止后端在途请求） */
  clientReqId?: string
}

/**
 * AI Agent 模式 store
 * 作者：JC
 * - Tab 级开关（enabledTabs）
 * - 终端输入缓冲（Agent 模式下键盘输入不进 shell，逐字回显由 TerminalView 负责）
 * - 一次请求的生命周期：submit → pending(loading) → result → 确认执行/取消
 * - 每 Tab 的 shell prompt 字符串（Agent 模式下显示在输入行前）
 */
export const useAgentStore = defineStore('agent', () => {
  /** Agent 模式开启的 Tab 集合 */
  const enabledTabs = ref<string[]>([])
  /** 当前待确认的请求（同一时间只处理一个） */
  const pending = ref<AgentPending | null>(null)
  /** 每 Tab 的仿真 prompt（如 "root@host:~# "），Agent 提交后的新行前缀 */
  const tabPrompts = ref<Record<string, string>>({})
  /** 每 Tab 的 prompt 是否已写入当前行（遗留状态，新流程由 TerminalView 直接控制） */
  const promptWritten = ref<Record<string, boolean>>({})
  /** 最新积分余额（由 AiAssistantCard/Agent 响应刷新，供卡片摘要展示） */
  const balance = ref<number | null>(null)

  function setBalance(v: number | null): void {
    balance.value = v
  }

  /** 输入缓冲（非响应式，逐键高频更新） */
  const buffers = new Map<string, string>()
  /** 每 Tab 最近一次交给 AI 的自然语言请求（下载卡死时自动换源重试用） */
  const lastPrompts = new Map<string, string>()
  /** 每 Tab 最近一次发往 shell 执行的命令（下载卡死时用于生成换源重试） */
  const lastCommands = new Map<string, string>()
  /**
   * 每 Tab 的任务代次（epoch）：Ctrl+C 终止时自增，
   * AI 请求 / 逐条发送命令 / 卡死自动重试等异步链据此判断自己是否已被取消。
   */
  const runEpochs = new Map<string, number>()
  /** 每 Tab 的输入历史（Agent 模式内 ↑/↓ 回忆用） */
  const histories = new Map<string, string[]>()
  /** 每 Tab 当前历史导航索引（null 表示未在导航中） */
  const historyIndexes = new Map<string, number | null>()
  /** 「Shell 直通」中的 Tab：键盘输入直达真实 shell（readline 补全/历史/编辑），
   *  由两种场景进入：AI 方案选「修改」编辑命令行；Agent 输入行被识别为 shell 命令 */
  const passthroughTabs = ref<string[]>([])
  /** 直通保持：交互式/常驻前台命令（vim、top、tail -f、docker exec -it…）回车后不退出直通，
   *  否则 Ctrl+C 会被 Agent 吞掉，用户无法中断前台程序（退出方式：Ctrl+C / Ctrl+Shift+I） */
  const stickyPassthroughTabs = ref<string[]>([])

  function isPassthrough(tabId: string): boolean {
    return passthroughTabs.value.includes(tabId)
  }

  function isPassthroughSticky(tabId: string): boolean {
    return stickyPassthroughTabs.value.includes(tabId)
  }

  function setPassthrough(tabId: string, on: boolean, sticky = false): void {
    const i = passthroughTabs.value.indexOf(tabId)
    if (on && i < 0) passthroughTabs.value.push(tabId)
    if (!on && i >= 0) passthroughTabs.value.splice(i, 1)
    const s = stickyPassthroughTabs.value.indexOf(tabId)
    if (on && sticky && s < 0) stickyPassthroughTabs.value.push(tabId)
    if ((!on || !sticky) && s >= 0) stickyPassthroughTabs.value.splice(s, 1)
  }

  function isEnabled(tabId: string): boolean {
    return enabledTabs.value.includes(tabId)
  }

  function toggle(tabId: string): boolean {
    const idx = enabledTabs.value.indexOf(tabId)
    if (idx >= 0) {
      enabledTabs.value.splice(idx, 1)
      // 关闭时【不】删缓冲: Agent 模式敲了但没提交的字只存在于终端的本地回显里,
      // TerminalView 需要先读到它才能把那几个字符从屏幕上擦掉(否则退出 Agent 后
      // 它们仍留在提示符后面, 再次切进 Agent 就成了"看得见却删不掉"的幽灵文本)。
      // 由 TerminalView 擦除后 clearBuffer; 下次重新开启时下面这行也会把它重置为空。
      // 注意：保留 tabPrompts，退出 Agent 模式时 TerminalView 需要它把提示符重绘回去。
      delete promptWritten.value[tabId]
      setPassthrough(tabId, false)
      historyIndexes.delete(tabId)
      if (pending.value?.tabId === tabId) pending.value = null
      return false
    }
    enabledTabs.value.push(tabId)
    buffers.set(tabId, '')
    // 开启 Agent 即恢复拦截：避免上一次交互式命令留下的直通状态吞掉这行的输入
    setPassthrough(tabId, false)
    return true
  }

  function getBuffer(tabId: string): string {
    return buffers.get(tabId) ?? ''
  }

  function appendBuffer(tabId: string, text: string): void {
    buffers.set(tabId, getBuffer(tabId) + text)
  }

  function setBuffer(tabId: string, text: string): void {
    buffers.set(tabId, text)
  }

  /** 把提交过的行加入该 Tab 历史（连续重复不加入） */
  function pushHistory(tabId: string, line: string): void {
    const trimmed = line.trim()
    if (!trimmed) return
    const list = histories.get(tabId) ?? []
    if (list.length > 0 && list[list.length - 1] === trimmed) return
    list.push(trimmed)
    if (list.length > 200) list.shift()
    histories.set(tabId, list)
    historyIndexes.delete(tabId)
  }

  /** 上一条历史；首次调用返回最新一条 */
  function prevHistory(tabId: string): string | null {
    const list = histories.get(tabId) ?? []
    if (list.length === 0) return null
    let idx = historyIndexes.get(tabId)
    if (idx == null) idx = list.length
    idx = Math.max(0, idx - 1)
    historyIndexes.set(tabId, idx)
    return list[idx] ?? null
  }

  /** 下一条历史；到达末尾后返回 null */
  function nextHistory(tabId: string): string | null {
    const list = histories.get(tabId) ?? []
    let idx = historyIndexes.get(tabId)
    if (idx == null) return null
    idx = idx + 1
    if (idx >= list.length) {
      historyIndexes.delete(tabId)
      return null
    }
    historyIndexes.set(tabId, idx)
    return list[idx] ?? null
  }

  function resetHistoryIndex(tabId: string): void {
    historyIndexes.delete(tabId)
  }

  /** 记录该 Tab 最近一次自然语言请求（供下载卡死时自动换源重试） */
  function setLastPrompt(tabId: string, prompt: string): void {
    lastPrompts.set(tabId, prompt)
  }

  function getLastPrompt(tabId: string): string {
    return lastPrompts.get(tabId) ?? ''
  }

  /** 记录该 Tab 最近一次发往 shell 的命令 */
  function setLastCommand(tabId: string, cmd: string): void {
    lastCommands.set(tabId, cmd)
  }

  function getLastCommand(tabId: string): string {
    return lastCommands.get(tabId) ?? ''
  }

  /** 取当前任务代次 */
  function currentEpoch(tabId: string): number {
    return runEpochs.get(tabId) ?? 0
  }

  /** 终止该 Tab 上所有进行中的异步任务链（Ctrl+C 调用）；返回新代次 */
  function bumpEpoch(tabId: string): number {
    const next = currentEpoch(tabId) + 1
    runEpochs.set(tabId, next)
    return next
  }

  function backspaceBuffer(tabId: string): boolean {
    const buf = getBuffer(tabId)
    if (!buf) return false
    // 按码点退格（兼容中文等多字节字符）
    buffers.set(tabId, Array.from(buf).slice(0, -1).join(''))
    return true
  }

  function setPrompt(tabId: string, prompt: string): void {
    tabPrompts.value[tabId] = prompt
    promptWritten.value[tabId] = false // 新 prompt 需要重新写入
  }

  function getPrompt(tabId: string): string {
    return tabPrompts.value[tabId] || ''
  }

  function isPromptWritten(tabId: string): boolean {
    return !!promptWritten.value[tabId]
  }

  function markPromptWritten(tabId: string): void {
    promptWritten.value[tabId] = true
  }

  function clearBuffer(tabId: string): number {
    const len = Array.from(getBuffer(tabId)).length
    buffers.set(tabId, '')
    promptWritten.value[tabId] = false // 清空后下次输入需重新写 prompt
    return len
  }

  /** 取走整行输入（Enter 提交时） */
  function takeBuffer(tabId: string): string {
    const buf = getBuffer(tabId).trim()
    buffers.set(tabId, '')
    promptWritten.value[tabId] = false // 提交后下次输入需重新写 prompt
    return buf
  }

  /** 发起 Agent 请求（上下文由调用方采集） */
  async function submit(tabId: string, prompt: string, ctx: {
    shellType?: string
    os?: string
    cwd?: string
    target?: string
    recentOutput?: string
    conversationId?: string
  }): Promise<void> {
    lastPrompts.set(tabId, prompt)
    // 客户端请求 ID：Ctrl+C 时据此真正中止后端在途请求
    const clientReqId = ctx.conversationId
      || `agent-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
    pending.value = { tabId, prompt, loading: true, executing: false, result: null, error: '', clientReqId }
    try {
      const result = await invoke<AgentResult>('ai_agent_query', {
        dto: { ...ctx, prompt, conversationId: clientReqId }
      })
      // 请求期间用户切换/关闭了 Agent 模式则丢弃结果
      if (pending.value?.tabId !== tabId) return
      pending.value = { tabId, prompt, loading: false, executing: false, result, error: '', clientReqId }
    } catch (e: any) {
      if (pending.value?.tabId !== tabId) return
      pending.value = {
        tabId, prompt, loading: false, executing: false, result: null,
        rawError: e?.message || String(e),
        error: friendlyError(e), clientReqId
      }
    }
  }

  function setExecuting(v: boolean): void {
    if (pending.value) pending.value.executing = v
  }

  function clearPending(): void {
    pending.value = null
  }

  return {
    enabledTabs, pending, balance, setBalance,
    tabPrompts, promptWritten,
    isEnabled, toggle,
    isPassthrough, isPassthroughSticky, setPassthrough,
    getBuffer, appendBuffer, setBuffer, backspaceBuffer, clearBuffer, takeBuffer,
    pushHistory, prevHistory, nextHistory, resetHistoryIndex,
    setLastPrompt, getLastPrompt, setLastCommand, getLastCommand,
    currentEpoch, bumpEpoch,
    setPrompt, getPrompt, isPromptWritten, markPromptWritten,
    submit, setExecuting, clearPending
  }
})
