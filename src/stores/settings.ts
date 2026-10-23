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
 * 全局设置 Store
 * 作者：JC
 * 所有设置项统一在此持久化（localStorage），设置弹窗控件直接双向绑定，
 * 修改即实时生效并自动保存；通过 Tauri 事件跨窗口同步，
 * 保证"设置生效范围是整个程序"。
 */
import { defineStore } from 'pinia'
import { ref, watch } from 'vue'
import { notifyPrefsChanged, onPrefsChanged } from '@/utils/crossWindow'
import { emitConfigChanged } from '@/utils/syncSignals'

const STORAGE_KEY = 'jc_shell_settings'

/** 快捷键默认值（与设置弹窗分组一致） */
export const SHORTCUT_DEFAULT: Record<string, string> = {
  newTab: 'Ctrl+T', closeTab: 'Ctrl+W',
  nextTab: 'Ctrl+Shift+]', prevTab: 'Ctrl+Shift+[',
  toggleAi: 'Ctrl+K', toggleAgent: 'Ctrl+Shift+I', toggleSidebar: 'Ctrl+B', settings: 'Ctrl+,',
  lock: 'Ctrl+Shift+L',
  splitPane: 'Alt+Shift+D', splitHorizontal: 'Alt+Shift+-',
  splitVertical: 'Alt+Shift+=', closePane: 'Ctrl+Shift+W',
  editorSave: 'Ctrl+S', editorFind: 'Ctrl+F', editorReplace: 'Ctrl+H', editorFormat: 'Ctrl+Shift+F',
}

export interface AppSettings {
  // ===== 通用 =====
  autoLaunch: boolean
  minimizeToTray: boolean
  confirmOnExit: boolean
  // ===== 交互 =====
  /** 多屏协同（批量模式）的开启方式；仅在分屏模式下生效 */
  multiSyncTrigger: 'doubleCtrl' | 'longCtrl' | 'alwaysOn' | 'alwaysOff'
  /** 分屏内文件系统工具条的显隐触发方式 */
  fsToolbarToggle: 'singleAlt' | 'doubleAlt'
  updateChannel: 'stable' | 'beta'
  silentUpdate: boolean
  proxyMode: 'direct' | 'system' | 'manual'
  manualProxyHost: string
  manualProxyPort: number
  // ===== 终端 =====
  defaultShell: 'auto' | 'powershell' | 'cmd' | 'bash' | 'zsh'
  terminalFontSize: number
  terminalLineHeight: number
  cursorStyle: 'block' | 'bar' | 'underline'
  cursorBlink: boolean
  scrollbackLines: number
  sshKeepAlive: number
  sshConnectTimeout: number
  autoReconnect: boolean
  maxReconnectTimes: number
  selectToCopy: boolean
  rightClickPaste: boolean
  allowDragUpload: boolean
  bellAlert: boolean
  /** 把行首的命令提示符（如 root@host:~#）加粗高亮，与普通输出区分开 */
  highlightPrompt: boolean
  /** 鼠标中键（滚轮）点击终端标签页以关闭该标签页 */
  middleCloseTab: boolean
  /** 侧边 Dock 栏是否固定显示（false = 自动隐藏，仅在边缘保留极窄提示条） */
  sideDockPinned: boolean
  /** 端口转发配置界面的交互方式：page=页面式一次填完；wizard=交互式分步引导 */
  forwardConfigStyle: 'page' | 'wizard'
  // ===== 主机与 SSH =====
  defaultPort: number
  defaultUser: string
  defaultAuth: 'password' | 'key'
  jumpEnabled: boolean
  jumpHost: string
  jumpRetry: number
  // ===== 快捷键 =====
  shortcuts: Record<string, string>
  // ===== 外观（主题/强调色/终端背景色在 app store） =====
  baseFontSize: number
  terminalFont: string
  followDpi: boolean
  uiScale: number
  terminalOpacity: number
  // ===== AI 助手 =====
  aiModel: string
  aiApiKey: string
  aiTokenLimit: number
  aiContextWindow: number
  aiSystemPrompt: string
  aiStream: boolean
  // ===== 实验性 =====
  betaPlugin: boolean
  betaCmdAi: boolean
  betaSshShare: boolean
  // ===== 安全 =====
  clipboardClearSeconds: number
  clipboardClearEnabled: boolean
  /** 切出应用（切换到其它软件）时自动锁定 */
  lockOnAppSwitch: boolean
  encryptCredentials: boolean
  verifyKeyPermission: boolean
  /** 会话私钥托管文件的保存期限：永久 / 90 天未使用即清理（私钥内容仍存在会话里，可自动重建） */
  sshKeyCacheTtl: 'forever' | '90d'
  // ===== 同步 =====
  /** 自动云同步(仅登录用户): 本地变更防抖推送 + 启动/定时拉取合并 */
  autoSync: boolean
  /** 是否随会话同步 SSH 私钥内容；关闭后私钥只留在本机，不上传云端 */
  syncSshKeys: boolean
}

export const DEFAULT_SETTINGS: AppSettings = {
  autoLaunch: false,
  // Tauri 2.x 已支持系统托盘（lib.rs 中 TrayIconBuilder），开启后点 × 窗口隐藏，
  // 通过任务栏托盘图标右键"显示 JC Shell"恢复，"退出 JC Shell"彻底退出。
  minimizeToTray: true,
  confirmOnExit: true,
  multiSyncTrigger: 'doubleCtrl',
  fsToolbarToggle: 'singleAlt',
  updateChannel: 'stable',
  silentUpdate: true,
  proxyMode: 'system',
  manualProxyHost: '127.0.0.1',
  manualProxyPort: 7890,

  defaultShell: 'auto',
  terminalFontSize: 14,
  terminalLineHeight: 1.2,
  cursorStyle: 'bar',
  cursorBlink: true,
  scrollbackLines: 10000,
  sshKeepAlive: 30,
  sshConnectTimeout: 15,
  autoReconnect: true,
  maxReconnectTimes: 3,
  selectToCopy: true,
  rightClickPaste: true,
  allowDragUpload: true,
  bellAlert: false,
  highlightPrompt: true,
  middleCloseTab: false,
  // 默认固定显示（不自动隐藏）：侧边 Dock 常驻可见，避免新用户找不到功能入口
  sideDockPinned: true,
  forwardConfigStyle: 'page',

  defaultPort: 22,
  defaultUser: 'root',
  defaultAuth: 'password',
  jumpEnabled: false,
  jumpHost: '',
  jumpRetry: 3,

  shortcuts: { ...SHORTCUT_DEFAULT },

  baseFontSize: 14,
  terminalFont: 'JetBrains Mono',
  followDpi: true,
  uiScale: 100,
  terminalOpacity: 95,

  aiModel: 'gpt-4o-mini',
  aiApiKey: '',
  aiTokenLimit: 4096,
  aiContextWindow: 16,
  aiSystemPrompt: '',
  aiStream: true,

  betaPlugin: false,
  betaCmdAi: false,
  betaSshShare: false,

  clipboardClearSeconds: 60,
  clipboardClearEnabled: true,
  // 默认关闭：切出应用不再强制锁屏，由用户按需开启或手动锁定
  lockOnAppSwitch: false,
  encryptCredentials: true,
  verifyKeyPermission: true,
  sshKeyCacheTtl: 'forever',
  autoSync: true,
  syncSshKeys: true,
}

/**
 * 设备级偏好: 与"这台机器"绑定, 不参与云同步(上行剔除, 下行保留本地值)。
 * 同步这些项会直接坏事: 默认 shell 在 Windows 是 powershell、macOS 是 zsh;
 * 开机自启动写的是本机注册表; 代理地址与 DPI 缩放更是换台机器就错。
 */
export const DEVICE_LOCAL_PREF_KEYS: (keyof AppSettings)[] = [
  'autoLaunch', 'proxyMode', 'manualProxyHost', 'manualProxyPort',
  'defaultShell', 'followDpi', 'uiScale',
]

/** 敏感或依赖本机资材的开关: 任何情况下都不上云 */
export const NEVER_SYNC_PREF_KEYS: (keyof AppSettings)[] = [
  // 用户自备的模型 API Key: 不当凭证处理就是泄露
  'aiApiKey',
  // 本机落盘加密形式由本机决定, 不应被另一台设备的偏好改写
  'encryptCredentials',
]

/** 一次性迁移标记：sideDockPinned 默认值由「自动隐藏」改为「固定显示」后用于纠正存量数据 */
const SIDE_DOCK_PINNED_MIGRATED_KEY = 'jc_shell_migrated_side_dock_pinned_default'

/**
 * 存量数据的一次性默认值纠正。
 * 保存是整份 settings 落盘，因此只要用户改动过任意一项，历史默认值
 * （sideDockPinned: false）就会被一起写进存储 —— 光改 DEFAULT_SETTINGS 对这类用户无效。
 * 这里按标记纠正一次，此后完全尊重用户自己的选择（再次改回自动隐藏不会被覆盖）。
 * 标记写入失败则放弃纠正，避免存储不可用时每次启动都强制覆盖用户设置。
 */
function migrateSideDockPinnedDefault(saved: Partial<AppSettings>) {
  try {
    if (localStorage.getItem(SIDE_DOCK_PINNED_MIGRATED_KEY)) return
    localStorage.setItem(SIDE_DOCK_PINNED_MIGRATED_KEY, '1')
  } catch {
    return
  }
  saved.sideDockPinned = true
}

function load(): AppSettings {
  const merged: AppSettings = JSON.parse(JSON.stringify(DEFAULT_SETTINGS))
  try {
    const saved = JSON.parse(localStorage.getItem(STORAGE_KEY) || '{}') as Partial<AppSettings>
    migrateSideDockPinnedDefault(saved)
    for (const key of Object.keys(DEFAULT_SETTINGS) as (keyof AppSettings)[]) {
      const v = saved[key]
      if (v === undefined || v === null) continue
      if (key === 'shortcuts') {
        merged.shortcuts = { ...SHORTCUT_DEFAULT, ...(v as Record<string, string>) }
      } else {
        ;(merged as unknown as Record<string, unknown>)[key] = v
      }
    }
  } catch { /* ignore */ }
  return merged
}

export const useSettingsStore = defineStore('settings', () => {
  const settings = ref<AppSettings>(load())
  /** 正在应用其他窗口同步来的数据（避免再次广播形成环） */
  let applyingRemote = false

  // 修改即保存：防抖写 localStorage 并广播到其他窗口
  let saveTimer: number | null = null
  watch(settings, () => {
    if (applyingRemote) return
    if (saveTimer !== null) window.clearTimeout(saveTimer)
    saveTimer = window.setTimeout(() => {
      saveTimer = null
      try { localStorage.setItem(STORAGE_KEY, JSON.stringify(settings.value)) } catch { /* ignore */ }
      notifyPrefsChanged([STORAGE_KEY])
      emitConfigChanged('prefs')
    }, 150)
  }, { deep: true })

  // 其他窗口改了设置：合并到当前对象（仅赋值，不重发）
  // 注意：不能整体替换 ref —— 组件层可能缓存了对象引用（如设置弹窗的
  // v-model 绑定），替换后旧引用会悬空，后续修改不再触发 watch/持久化；
  // 且 emit 会广播到发送方自身，首条修改的回声就会触发替换导致断链。
  // Object.assign 保持对象身份稳定；Vue 响应式 setter 对同值赋值不触发，回声无副作用。
  const applyRemote = () => {
    applyingRemote = true
    try {
      Object.assign(settings.value, load())
    } finally {
      // 下一个微任务再放开，确保 watch 触发时标志仍有效
      queueMicrotask(() => { applyingRemote = false })
    }
  }
  onPrefsChanged((keys) => {
    if (!keys.includes(STORAGE_KEY)) return
    applyRemote()
  })
  // 双保险：同源窗口间 storage 事件同样会触发，
  // 确保终端字体/字号等修改在另一窗口实时生效
  window.addEventListener('storage', (e) => {
    if (e.key !== STORAGE_KEY) return
    applyRemote()
  })

  /** 局部更新（如终端 Ctrl+滚轮改字号），同样走持久化链路 */
  function patchSettings(patch: Partial<AppSettings>) {
    Object.assign(settings.value, patch)
  }

  /** 立即落盘并广播（跳过防抖，确认保存时用） */
  function flush() {
    if (saveTimer !== null) { window.clearTimeout(saveTimer); saveTimer = null }
    try { localStorage.setItem(STORAGE_KEY, JSON.stringify(settings.value)) } catch { /* ignore */ }
    notifyPrefsChanged([STORAGE_KEY])
    emitConfigChanged('prefs')
  }

  /**
   * 恢复默认设置：把所有设置项还原为 DEFAULT_SETTINGS。
   * 只影响设置项本身，不触碰会话/主机等业务数据。
   * 深拷贝避免把 DEFAULT_SETTINGS 里的引用（如 shortcuts）共享给响应式对象后被就地改写。
   */
  function resetAll() {
    Object.assign(settings.value, JSON.parse(JSON.stringify(DEFAULT_SETTINGS)) as AppSettings)
    flush()
  }

  /** 可同步的偏好子集: 剔除设备级与敏感字段(上行内容, 也是写入 base 快照的同一份数据) */
  function syncableValues(): Record<string, unknown> {
    const skip = new Set<string>([...DEVICE_LOCAL_PREF_KEYS, ...NEVER_SYNC_PREF_KEYS] as string[])
    const out: Record<string, unknown> = {}
    for (const key of Object.keys(DEFAULT_SETTINGS) as (keyof AppSettings)[]) {
      if (skip.has(key)) continue
      out[key] = settings.value[key]
    }
    return out
  }

  /**
   * 应用云端下发的偏好(只改白名单内且类型一致的字段, 设备级字段不碰)。
   * <p>逐字段校验而不是 Object.assign 整份: 一份被改坏或旧版本的文档不应该把本机配置捣乱。</p>
   */
  function applySyncedValues(values: Record<string, unknown>): number {
    const skip = new Set<string>([...DEVICE_LOCAL_PREF_KEYS, ...NEVER_SYNC_PREF_KEYS] as string[])
    let applied = 0
    for (const [key, val] of Object.entries(values || {})) {
      if (skip.has(key)) continue
      const def = (DEFAULT_SETTINGS as unknown as Record<string, unknown>)[key]
      if (def === undefined) continue
      // 类型不一致直接丢弃(快捷键子对象只取同类型的键值对)
      if (key === 'shortcuts') {
        if (!val || typeof val !== 'object') continue
        const src = val as Record<string, unknown>
        const merged: Record<string, string> = { ...settings.value.shortcuts }
        for (const [k, v] of Object.entries(src)) if (typeof v === 'string') merged[k] = v
        settings.value.shortcuts = merged
        applied++
        continue
      }
      if (typeof def !== typeof val) continue
      ;(settings.value as unknown as Record<string, unknown>)[key] = val
      applied++
    }
    if (applied > 0) flush()
    return applied
  }

  return { settings, patchSettings, flush, resetAll, syncableValues, applySyncedValues }
})
