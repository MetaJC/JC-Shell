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
 * 移动端系统返回（安卓返回键 / 全面屏返回手势）统一处理
 * 作者：JC
 *
 * 原生侧（src-tauri/gen/android/.../MainActivity.kt）不再直接退出应用，而是通过
 * `evaluateJavascript` 调到这里，由前端按「浮层 → 子页 → 非首页 → 二次确认退出」
 * 的顺序裁决：
 *   - 返回 true  = 已消费，留在应用内；
 *   - 返回 false = 交由原生结束 Activity（真正退出）。
 *
 * 为什么在前端裁决：移动端没有路由栈，「上一层」是由 dock store 的移动端子页栈
 * 与底部导航当前页共同决定的，只有前端知道当前处于哪一层。
 */
import { ElMessage } from 'element-plus'
import { isMobileWebview } from '@/utils/platform'
import { useDockStore, MOBILE_DEFAULT_DOCK_PAGE } from '@/stores/dock'
import { useI18nStore } from '@/stores/i18n'

/** 二次确认窗口：第一次返回后，这段时间内再次返回才真正退出（毫秒） */
const EXIT_CONFIRM_WINDOW = 2000
/** 是否已进入「再次返回即退出」的待确认状态 */
let exitArmed = false
let exitTimer: ReturnType<typeof setTimeout> | null = null

/** 关闭 FAB 扇形菜单（遮罩上绑定了「点空白收起」）：返回是否关掉了 */
function closeFanMenu(): boolean {
  const mask = document.querySelector('.m-fan-mask') as HTMLElement | null
  if (!mask) return false
  mask.click()
  return true
}

/**
 * 关闭当前最上层的弹窗 / 浮层（等价于点右上角关闭按钮）。
 * EP 的弹窗关闭后 DOM 常驻（display:none），故需按 display 过滤。
 */
function closeTopOverlay(): boolean {
  const overlays = Array.from(document.querySelectorAll('.el-overlay')).filter(
    el => (el as HTMLElement).style.display !== 'none'
  )
  if (overlays.length === 0) return false
  const top = overlays[overlays.length - 1] as HTMLElement
  const btn = top.querySelector('.el-dialog__headerbtn, .el-message-box__headerbtn') as HTMLElement | null
  if (btn) {
    btn.click()
    return true
  }
  // 没有关闭按钮的浮层：等价于点遮罩关闭。
  // EP 的遮罩关闭由 useSameTarget 判定 —— 要求 mousedown / mouseup 都落在遮罩自身，
  // 只派发一个 click 不生效，故按顺序补齐三连事件（target 即 currentTarget）
  for (const type of ['mousedown', 'mouseup', 'click'] as const) {
    top.dispatchEvent(new MouseEvent(type, { bubbles: true, cancelable: true }))
  }
  return true
}

/**
 * 关闭自定义全屏浮层（非 Element Plus 弹窗体系）：
 *   .jav           — APP 内网页查看器（用户协议 / 隐私政策，z 4000，压在登录浮层之上）
 *   .m-phonelogin  — 手机号登录浮层
 * 返回键等价于点它们左上角的返回按钮，即「回到上一层」而不是退出应用。
 * 顺序：先关最上层的网页查看器，再关登录浮层。
 */
function closeCustomFullscreen(): boolean {
  for (const selector of ['.jav', '.m-phonelogin']) {
    const layer = document.querySelector(selector)
    if (!layer) continue
    const btn = layer.querySelector('.jav__back, .m-pl-back') as HTMLElement | null
    if (btn) {
      btn.click()
      return true
    }
  }
  return false
}

/** 进入待退出状态：弹一次系统提示（非弹窗），窗口内再返回才退出 */
function armExit(): void {
  exitArmed = true
  if (exitTimer) clearTimeout(exitTimer)
  // 同一时刻只留一条提示，避免连点返回堆积
  ElMessage.closeAll()
  ElMessage({
    message: useI18nStore().t('mobile.backAgainToExit'),
    type: 'info',
    duration: EXIT_CONFIRM_WINDOW,
  })
  exitTimer = setTimeout(() => {
    exitArmed = false
    exitTimer = null
  }, EXIT_CONFIRM_WINDOW)
}

/** 处理一次返回：true = 已消费（不退出），false = 允许原生退出应用 */
export function handleMobileBack(): boolean {
  const dockStore = useDockStore()
  // 1. FAB 扇形菜单打开中 → 先收起菜单
  if (closeFanMenu()) return true
  // 2. 自定义全屏浮层（协议查看器 / 手机登录）→ 关闭并回到上一层
  if (closeCustomFullscreen()) return true
  // 3. 移动端子页（新建 / 编辑会话等）→ 退回上一层
  if (dockStore.mobileSubPages.length > 0) {
    dockStore.popMobileSubPage()
    return true
  }
  // 4. 有弹窗 / 浮层 → 关掉它（等同于点关闭按钮）
  if (closeTopOverlay()) return true
  // 5. 不在默认页 → 回到默认页（会话列表），而不是直接退出应用
  if (dockStore.activePage !== MOBILE_DEFAULT_DOCK_PAGE) {
    dockStore.clearMobileSubPages()
    dockStore.setActivePage(MOBILE_DEFAULT_DOCK_PAGE)
    return true
  }
  // 6. 已在最上层 → 二次确认：第一次给提示，第二次才交给原生退出
  if (exitArmed) return false
  armExit()
  return true
}

/**
 * 安装返回手势处理（仅移动端）。
 * 原生侧通过 `window.__jcOnAndroidBack()` 调用，返回值会被 evaluateJavascript 读到。
 */
export function installMobileBackHandler(): void {
  if (!isMobileWebview()) return
  ;(window as unknown as { __jcOnAndroidBack?: () => boolean }).__jcOnAndroidBack = handleMobileBack
}
