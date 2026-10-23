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
 * 剪贴板工具：统一复制入口，支持「N 秒后自动清除剪贴板」安全设置
 * 作者：JC
 * 优先使用 Tauri 原生剪贴板插件（绕开 WebView2 读取剪贴板的权限弹窗）；
 * 插件不可用或调用失败时回退到 Web Clipboard API，保证复制/粘贴在两种环境下都可用。
 */
import { readText as tauriReadText, writeText as tauriWriteText } from '@tauri-apps/plugin-clipboard-manager'
import { useSettingsStore } from '@/stores/settings'

let clearTimer: ReturnType<typeof setTimeout> | null = null
/** 最近一次由本应用写入剪贴板的内容（到期时仅在内容未被外部覆盖时才清除） */
let lastCopied = ''

/** 写剪贴板：优先 Tauri 原生插件，失败回退 Web Clipboard API */
async function writeClipboard(text: string): Promise<void> {
  try {
    await tauriWriteText(text)
    return
  } catch { /* Tauri 插件不可用（纯浏览器）或调用失败 → 回退 */ }
  if (typeof navigator !== 'undefined' && navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(text)
    return
  }
  throw new Error('剪贴板不可用')
}

/** 读剪贴板：优先 Tauri 原生插件，失败回退 Web Clipboard API；均不可用时返回空串 */
export async function readClipboardText(): Promise<string> {
  try {
    return (await tauriReadText()) ?? ''
  } catch { /* Tauri 插件不可用或调用失败 → 回退 */ }
  try {
    if (typeof navigator !== 'undefined' && navigator.clipboard?.readText) {
      return (await navigator.clipboard.readText()) ?? ''
    }
  } catch { /* 用户拒绝授权等，按空内容处理 */ }
  return ''
}

/** 复制文本；开启剪贴板自动清除时，到期清除（仅当剪贴板仍为本次复制内容） */
export async function copyText(text: string): Promise<void> {
  await writeClipboard(text)
  lastCopied = text
  const s = useSettingsStore().settings
  if (clearTimer) {
    clearTimeout(clearTimer)
    clearTimer = null
  }
  if (s.clipboardClearEnabled && s.clipboardClearSeconds > 0) {
    clearTimer = setTimeout(async () => {
      clearTimer = null
      try {
        const cur = await readClipboardText()
        if (cur === lastCopied) await writeClipboard('')
      } catch { /* 读取失败（权限等）不清除 */ }
    }, s.clipboardClearSeconds * 1000)
  }
}
