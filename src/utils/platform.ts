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
 * 平台判定工具（全项目唯一的平台真相来源）
 * 作者：JC
 *
 * 背景：Tauri Android 上所有窗口都渲染进同一个 Activity 的视图栈，
 * `visible:false` 不生效 —— 后创建/后显示的窗口会永久置顶盖住主窗口。
 * 因此移动端必须绕开桌面多窗口架构（settings / term-* / tab-ghost 等辅助窗口），
 * 改用主窗口内的 dialog / 路由承载。桌面端逻辑完全不受影响。
 *
 * 设计约定（重要）：
 *   1. 平台差异只在本文件收敛。组件里不要再写裸的 UA 判断，
 *      一律使用 `isMobileWebview()` / `CAP.*`。
 *   2. UI 优先问「能力」（CAP.xxx）而不是「平台」（isMobileWebview），
 *      这样语义自解释、便于 review 与全局审计。
 *   3. 平台来源三级：dev 覆盖 > Tauri OS 插件（同步读取，最可靠）> UA 兜底。
 */
import * as os from '@tauri-apps/plugin-os'

export type PlatformType = 'windows' | 'linux' | 'macos' | 'android' | 'ios' | 'unknown'

/** dev 覆盖用：localStorage key（仅在开发期生效） */
const DEV_OVERRIDE_KEY = 'jc:platform-override'

let resolved: PlatformType | null = null

/** 解析平台：dev 覆盖 > Tauri OS 插件 > UA 兜底 */
function detect(): PlatformType {
  // 1) 开发期覆盖：?platform=mobile|desktop 或 localStorage（方便在 PC 上预览移动端 UI）
  //    仅 DEV 构建读取：localStorage 会跨构建持久残留，若生产也读取，
  //    开发机上调试移动端 UI 的残留会把正式版 PC 应用锁死在移动端布局。
  //    （铁律：移动端预览/适配不允许影响 PC 端，此开关必须在生产构建中彻底失效。）
  if (import.meta.env.DEV) {
    try {
      const q = new URLSearchParams(window.location.search).get('platform')
      const ov = q || window.localStorage.getItem(DEV_OVERRIDE_KEY)
      if (ov === 'mobile' || ov === 'android') return 'android'
      if (ov === 'ios') return 'ios'
      if (ov === 'desktop' || ov === 'windows' || ov === 'linux' || ov === 'macos') return 'windows'
    } catch { /* 非浏览器环境，忽略 */ }
  }

  // 2) Tauri OS 插件：Rust 端把平台注入 __TAURI_OS_PLUGIN_INTERNALS__，读取是同步的
  try {
    const t = os.type() as PlatformType
    if (t && t !== 'unknown') return t
  } catch { /* 非 Tauri 环境（如单元测试），走 UA 兜底 */ }

  // 3) UA 兜底
  const ua = typeof navigator !== 'undefined' ? navigator.userAgent : ''
  if (/android/i.test(ua)) return 'android'
  if (/iphone|ipad|ipod/i.test(ua)) return 'ios'
  return 'windows'
}

/** 当前平台（首次调用解析并缓存；dev 覆盖变更后需刷新页面） */
export function platformType(): PlatformType {
  if (resolved === null) resolved = detect()
  return resolved
}

/**
 * 开发期平台覆盖（仅用于在桌面/浏览器预览移动端 UI，避免每次开模拟器）。
 * 用法：控制台 `window.jcPlatform.set('mobile')` 后刷新；或访问 `?platform=mobile`。
 * 传 null 清除。生产环境勿调用。
 */
export function setPlatformOverride(v: 'mobile' | 'desktop' | null): void {
  try {
    if (v === null) window.localStorage.removeItem(DEV_OVERRIDE_KEY)
    else window.localStorage.setItem(DEV_OVERRIDE_KEY, v === 'mobile' ? 'android' : 'windows')
  } catch { /* ignore */ }
  resolved = null
}

/** 读取当前 dev 覆盖值（供设置面板展示）：auto / mobile / desktop */
export function getPlatformOverride(): 'auto' | 'mobile' | 'desktop' {
  try {
    const ov = window.localStorage.getItem(DEV_OVERRIDE_KEY)
    if (ov === 'android' || ov === 'mobile' || ov === 'ios') return 'mobile'
    if (ov) return 'desktop'
  } catch { /* ignore */ }
  return 'auto'
}

/** 是否运行在移动端 WebView（Android / iOS） */
export function isMobileWebview(): boolean {
  const p = platformType()
  return p === 'android' || p === 'ios'
}

/** 是否为 Android WebView（当前移动端适配目标） */
export function isAndroidWebview(): boolean {
  return platformType() === 'android'
}

/** 是否桌面端 */
export function isDesktop(): boolean {
  return !isMobileWebview()
}

/**
 * 能力标志：UI 只问「能力」不问「平台」。
 * 新增移动端差异时优先在此扩展，而不是在组件里写 isMobileWebview()。
 * 注意：这里是「UX 层」的开关；危险操作在 Rust 命令层另有一道守卫兜底。
 */
export const CAP = {
  /** 本地终端（真实本地 PTY）：移动端无本地 shell，禁用 */
  localTerminal: (): boolean => !isMobileWebview(),
  /** 桌面多窗口（settings / term-* / tab-ghost 独立窗口）：移动端单窗口 */
  multiWindow: (): boolean => !isMobileWebview(),
  /** Tab 拖出 / 跨窗口迁移：依赖桌面全局光标轮询 */
  tabDrag: (): boolean => !isMobileWebview(),
  /** OAuth 深链回调（jc-shell://）：需系统注册 scheme，当前仅 Android 真机支持 */
  oauthDeepLink: (): boolean => isAndroidWebview(),
  /** 窗口全屏（F11 / 终端悬浮按钮）：桌面窗口概念，移动端无 */
  windowFullscreen: (): boolean => !isMobileWebview(),
  /** 窗口拖拽移动（精简模式顶部拖拽条 / Alt 全窗拖拽及对应提示）：桌面窗口概念，移动端无 */
  windowDrag: (): boolean => !isMobileWebview(),
  /** 终端悬浮按钮「重连」：移动端重连入口在标签菜单，悬浮按钮不展示（省出屏幕、避免误触） */
  terminalQuickReconnect: (): boolean => !isMobileWebview(),
  /**
   * 终端页内的三块停靠面板（会话列表 / 设备信息 / 文件系统）。
   * 移动端改为底部导航里的独立页面承载，这里不仅是不显示，而是**不挂载** ——
   * 面板内部有轮询（设备指标、SFTP 目录同步），仅用 CSS 隐藏会让它们在后台空转。
   */
  dockPanels: (): boolean => !isMobileWebview(),
  /**
   * APP 内手机号登录（滑块验证码 + 短信验证码一步登录）：仅移动端启用。
   * 移动端无系统浏览器深链（jc-shell:// 在 iOS 不可用、Android 体验差），
   * 故用本命令内登录代替桌面端的 OAuth 跳转；桌面端仍走 OAuth，不受影响。
   */
  appPhoneLogin: (): boolean => isMobileWebview(),
  /**
   * 连接会话时的「连接中」过渡遮罩：仅移动端启用。
   * 桌面端连接过程直接显示在终端里（已有 connecting 文本 + 状态条），不需要全屏遮罩；
   * 移动端屏幕窄、跳转明显，用一层精美过渡动画盖住终端区，连接成功即消失。
   */
  connectingOverlay: (): boolean => isMobileWebview(),
  /**
   * 终端是否启用 WebGL 渲染器（xterm addon-webgl）。
   * Android/iOS WebView 的 WebGL 上下文数量有限且易在隐藏/后台时丢失，
   * 表现为「关闭标签后剩余终端变黑 / 新建终端也黑」（画布停留在旧尺寸或上下文耗尽）。
   * 移动端改用 xterm 默认 DOM 渲染器（稳定优先），桌面端继续用 WebGL 拿滚动性能。
   */
  gpuTerminalRenderer: (): boolean => !isMobileWebview(),
  /**
   * 生物识别解锁（系统指纹 / 人脸）：仅移动端接入。
   * 桌面端不引入 tauri-plugin-biometric，解锁路径仍只有 PIN，故为 false。
   * 这里只回答「平台是否具备该能力」；硬件是否存在、是否已录入指纹/人脸，
   * 属于运行时事实，由 `utils/biometric.ts` 向系统查询（checkStatus）。
   */
  biometricUnlock: (): boolean => isMobileWebview(),
} as const

/** 能力名联合类型，供声明式标注（如菜单项 cap 字段） */
export type Capability = keyof typeof CAP
