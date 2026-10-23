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
 * Tauri IPC 封装 —— 统一入口，屏蔽 Web/Tauri 双环境差异
 * 作者：JC
 *
 * 错误处理约定：
 *  - 内部自动静默上报到主站（不阻塞、不抛错）
 *  - 仍然抛出原错误，调用方可自行选择：
 *      A) catch (e) { handleApiError(e, '上下文') }  ← 推荐：友好提示 + 上报
 *      B) catch (e) { ElMessage.error(\`xxx失败: ${e}\`) }  ← 老代码，不强制改
 *      C) 不 catch（让上层统一错误边界处理）
 */
import { invoke as tauriInvoke } from '@tauri-apps/api/core'
import { reportInvokeError } from '@/utils/errorReport'

export interface InvokeOptions {
  timeoutMs?: number
}

const TAURI_AVAILABLE = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

/**
 * 调用 Tauri Command，非 Tauri 环境（纯浏览器开发）抛出明确错误
 * @param cmd Command 名称（与 Rust #[tauri::command] fn 名一致）
 * @param args 入参
 * @param opts 选项
 */
export async function invoke<T = unknown>(
  cmd: string,
  args?: Record<string, unknown>,
  opts: InvokeOptions = {}
): Promise<T> {
  if (!TAURI_AVAILABLE) {
    throw new Error(
      `[JC Shell] Command \`${cmd}\` 需在 Tauri 桌面环境执行。` +
      `当前是纯浏览器模式，部分功能仅能演示。`
    )
  }
  const { timeoutMs } = opts
  const p = tauriInvoke<T>(cmd, args ?? {})
  // 仅对真实 Tauri 调用做静默上报；非 Tauri 环境的提示错误不算"客户端真实错误"
  const silent = !String(cmd).startsWith('report_')
  if (timeoutMs && timeoutMs > 0) {
    try {
      return await Promise.race([
        p,
        new Promise<never>((_, reject) =>
          setTimeout(() => reject(new Error(`[JC Shell] Command \`${cmd}\` 超时 ${timeoutMs}ms`)), timeoutMs)
        ),
      ])
    } catch (e) {
      if (silent) reportInvokeError(cmd, e, args)
      throw e
    }
  }
  try {
    return await p
  } catch (e) {
    if (silent) reportInvokeError(cmd, e, args)
    throw e
  }
}

/** 当前是否在 Tauri 桌面环境运行 */
export function isTauriEnv(): boolean {
  return TAURI_AVAILABLE
}
