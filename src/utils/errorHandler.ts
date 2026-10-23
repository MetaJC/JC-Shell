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
 * API 错误统一处理 - 友好提示 + 静默上报
 * 作者：JC
 *
 * 铁律：任何来自 Rust / 后端 / 网络的技术错误，都不得直接展示给用户。
 * 用户只看到中文友好提示；技术细节一律进错误上报（主站 /api/openjc/client/error-logs）。
 *
 * 三种用法：
 *  1) try/catch 里要弹 toast：
 *       catch (e) { handleApiError(e, 'stores/xxx.fetch', { prefix: '加载失败' }) }
 *     → 弹 "加载失败：服务暂不可用，请稍后再试"
 *
 *  2) 只要友好文案、自己决定怎么展示（store 字段 / dialog 文案）：
 *       const msg = friendlyError(e)
 *
 *  3) 只要上报、不弹任何东西：
 *       reportOnly(e, 'context')
 */
import { ElMessage } from 'element-plus'
import { buildReport, classifyError, reportError } from './errorReport'

/** 各错误类型对应的用户可见文案（绝不含技术细节） */
const FRIENDLY_BY_TYPE: Record<string, string> = {
  NETWORK: '网络异常，请检查网络后稍后再试',
  TIMEOUT: '请求超时，请稍后再试',
  PARSE: '数据格式异常，请稍后再试',
  BUSINESS: '服务暂不可用，请稍后再试',
  RUNTIME: '运行异常，请稍后再试',
  INVOKE: '调用异常，请稍后再试',
  UNKNOWN: '服务暂不可用，请稍后再试',
}

/** 通用兜底文案 */
export const GENERIC_FRIENDLY = FRIENDLY_BY_TYPE.UNKNOWN

/**
 * 把任意异常转成一条安全的中文提示（不弹 toast、不做上报）
 * 用于：store 的 error / lastError 字段、需要自定义展示位置的场景。
 */
export function friendlyError(e: unknown): string {
  const { type } = classifyError(e)
  return FRIENDLY_BY_TYPE[type] || FRIENDLY_BY_TYPE.UNKNOWN
}

/**
 * 处理 API 错误：静默上报 + 弹一条中文友好提示（默认），返回展示给用户的文案。
 *
 * @param e        捕获到的异常
 * @param context  出错位置标识（进日志，不展示给用户）
 * @param opts.prefix  中文动作前缀，如 "压缩失败" → "压缩失败：网络异常，请稍后再试"
 * @param opts.showToast  false 时不弹 toast（由调用方自行展示返回值）
 */
export function handleApiError(
  e: unknown,
  context: string,
  opts?: {
    url?: string
    showToast?: boolean
    extra?: Record<string, unknown>
    /** 中文动作前缀，例如 "压缩失败" / "重命名失败" */
    prefix?: string
  }
): string {
  const report = buildReport(e, context, opts)
  const base = friendlyError(e)
  const shown = opts?.prefix ? `${opts.prefix}：${base}` : base
  report.errorMessage = shown
  reportError(report)

  if (opts?.showToast !== false) {
    ElMessage.error(shown)
  }
  return shown
}

/** 只上报、不做任何 UI 展示（调用方已自行处理文案时使用） */
export function reportOnly(e: unknown, context: string, opts?: { url?: string; extra?: Record<string, unknown> }): void {
  reportError(buildReport(e, context, opts))
}

/**
 * 把「可能是技术错误、也可能是业务文案」的字符串转成安全文案。
 *
 * 场景：某些 store / Rust 层会把错误以 string 形式返回（如 editorStore.save 返 err 字符串、
 * transfer 的 f.error），这些字符串直接来自后端，不能裸展示。
 * 这里做一层收敛：已知的业务白名单文案原样保留，其余一律替换为友好文案。
 */
const SAFE_PASSTHROUGH = [
  '传输已取消',
  '下载已取消',
  '操作已取消',
  '用户取消',
  'cancel',
]
export function friendlyText(raw: string | null | undefined): string {
  if (!raw) return GENERIC_FRIENDLY
  const s = String(raw)
  // 明确的"用户主动取消"类文案可以原样展示（属于预期行为，不是故障）
  if (SAFE_PASSTHROUGH.some(k => s.includes(k))) return s
  // 其余一律不信任：任何疑似技术错误（含错误码、URL、英文堆栈特征）都替换为通用文案
  const looksTechnical =
    /error sending request|http|https?:\/\/|ECONN|ETIMEDOUT|timeout|failed|exception|stack|\.rs:|code[=:]|IO error|permission denied|No such file/i.test(s)
  return looksTechnical ? GENERIC_FRIENDLY : s
}