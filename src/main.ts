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

import { createApp } from 'vue'
import { createPinia } from 'pinia'
import ElementPlus from 'element-plus'
import 'element-plus/dist/index.css'
import 'element-plus/theme-chalk/dark/css-vars.css'
import zhCn from 'element-plus/es/locale/lang/zh-cn'
import en from 'element-plus/es/locale/lang/en'
import * as ElementPlusIconsVue from '@element-plus/icons-vue'
import App from './App.vue'
import router from './router'
import { useI18nStore } from '@/stores/i18n'
import { setUiFrozen } from '@/utils/uiFreeze'
import './style.css'
import '@xterm/xterm/css/xterm.css'
// 移动端布局覆盖（全部规则以 html.jc-platform-mobile 为前缀，桌面端零命中）
import './styles/mobile.css'
import { isMobileWebview, platformType, setPlatformOverride } from '@/utils/platform'
import { appName } from '@/utils/edition'

// 在 <html> 上打平台标记：移动端样式只认这一个开关，组件内无需再写平台判断
if (isMobileWebview()) {
  document.documentElement.classList.add('jc-platform-mobile')
}

// ===== 浏览器访问防护 =====
// dev 模式下 Vite 监听 127.0.0.1:1420，WebView 加载此地址，浏览器也能访问。
// 本前端必须在非 Tauri 环境中直接显示错误页并停止挂载 Vue，挡住普通用户通过浏览器使用。
// 注：这是基础防护，dev 模式 100% 防护不可能（必须监听 HTTP）。
//      生产模式 tauri build 后使用 tauri:// 协议，浏览器根本无法访问，天然安全。
if (typeof window === 'undefined' || !('__TAURI_INTERNALS__' in window)) {
  document.documentElement.lang = 'zh-CN'
  document.title = `${appName()} — 仅应用内部可用`
  document.body.style.cssText = 'margin:0;background:#1e1e1e;'
  document.body.innerHTML = `
    <div style="position:fixed;inset:0;display:flex;align-items:center;justify-content:center;background:#1e1e1e;color:#d4d4d4;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',system-ui,sans-serif;padding:32px;">
      <div style="text-align:center;max-width:520px;">
        <div style="font-size:64px;margin-bottom:24px;line-height:1;">🖥️</div>
        <div style="font-size:24px;font-weight:600;margin-bottom:16px;color:#fff;">${appName()} 仅可通过应用内部访问</div>
        <div style="font-size:14px;color:#999;line-height:1.8;">请通过已安装的 ${appName()} 应用打开本应用，不要在浏览器中访问此地址。</div>
      </div>
    </div>
  `
  // 阻止后续所有代码执行（Vue / Pinia / Router 都不会挂载）
  throw new Error('JC Shell is desktop-only; not running in Tauri context')
}

// Tauri 子窗口（如 settings / feedback）以纯路径 /xxx 打开，hash router 不会识别。
// 以窗口 label 为准强制写入 hash（比解析 pathname 更可靠：dev/prod、
// tauri.localhost 代理等场景下路径可能被重写，导致设置/反馈窗口回退到主界面路由 '/'）。
// 浏览器模式（无 Tauri 运行时）跳过此段，否则 import/getCurrentWebviewWindow 抛错会导致整个 main.ts 报错、Vue 不挂载
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
const _tauriWin = (() => {
  try { return getCurrentWebviewWindow() } catch { return null }
})()
if (_tauriWin) {
  const _winLabel = _tauriWin.label
  const _path = window.location.pathname
  if (_winLabel === 'settings') {
    // 设置窗口：无条件钉到 #/settings，防止回退渲染主界面
    if (window.location.hash !== '#/settings') {
      window.location.hash = '/settings'
    }
  } else if (_winLabel === 'feedback') {
    // 反馈窗口：无条件钉到 #/feedback
    if (window.location.hash !== '#/feedback') {
      window.location.hash = '/feedback'
    }
  } else if (_path && _path !== '/' && !window.location.hash) {
    // 其他纯路径打开的窗口（兜底）：把 path 写入 hash（# 与 path 共存时，hash 优先）
    window.location.hash = _path
  }
}

// 窗口拖拽防卡顿：在任意拖拽区（自定义标题栏 / data-tauri-drag-region）按下时，
// 给 body 加 window-dragging 冻结指针事件与过渡动画（见 style.css），
// 并置全局冻结标志让周期性后台任务跳过 —— 窗口移动由主线程消息队列驱动，
// 拖拽期间任何 JS 计算或重排都会直接阻塞移动造成掉帧；拖拽结束即恢复。
;(function setupWindowDragOptimize() {
  // 移动端没有桌面窗口拖动，且触摸会产生 mousedown → 会误触发「冻结」导致界面短暂无响应
  if (isMobileWebview()) return
  let guardTimer: number | null = null
  // 本地状态：避免每次 mousemove 都去读 classList
  let active = false

  const release = () => {
    if (!active) return
    active = false
    document.body.classList.remove('window-dragging')
    setUiFrozen(false)
    if (guardTimer !== null) { window.clearTimeout(guardTimer); guardTimer = null }
  }

  const freeze = () => {
    if (active) return
    active = true
    document.body.classList.add('window-dragging')
    // 让周期性后台任务（目录轮询 / SSH cwd 同步等）在拖拽期间跳过
    setUiFrozen(true)
  }

  // 原生窗口拖拽期间 WebView 收不到鼠标事件，结束事件不可靠，多重兜底：
  // 1) mouseup：绝大多数情况拖拽结束会派发；
  // 2) mousemove 且 buttons === 0：鼠标已松开 —— 拖拽中 buttons 恒为 1，
  //    用按键状态区分，避免把"拖拽途中的移动"误判成结束而提前解冻；
  // 3) 安全阀：以上都没来时超时自动恢复（防止界面长时间停在冻结态不响应点击）。
  const onMouseUp = () => release()
  const onMouseMove = (e: MouseEvent) => {
    if (active && e.buttons === 0) release()
  }

  document.addEventListener('mousedown', (e) => {
    const target = e.target as HTMLElement | null
    // 覆盖所有原生拖拽区：自定义标题栏 + 任意 data-tauri-drag-region（如设置窗口）
    if (!target?.closest?.('.window-title-bar, [data-tauri-drag-region]')) return
    // 窗口控制按钮（最小化/最大化/关闭）不参与窗口拖动，不冻结
    if (target.closest?.('.window-controls')) return
    // macOS 红黄绿三圆点：点击是窗口操作（关闭/最小化/最大化），不是拖拽
    if (target.closest?.('.mac-traffic-lights')) return
    // 双击拖拽区的第二次按下不冻结：冻结会给 #app 加 pointer-events:none，
    // 而鼠标事件是「派发前命中测试」的 —— 紧接着的 mouseup 会命中 body 而不是拖拽区，
    // Tauri 内置脚本在 macOS 上正是靠 mouseup 的 composedPath 判定双击最大化（Windows 用
    // mousedown 的 detail === 2，不受影响），冻结会导致 Mac 双击放大缩小失效。
    // 双击过程没有窗口移动，不冻结也不会有拖拽掉帧问题。
    if (e.detail === 2) return
    freeze()
    // 安全阀：原生窗口拖拽无可靠结束事件，超时自动恢复
    guardTimer = window.setTimeout(release, 15000)
  }, true)
  document.addEventListener('mouseup', onMouseUp, true)
  document.addEventListener('mousemove', onMouseMove, true)
})()

/**
 * 全局拦截 DevTools 快捷键：Ctrl+Shift+I / F12。
 * 根因在 Rust 层已禁用 DevTools（tauri.conf.json windows[].devtools:false +
 * WebviewWindowBuilder::devtools(false)），这里再加一道 JS 兜底，确保：
 *   1) 任何输入控件聚焦时都不会让浏览器调试窗口冒出来；
 *   2) 即便未来某个窗口忘了关 DevTools，键盘也打不开。
 * 不调用 stopImmediatePropagation —— TerminalWorkspace 的 Ctrl+Shift+I 捕获监听仍可触发 Agent 切换。
 */
function onGlobalDevtoolsGuard(e: KeyboardEvent) {
  if (e.key === 'F12') {
    e.preventDefault()
    return
  }
  if (e.ctrlKey && e.shiftKey && !e.altKey && !e.metaKey && (e.key === 'I' || e.key === 'i')) {
    e.preventDefault()
  }
}
window.addEventListener('keydown', onGlobalDevtoolsGuard, true)

// ===== 开发期平台覆盖（便于在 PC 上预览移动端 UI，避免每次都开模拟器）=====
// 用法：控制台执行 window.jcPlatform.set('mobile') 后刷新；
//       或直接访问 ?platform=mobile。传 null 清除。仅 DEV 生效。
if (import.meta.env.DEV) {
  ;(window as unknown as Record<string, unknown>).jcPlatform = {
    get: platformType,
    set: setPlatformOverride,
  }
  console.info('[jc] platform =', platformType())
}

const app = createApp(App)

for (const [key, component] of Object.entries(ElementPlusIconsVue)) {
  app.component(key, component)
}

const pinia = createPinia()
app.use(pinia)
app.use(router)

const i18n = useI18nStore(pinia)
app.config.globalProperties.$t = (k: string, p?: Record<string, string | number>) => i18n.t(k, p)

// 外观全局偏好：基础字号 / 界面缩放（实时生效 + 跨窗口同步）
import { initUiPrefs } from '@/utils/uiPrefs'
initUiPrefs()
// 移动端软键盘遮挡检测：写入 --jc-kb-overlap 供弹窗避让样式使用（见 mobile.css）
import { installMobileKeyboardShift } from '@/utils/mobileKeyboard'
installMobileKeyboardShift()
// 【临时】输入密码时黑屏的诊断线索（定位后连同 pinBlackTrace.ts 一起删除）
import { installPinBlackTrace } from '@/utils/pinBlackTrace'
installPinBlackTrace()
// 移动端系统返回（安卓返回键 / 全面屏手势）：由前端裁决「返回上层」或「二次确认退出」
import { installMobileBackHandler } from '@/utils/mobileBack'
installMobileBackHandler()
app.use(ElementPlus, {
  locale: i18n.locale === 'en' ? en : zhCn,
})
// 切换语言时同步 Element Plus locale：watch store.locale (reactive in setup, so use effect via getter)
import { watch, effect } from 'vue'
watch(() => i18n.locale, (next) => {
  const ep = next === 'en' ? en : zhCn
  // Element Plus 2.8: app.use 时已注入，动态切换通过注入的 key 重新 use 不稳定；
  // 我们用一个全局 window 属性 + 页面刷新的强手段代价太高，
  // 这里选择替换全局 $ELEMENT locale（Element Plus 的 runtime 机制）。
  try {
    const anyApp = app as any
    const epKey = Symbol.for('element-plus')
    if (anyApp._context?.app?.config) {
      anyApp._context.app.config.globalProperties.$ELEMENT = { locale: ep, size: 'default' }
    }
    const win = window as any
    if (!win.__JC_ELEMENT_LOCALE__) win.__JC_ELEMENT_LOCALE__ = ep
    document.documentElement.setAttribute('data-el-locale', next)
  } catch { /* ignore */ }
})

app.mount('#app')
