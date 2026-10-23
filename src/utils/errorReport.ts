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
 * 客户端错误日志 - 静默上报工具
 * 作者：JC
 *
 * 设计目标：
 *  1. 不阻塞主流程：缓冲 + 定时 flush；上限 30 条立即 flush 避免丢失
 *  2. 自动归类：NETWORK / BUSINESS / TIMEOUT / PARSE / RUNTIME / INVOKE / UNKNOWN
 *  3. 自动收集软件信息：版本号 / 平台 / 用户等通过 `enrich()` 注入
 *  4. 上报失败仅 console.warn，绝不抛错给上层
 */
import { invoke } from '@/api'

export type ErrorType =
  | 'NETWORK'
  | 'BUSINESS'
  | 'TIMEOUT'
  | 'PARSE'
  | 'RUNTIME'
  | 'INVOKE'
  | 'UNKNOWN'

export interface ReportPayload {
  /** 错误类型 */
  errorType: ErrorType
  /** 错误码：业务码 / HTTP 状态码 / Rust AppError.code */
  errorCode?: string
  /** 用户侧可见的友好提示文案（已脱敏） */
  errorMessage?: string
  /** 原始错误堆栈 / 技术详情（仅排查用） */
  errorDetail?: string
  /** 出错位置标识（如 stores/qqGroup.fetchGroups） */
  source?: string
  /** 出错的接口路径或资源 URL */
  url?: string
  /** 额外上下文（JSON 字符串） */
  extra?: string
}

// ============ 缓冲 + 定时 flush ============
const BUFFER: ReportPayload[] = []
const FLUSH_INTERVAL_MS = 30_000
const MAX_BUFFER = 30
let timer: number | null = null
let flushing = false

/** 手动 push 一条错误到缓冲（fire-and-forget） */
export function reportError(payload: ReportPayload): void {
  try {
    BUFFER.push({ ...payload })
    if (BUFFER.length >= MAX_BUFFER) {
      void flush()
    } else {
      scheduleFlush()
    }
  } catch (e) {
    console.warn('[errorReport] enqueue 失败', e)
  }
}

function scheduleFlush(): void {
  if (timer !== null) return
  timer = window.setTimeout(() => {
    timer = null
    void flush()
  }, FLUSH_INTERVAL_MS)
}

async function flush(): Promise<void> {
  if (flushing || BUFFER.length === 0) return
  flushing = true
  const items = BUFFER.splice(0, BUFFER.length)
  try {
    // 通过 Tauri command 转发到主站 /api/openjc/client/error-logs/report-batch
    // Rust 端失败会被吞掉，不会冒到前端
    await invoke('report_client_errors', { reqs: items })
  } catch (e) {
    console.warn('[errorReport] 批量上报失败（已丢弃 {} 条）'.replace('{}', String(items.length)), e)
  } finally {
    flushing = false
  }
}

// 页面关闭前尽量 flush（Tauri 桌面环境实际上没等机会；浏览器环境有效）
if (typeof window !== 'undefined') {
  window.addEventListener('beforeunload', () => {
    void flush()
  })
}

// ============ 自动归类（供 api 包装和业务 store 用） ============
const TIMEOUT_RE = /timeout|超时/i

/**
 * 把任意异常归类为 ErrorType + errorCode
 *  - Rust AppError 序列化为 {code, message}
 *  - 网络错误：TypeError / fetch failed / ECONNRESET 等
 *  - 业务错误：有 code 字段且不是 INTERNAL_ERROR
 */
export function classifyError(e: unknown): { type: ErrorType; code?: string } {
  if (e == null) return { type: 'UNKNOWN' }
  if (typeof e === 'object') {
    const obj = e as { code?: unknown; message?: unknown; kind?: unknown }
    const code = typeof obj.code === 'string' ? obj.code : undefined
    const msg = typeof obj.message === 'string' ? obj.message : ''

    if (code === 'INTERNAL_ERROR') {
      // 来自 Rust 端兜底：进一步判别是不是网络层
      if (/error sending request|fetch failed|ECONNRESET|ETIMEDOUT|ENETUNREACH|connection|reset/i.test(msg)) {
        return { type: 'NETWORK', code }
      }
      if (TIMEOUT_RE.test(msg)) return { type: 'TIMEOUT', code }
      return { type: 'BUSINESS', code }
    }
    if (code) return { type: 'BUSINESS', code }
    if (TIMEOUT_RE.test(msg)) return { type: 'TIMEOUT' }
    if (/JSON|parse|Unexpected token/i.test(msg)) return { type: 'PARSE' }
    if (/network|fetch|socket|connection|reset/i.test(msg)) return { type: 'NETWORK' }
  }
  if (e instanceof Error) {
    if (TIMEOUT_RE.test(e.message)) return { type: 'TIMEOUT' }
    if (/network|fetch|socket|connection|reset/i.test(e.message)) return { type: 'NETWORK' }
  }
  return { type: 'UNKNOWN' }
}

/** 内部便捷：从异常直接构造上报 payload */
export function buildReport(e: unknown, source: string, opts?: { url?: string; extra?: Record<string, unknown> }): ReportPayload {
  const { type, code } = classifyError(e)
  let detail = ''
  try {
    if (e instanceof Error) detail = `${e.name}: ${e.message}\n${e.stack || ''}`
    else if (typeof e === 'object') detail = JSON.stringify(e, null, 2)
    else detail = String(e)
  } catch {
    detail = String(e)
  }
  if (detail.length > 32 * 1024) detail = detail.slice(0, 32 * 1024) + '\n... (truncated)'
  return {
    errorType: type,
    errorCode: code,
    errorMessage: '服务暂不可用，请稍后再试',
    errorDetail: `[${source}] ${detail}`,
    source,
    url: opts?.url,
    extra: opts?.extra ? JSON.stringify(opts.extra) : undefined,
  }
}

/** 供 api/index.ts 包装 invoke 用：catch 后调用 */
export function reportInvokeError(cmd: string, e: unknown, args?: Record<string, unknown>): void {
  reportError(buildReport(e, `invoke:${cmd}`, {
    extra: { cmd, argsKeys: args ? Object.keys(args) : [] },
  }))
}