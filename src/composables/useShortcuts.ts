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
 * 全局快捷键调度：把设置中可编辑的组合键映射到真实动作
 * 作者：JC
 * 组合键持久化在 settings store（跨窗口同步），这里仅做监听与分发。
 */
import { useSettingsStore } from '@/stores/settings'
import { useAppStore } from '@/stores/app'
import { openSettings } from '@/composables/useSettingsOpener'

/** Tab/分屏相关动作由宿主窗口（MainWindow）提供 */
export interface ShortcutActions {
  newTab(): void
  closeTab(): void
  nextTab(): void
  prevTab(): void
  splitPane(): void
  splitHorizontal(): void
  splitVertical(): void
  closePane(): void
  /** 手动锁定程序（需已设置安全密码） */
  lock(): void
}

/** 键盘事件 → 规范化组合键字符串（与设置弹窗录入格式一致：Ctrl+Shift+K） */
export function comboOfEvent(e: KeyboardEvent): string {
  const parts: string[] = []
  if (e.ctrlKey) parts.push('Ctrl')
  if (e.shiftKey) parts.push('Shift')
  if (e.altKey) parts.push('Alt')
  if (e.metaKey) parts.push('Meta')
  let k = e.key
  if (k === ' ') k = 'Space'
  if (k.length === 1) k = k.toUpperCase()
  parts.push(k)
  return parts.join('+')
}

/** 挂载全局快捷键监听，返回注销函数 */
export function attachShortcuts(actions: ShortcutActions): () => void {
  const prefs = useSettingsStore()
  const app = useAppStore()

  function handler(e: KeyboardEvent) {
    // 输入控件内的按键不劫持（快捷键录制框、表单等）
    const target = e.target as HTMLElement | null
    if (target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable)) return
    const combo = comboOfEvent(e)
    const sc = prefs.settings.shortcuts

    const map: Array<[string | undefined, () => void]> = [
      [sc.newTab, actions.newTab],
      [sc.closeTab, actions.closeTab],
      [sc.nextTab, actions.nextTab],
      [sc.prevTab, actions.prevTab],
      [sc.toggleAi, () => app.toggleAiPanel()],
      [sc.toggleSidebar, () => app.toggleSidebar()],
      [sc.settings, () => void openSettings()],
      [sc.lock, actions.lock],
      [sc.splitPane, actions.splitPane],
      [sc.splitHorizontal, actions.splitHorizontal],
      [sc.splitVertical, actions.splitVertical],
      [sc.closePane, actions.closePane],
    ]
    for (const [key, fn] of map) {
      if (key && combo === key) {
        e.preventDefault()
        fn()
        return
      }
    }
  }

  window.addEventListener('keydown', handler)
  return () => window.removeEventListener('keydown', handler)
}
