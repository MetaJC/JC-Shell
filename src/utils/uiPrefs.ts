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
 * 外观全局偏好：基础字号 / 界面缩放（followDpi / uiScale）
 * 作者：JC
 * 每个窗口独立应用，跨窗口同步由 settings store 广播触发。
 */
import { watch } from 'vue'
import { useSettingsStore } from '@/stores/settings'

/** 初始化并监听外观设置变化，实时应用到 document */
export function initUiPrefs(): void {
  const prefs = useSettingsStore()

  const apply = () => {
    const s = prefs.settings
    // 基础字号：作用到根元素（rem/em 与 Element Plus 相对尺寸随之缩放）
    document.documentElement.style.fontSize = s.baseFontSize + 'px'
    // 界面缩放：跟随系统 DPI 时不额外缩放；否则按 uiScale 缩放
    // WebView2（Chromium 内核）支持 CSS zoom 属性
    const zoom = s.followDpi ? 100 : s.uiScale
    const style = document.documentElement.style as CSSStyleDeclaration & { zoom?: string }
    style.zoom = zoom === 100 ? '' : String(zoom / 100)
  }

  apply()
  watch(() => prefs.settings, apply, { deep: true })
}
