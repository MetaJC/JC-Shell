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
import { ref, computed, watch } from 'vue'
import { invoke, isTauriEnv } from '@/api'
import { notifyPrefsChanged, onPrefsChanged } from '@/utils/crossWindow'

export type ThemeMode = 'light' | 'dark' | 'eye-care' | 'system'
export type SettingsTab =
  | 'account'
  | 'data' | 'general' | 'terminal' | 'host' | 'shortcut'
  | 'appearance' | 'language'
  | 'security' | 'beta' | 'about'

const THEME_KEY = 'jc_shell_theme_mode'
const ACCENT_KEY = 'jc_shell_theme_accent'
/** 终端背景色（空字符串 = 使用默认纯黑色 #000000） */
const TERM_BG_KEY = 'jc_shell_terminal_bg'
/** 终端字体颜色（空字符串 = 使用默认白色 #ffffff） */
const TERM_FG_KEY = 'jc_shell_terminal_fg'
/** 终端背景色预置色板：精选互不接近的色样（深色在前、浅色在后），另支持自定义取色。
 *  数量已较初版翻倍，提供更多开箱即用的选择。 */
const TERMINAL_BG_PRESET = [
  // 深色系（覆盖常见主题：纯黑 / 暖灰 / 冷蓝灰 / 紫 / 青 / 海军蓝 / 酒红 / 橄榄绿）
  '#000000', '#0d1117', '#2b2b2d', '#1e1e2e', '#282a36',
  '#161b22', '#002b36', '#1a2639', '#300a24', '#3d4d35',
  '#1a1b26', '#121212', '#1c1c1e', '#21262d', '#191970',
  '#0b132b', '#0f1e2e', '#1f2937', '#111827', '#2d2a32',
  '#241f31', '#0e1726', '#222831', '#2c2c2c', '#1b1b2f',
  // 浅色系（纯白 / 浅灰 / 米黄 / 暖白）
  '#ffffff', '#f5f5f7', '#fdf6e3', '#fbf1c7', '#fff8e7',
  '#fafafa', '#f0f0f0', '#fffbe6', '#fef2f2', '#ecfdf5',
]
/** 终端字体颜色预置色板：精选互不接近的色样（浅字 / 深字 / 彩色），另支持自定义取色。
 *  数量已较初版翻倍，提供更多开箱即用的选择。 */
const TERMINAL_FG_PRESET = [
  // 浅色字（配深色背景）
  '#ffffff', '#f3f4f6', '#d4d4d4', '#a3be8c', '#88c0d0', '#ebcb8b',
  '#e5e7eb', '#cbd5e1', '#f1f5f9', '#b5e853', '#7fdbff', '#ffeaa7',
  // 深色字（配浅色背景）
  '#000000', '#161617', '#4b5563', '#6b7280',
  '#1f2937', '#374151', '#111827', '#52525b',
  // 彩色字
  '#51e98f', '#ff6b6b', '#6cb8ff', '#4ae3e3',
  '#f9ca24', '#fc427b', '#48dbfb', '#1dd1a1',
]
const ACCENT_PRESET = [
  '#60a5fa', '#22d3ee', '#34d399', '#fbbf24', '#f472b6',
  '#a78bfa', '#fb923c', '#ef4444', '#38bdf8', '#4ade80',
  '#facc15', '#f87171', '#818cf8', '#2dd4bf', '#e879f9',
]

/** 校验颜色值：只接受 #hex 与 rgb()/rgba()，其余一律视为「跟随主题」(返回空串) */
function normalizeColor(color: string): string {
  return /^#[0-9a-fA-F]{3,8}$/.test(color) || /^rgba?\(/.test(color) ? color : ''
}

/** 把主题模式立即应用到 DOM（Element Plus + Tailwind + 自定义 CSS 变量） */
function applyTheme(mode: ThemeMode, accent: string) {
  const d = document.documentElement
  const mqlDark = matchMedia('(prefers-color-scheme: dark)').matches
  let effective: string
  if (mode === 'eye-care') {
    effective = 'eye-care'
  } else if (mode === 'dark' || (mode === 'system' && mqlDark)) {
    effective = 'dark'
  } else {
    effective = 'light'
  }
  const isDark = effective === 'dark'
  d.classList.toggle('dark', isDark)
  d.setAttribute('data-theme', effective)
  d.style.setProperty('--jc-accent', accent)
}

export const useAppStore = defineStore('app', () => {
  const sidebarCollapsed = ref(false)
  const aiPanelVisible = ref(true)

  /** 精简模式：只显示终端窗口（隐藏标题栏/停靠面板/Tab 标签），运行时状态不持久化 */
  const zenMode = ref(false)
  function toggleZenMode() {
    zenMode.value = !zenMode.value
    syncZenWindow()
  }

  /**
   * 精简模式同步到原生窗口：macOS 需要在精简时去掉窗框以隐藏原生三圆点，
   * 退出时恢复窗框 + Overlay 标题栏让三圆点回来（见 Rust sync_zen_mode）。
   * 窗口由命令自动识别调用方，主窗口与独立终端窗口各自生效。
   */
  function syncZenWindow() {
    if (!isTauriEnv()) return
    invoke('sync_zen_mode', { zen: zenMode.value }).catch(() => {})
  }

  /** 窗口全屏状态（F11/全屏按钮驱动）：全屏时同样隐藏标题栏/停靠面板，但保留 Tab 标签 */
  const appFullscreen = ref(false)
  function setAppFullscreen(v: boolean) { appFullscreen.value = v }

  /** 安全密码锁屏状态：锁定时隐藏左下角头像（避免锁屏遮罩盖不住 fixed 头像） */
  const locked = ref(false)

  const savedMode = (localStorage.getItem(THEME_KEY) || '') as ThemeMode
  const savedAccent = localStorage.getItem(ACCENT_KEY) || ACCENT_PRESET[0]
  const themeMode = ref<ThemeMode>(
    savedMode === 'light' || savedMode === 'dark' || savedMode === 'eye-care' || savedMode === 'system'
      ? savedMode : 'system'
  )
  const accentColor = ref<string>(savedAccent)

  /** 自定义终端背景色；空 = 跟随主题默认色 */
  const terminalBg = ref<string>(localStorage.getItem(TERM_BG_KEY) || '')
  /** 自定义终端字体颜色；空 = 跟随主题默认色 */
  const terminalFg = ref<string>(localStorage.getItem(TERM_FG_KEY) || '')

  const settingsVisible = ref(false)
  const settingsInitialTab = ref<SettingsTab>('general')

  /** 启动后是否已经展示过 AI Agent 入口指向提示（内存态，每次打开软件都会重置） */
  const agentTipShown = ref(false)
  function markAgentTipShown() { agentTipShown.value = true }

  const effectiveTheme = computed<'light' | 'dark' | 'eye-care'>(() => {
    const mode = themeMode.value
    if (mode === 'eye-care') return 'eye-care'
    if (mode === 'dark') return 'dark'
    if (mode === 'light') return 'light'
    return matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
  })

  /** 终端实际背景色：用户自定义优先，否则用默认纯黑色 */
  const terminalBgColor = computed(() => terminalBg.value || '#000000')

  /** 终端实际字体色：用户自定义优先，否则用默认白色 */
  const terminalFgColor = computed(() => terminalFg.value || '#ffffff')

  /** 设置终端背景色（'' = 跟随主题），实时生效并跨窗口同步；支持 #hex 与 rgb()/rgba() */
  function setTerminalBg(color: string) {
    const c = normalizeColor(color)
    terminalBg.value = c
    try { localStorage.setItem(TERM_BG_KEY, c) } catch { /* ignore */ }
    notifyPrefsChanged([TERM_BG_KEY])
  }

  /** 设置终端字体颜色（'' = 跟随主题），实时生效并跨窗口同步；支持 #hex 与 rgb()/rgba() */
  function setTerminalFg(color: string) {
    const c = normalizeColor(color)
    terminalFg.value = c
    try { localStorage.setItem(TERM_FG_KEY, c) } catch { /* ignore */ }
    notifyPrefsChanged([TERM_FG_KEY])
  }

  function toggleSidebar() { sidebarCollapsed.value = !sidebarCollapsed.value }
  function toggleAiPanel() { aiPanelVisible.value = !aiPanelVisible.value }

  function setTheme(next: ThemeMode) {
    themeMode.value = next
    try { localStorage.setItem(THEME_KEY, next) } catch { /* ignore */ }
    applyTheme(themeMode.value, accentColor.value)
    notifyPrefsChanged([THEME_KEY])
  }

  function setAccent(color?: string) {
    const c = color && /^#[0-9a-fA-F]{6,8}$/.test(color) ? color : ACCENT_PRESET[Math.floor(Math.random() * ACCENT_PRESET.length)]
    accentColor.value = c
    try { localStorage.setItem(ACCENT_KEY, c) } catch { /* ignore */ }
    applyTheme(themeMode.value, accentColor.value)
    notifyPrefsChanged([ACCENT_KEY])
  }

  function randomizeAccent() { setAccent() }

  /**
   * 打开主窗口内的设置弹窗。
   * 这是 openSettings()（composables/useSettingsOpener）的降级路径：
   * 独立设置窗口不可用时（Rust 命令缺失 / 非 Tauri 环境）兜底，保证点了一定有反应。
   * 业务入口请统一调用 useSettingsOpener 的 openSettings()，不要直接调这里。
   */
  function openSettings(tab: SettingsTab = 'general') {
    settingsInitialTab.value = tab
    settingsVisible.value = true
  }
  function closeSettings() { settingsVisible.value = false }

  // 初次 + 系统主题变化时，同步 DOM
  applyTheme(themeMode.value, accentColor.value)
  if (typeof window !== 'undefined' && window.matchMedia) {
    const mql = window.matchMedia('(prefers-color-scheme: dark)')
    mql.addEventListener?.('change', () => {
      if (themeMode.value === 'system') applyTheme('system', accentColor.value)
    })
  }
  watch([themeMode, accentColor], ([m, c]) => applyTheme(m as ThemeMode, c as string))

  // 跨窗口同步：其他窗口（设置窗口/主窗口）改了主题或强调色时，重读存储并应用。
  // 仅赋值不重发事件，不会形成广播环。
  onPrefsChanged((keys) => {
    if (keys.includes(THEME_KEY)) {
      const m = (localStorage.getItem(THEME_KEY) || '') as ThemeMode
      if (m === 'light' || m === 'dark' || m === 'eye-care' || m === 'system') themeMode.value = m
    }
    if (keys.includes(ACCENT_KEY)) {
      const c = localStorage.getItem(ACCENT_KEY)
      if (c && /^#[0-9a-fA-F]{6,8}$/.test(c)) accentColor.value = c
    }
    if (keys.includes(TERM_BG_KEY)) {
      terminalBg.value = localStorage.getItem(TERM_BG_KEY) || ''
    }
    if (keys.includes(TERM_FG_KEY)) {
      terminalFg.value = localStorage.getItem(TERM_FG_KEY) || ''
    }
  })

  return {
    sidebarCollapsed, aiPanelVisible,
    toggleSidebar, toggleAiPanel,
    zenMode, toggleZenMode,
    appFullscreen, setAppFullscreen,
    locked,
    themeMode, accentColor, effectiveTheme, ACCENT_PRESET,
    setTheme, setAccent, randomizeAccent,
    terminalBg, terminalBgColor, setTerminalBg, TERMINAL_BG_PRESET,
    terminalFg, terminalFgColor, setTerminalFg, TERMINAL_FG_PRESET,
    settingsVisible, settingsInitialTab, openSettings, closeSettings,
    agentTipShown, markAgentTipShown,
  }
})