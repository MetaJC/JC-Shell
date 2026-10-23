<!--
  Copyright 2024-2026 OpenJC (https://www.openjc.cc)
  
  Licensed under the Apache License, Version 2.0 (the "License");
  you may not use this file except in compliance with the License.
  You may obtain a copy of the License at
  
      http://www.apache.org/licenses/LICENSE-2.0
  
  Unless required by applicable law or agreed to in writing, software
  distributed under the License is distributed on an "AS IS" BASIS,
  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
  See the License for the specific language governing permissions and
  limitations under the License.
-->

<template>
  <div class="layout-root h-full w-full flex flex-col">
    <!-- 自定义窗口标题栏：左侧菜单 + 中间拖拽区 + 右侧窗口控制按钮（精简/全屏模式隐藏） -->
    <div v-if="!chromeHidden" class="window-title-bar" :class="{ 'is-mac': isMac }" data-tauri-drag-region>
      <!-- Windows / Linux：HTML 菜单栏 + 中间拖拽区 -->
      <template v-if="!isMac">
        <TopMenuBar @action="onMenuAction" />
        <div class="title-bar-drag-area flex-1" data-tauri-drag-region />
      </template>
      <!-- macOS：菜单由系统菜单栏位置（Tauri Menu API）提供，此处只保留中间拖拽区。
           拖拽 +「双击放大 / 还原」都由 Tauri 内置拖拽区脚本处理；
           所有平台均使用 data-tauri-drag-region 触发拖拽（不使用 -webkit-app-region，
           那是 Electron 专有属性、Tauri 不识别），否则双击放大缩小失效 -->
      <template v-else>
        <div class="title-bar-drag-area flex-1" data-tauri-drag-region />
      </template>

      <div class="window-controls">
        <UpdateReadyBtn />
        <button class="wc-btn wc-pin" :class="{ 'wc-pin-active': alwaysOnTop }" @click="onToggleAlwaysTop" :title="alwaysOnTop ? '取消窗口置顶' : '窗口保持最前'">
          <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round">
            <path d="M5.5 2.5h5v4l1.5 2.5h-8L5.5 6.5v-4z"/>
            <line x1="8" y1="9" x2="8" y2="13.5"/>
          </svg>
        </button>
        <!-- 最小化 / 最大化 / 关闭：仅 Windows / Linux 显示；
             macOS 用原生三圆点（Overlay 标题栏），避免与原生控件重复 -->
        <template v-if="!isMac">
          <button class="wc-btn wc-min" @click="onMinimize" :title="t('common.minimize')">
            <svg viewBox="0 0 12 12" width="12" height="12"><rect x="1" y="5.5" width="10" height="1" fill="currentColor"/></svg>
          </button>
          <button class="wc-btn wc-max" @click="onToggleMax" :title="isMaximized ? t('common.restore') : t('common.maximize')">
            <svg v-if="isMaximized" viewBox="0 0 12 12" width="12" height="12">
              <rect x="3" y="1" width="8" height="8" fill="none" stroke="currentColor" stroke-width="1"/>
              <rect x="1" y="3" width="8" height="8" fill="var(--jc-bg)" stroke="currentColor" stroke-width="1"/>
            </svg>
            <svg v-else viewBox="0 0 12 12" width="12" height="12">
              <rect x="1" y="1" width="10" height="10" fill="none" stroke="currentColor" stroke-width="1"/>
            </svg>
          </button>
          <button class="wc-btn wc-close" @click="onClose" :title="t('common.close')">
            <svg viewBox="0 0 12 12" width="12" height="12">
              <line x1="2" y1="2" x2="10" y2="10" stroke="currentColor" stroke-width="1.2"/>
              <line x1="10" y1="2" x2="2" y2="10" stroke="currentColor" stroke-width="1.2"/>
            </svg>
          </button>
        </template>
      </div>
    </div>
    <!-- m-body--terminal：移动端当前停在终端页时打标，供软键盘避让样式只作用于终端页
         （终端页图层常驻挂载，仅靠 DOM 结构无法区分「哪一页在前台」） -->
    <div
      ref="layoutBodyEl"
      class="layout-body flex flex-1 min-h-0 zen-no-drag"
      :class="{ 'm-body--terminal': isMobile && activePage === 'terminal' }"
    >
      <!-- 桌面端：窗口最左缘的竖排侧边 Dock（按钮即页面 Tab，支持自动隐藏 / 固定显示）。
           移动端改用屏幕底部导航（见本容器末尾的 MobileTabBar）—— 触摸端拇指可达区在底部，
           且竖条在窄屏上会挤占内容宽度。 -->
      <SideDock v-if="!chromeHidden && !isMobile" />

      <!--
        页面宿主：占满 Dock 右侧的剩余宽度。
        每个页面都用普通 div 绝对定位铺满宿主（而不是给页面组件本身加 v-show）——
        这样即使页面组件是多根/带注释的模板，也不会出现“两个页面同时参与 flex 分宽”
        导致右侧留出大片空白、或切页看不出变化的问题。
      -->
      <div
        ref="hostEl"
        class="relative flex-1 min-w-0 min-h-0 m-page-host"
        @touchstart.passive="onPageTouchStart"
        @touchend="onPageTouchEnd"
      >
        <!-- 终端页（默认页）：常驻挂载，切到其它页面时只隐藏不卸载，终端会话不中断。
             m-page-layer--terminal：移动端软键盘避让时按此收短这一层高度（见 mobile.css 第 21 节） -->
        <div
          ref="terminalLayerEl"
          v-show="activePage === 'terminal'"
          class="absolute inset-0 flex m-page-layer m-page-layer--terminal"
          :class="pageAnimClass"
        >
          <TerminalPage
            :chrome-hidden="chromeHidden"
            @open-host="(h: HostItem) => emit('openHost', h)"
          >
            <slot />
          </TerminalPage>
        </div>

        <!-- 其它页面：由 pages/index.ts 注册表提供。
             移动端这一层与 keep-alive 都常驻（只切换内部组件）：若切到终端页时把
             keep-alive 一起卸载，缓存会全部丢失，再回 SFTP 就变成重新挂载
             （目录栈重置 + 重新拉远程列表）。
             桌面端维持原行为（v-if 卸载整层）：FtpPage 依赖 onBeforeUnmount 回收远端会话。
             注：切到终端页时 activePageComponent 为 undefined，缓存组件会被
             keep-alive 停用（onDeactivated）→ 后台轮询自动停，切回再恢复。 -->
        <div
          v-if="isMobile || activePage !== 'terminal'"
          v-show="activePage !== 'terminal'"
          class="absolute inset-0 flex flex-col m-page-layer"
          :class="pageAnimClass"
        >
          <keep-alive>
            <component v-if="activePageComponent" :is="activePageComponent" :key="activePage" />
          </keep-alive>
        </div>
      </div>

      <!-- 移动端：屏幕底部导航栏（会话 / 终端 / SFTP / 仪表盘 / 我的）。
           与桌面 SideDock 共用同一个 dock store，「当前页在前台」只有一份真相，
           因此两个平台下的切页语义完全一致，只是导航形态不同（底部横排 vs 左侧竖排）。 -->
      <MobileTabBar v-if="!chromeHidden && isMobile" />
    </div>

    <!-- 移动端子页面（新建/编辑会话等）：整页覆盖在底部导航之上，从右滑入、返回时滑出。
         key 用「栈深 + 目标会话」保证换一个会话时重新挂载表单（按新参数初始化）。 -->
    <Transition name="m-subpage">
      <MobileSessionEditPage
        v-if="isMobile && !chromeHidden && topMobileSubPage"
        :key="mobileSubPageKey"
        :page="topMobileSubPage"
        class="m-subpage"
      />
    </Transition>

    <!-- 拖拽磁吸预览遮罩 -->
    <DragOverlay />

    <!-- 精简模式：顶部悬浮拖拽条（隐藏时不影响文字选中；全屏模式不显示）
         仅桌面端：移动端没有「窗口拖拽移动」概念，拖拽条/提示都不展示 -->
    <div
      v-if="app.zenMode && canDragWindow"
      class="zen-drag-bar"
      :class="{ 'is-visible': dragBarVisible, 'is-alt': altDragging }"
      data-tauri-drag-region
    />
    <!-- 精简模式：Alt 按住时全窗口拖拽遮罩 -->
    <div v-if="app.zenMode && altDragging && canDragWindow" class="zen-drag-overlay" data-tauri-drag-region />

    <!-- 精简模式：底部拖拽提示（每次进入精简模式显示，10 秒后自动消失） -->
    <div v-if="zenTipVisible && canDragWindow" class="zen-tip">{{ dragTipText }}</div>
  </div>
</template>

<script setup lang="ts">
/**
 * Dock 布局容器（主窗口与独立终端窗口共用）
 * 作者：JC
 * 三面板动态停靠（左/右/底）+ 分隔条 + 折叠态尺寸。
 * 用户头像已移入左侧停靠面板底部（见 TerminalPage），跟随左面板折叠/展开自动隐藏/显示。
 */
import { ref, computed, onMounted, onBeforeUnmount, watch, provide } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useAppStore } from '@/stores/app'
import { useI18nStore } from '@/stores/i18n'
import { useDockStore, DEFAULT_DOCK_PAGE, MOBILE_PAGE_ORDER } from '@/stores/dock'
import { CAP, isMobileWebview } from '@/utils/platform'
import { KB_TOGGLE_EVENT, onKeyboardMetricsChange, nativeKeyboardCssPx } from '@/utils/mobileKeyboard'
import { OPEN_HOST_KEY } from '@/composables/useOpenHost'
import DragOverlay from '@/components/common/DragOverlay.vue'
import SideDock from './SideDock.vue'
import MobileTabBar from './MobileTabBar.vue'
import MobileSessionEditPage from '@/components/pages/mobile/MobileSessionEditPage.vue'
import TerminalPage from '@/components/pages/TerminalPage.vue'
import { DOCK_PAGES } from '@/components/pages'
import TopMenuBar from './TopMenuBar.vue'
import UpdateReadyBtn from '@/components/update/UpdateReadyBtn.vue'
import type { HostItem } from '@/stores/host'

const app = useAppStore()
const t = useI18nStore().t
const dockStore = useDockStore()

/** 精简模式下的窗口拖拽（顶部拖拽条 / Alt 全窗拖拽 / 对应提示）是否可用：桌面端专属能力 */
const canDragWindow = CAP.windowDrag()

/** 当前选中的页面（侧边 Dock 按钮即页面 Tab） */
const activePage = computed(() => dockStore.activePage)
/** 非终端页组件（终端页需要承载默认插槽，由本组件单独常驻渲染） */
const activePageComponent = computed(() => DOCK_PAGES[activePage.value])

/** 平台检测：Mac 用原生三圆点（Overlay 标题栏）并隐藏 Windows 风格按钮；Windows/Linux 自绘按钮 + 菜单栏 */
const isMac =
  typeof navigator !== 'undefined' &&
  /Mac/i.test(navigator.userAgent) &&
  !/Windows/i.test(navigator.userAgent)

/** 精简模式拖拽提示：macOS 的 Option 键对应其它平台的 Alt 键 */
const dragTipText = computed(() =>
  isMac ? '按住 Option 键可以拖拽移动窗口' : '按住 Alt 键可以拖拽移动窗口'
)

/** 隐藏标题栏/停靠面板/底部导航：精简模式或窗口全屏（全屏时终端 Tab 标签由工作区保留） */
const chromeHidden = computed(() => app.zenMode || app.appFullscreen)

/** 是否移动端：决定渲染竖排 SideDock 还是底部 MobileTabBar（平台在启动时固定） */
const isMobile = isMobileWebview()

/** 移动端子页栈顶（新建/编辑会话整页）；空则无子页，直接显示底部导航页 */
const topMobileSubPage = computed(() => dockStore.mobileSubPages[dockStore.mobileSubPages.length - 1] || null)

// ===== 移动端：终端页软键盘避让（终端窗口底边整体升到键盘上沿之上） =====
/**
 * 全部用实测几何，不做任何估算：
 * - 键盘上沿 = visualViewport 的底边（vv.height + vv.offsetTop，布局坐标系）；
 * - 「是否被键盘挡住」= 页面区（.layout-body）自身底边 − 键盘上沿；
 * - 终端页图层应占的高度 = 键盘上沿 − 图层顶边（getBoundingClientRect().top）。
 *
 * 为什么不用遮挡量 / innerHeight 反推：实测这台机的 WebView 在布局并未重排
 * （键盘只是盖在上面）时也会把 innerHeight 一起报小，按遮挡量算会得 0 或差一截；
 * 顶部安全区也是兜底估值。上面这套几何量全部当场测得，与 adjustPan / adjustResize、
 * 状态栏高度、键盘动画中间态都无关，也不依赖 mobileKeyboard.ts 的判定。
 * 键盘收起时清掉内联高度，图层恢复 inset-0 满高（见 mobile.css 第 21 节）。
 */
const terminalLayerEl = ref<HTMLElement | null>(null)
/** 页面区元素：只用来取「没被键盘挡住时」的底边基准 */
const layoutBodyEl = ref<HTMLElement | null>(null)

/** 判定为「键盘弹出」的最小遮挡高度：状态栏圆角之类的几像素差不能被误判成键盘 */
const KB_MIN_COVERED = 60
/** 图层最小可用高度：个别机型动画期间会短暂上报极小的可视高度
 *  （与 mobile.css 第 21 节的 min-height 保持一致） */
const KB_MIN_LAYER_H = 120
/**
 * 额外安全余量（px）。理论上「键盘上沿 − 图层顶边」已经精确到像素，
 * 但个别输入法的工具栏（键盘上方那一排）不进可视区上报，真机上可能仍压住几像素；
 * 若出现这种情况，把这里调大（例如 12 / 24）即可，其它都不用动。
 */
const KB_SAFE_MARGIN = 0

let kbRaf = 0
/** 最近一次写入的内联高度（-1 表示当前没有内联高度），避免重复写触发无谓回流 */
let kbAppliedH = -1
/** 布局视口（ICB）高度基线：键盘未弹出时的全高，用来判断布局是否已随键盘让出高度 */
let layoutMaxH = 0

/** 清掉内联高度：图层恢复 inset-0 的满高 */
function clearTerminalLayerHeight(): void {
  const layer = terminalLayerEl.value
  if (!layer || kbAppliedH === -1) return
  layer.style.height = ''
  layer.style.bottom = ''
  kbAppliedH = -1
}

function syncTerminalLayerHeight(): void {
  const layer = terminalLayerEl.value
  const body = layoutBodyEl.value
  const vv = window.visualViewport
  const root = document.documentElement

  // 键盘不支持检测 / 终端页当前不可见（v-show 后没有渲染盒子）：恢复满高
  if (!layer || !body || !vv || layer.getClientRects().length === 0) {
    root.classList.remove('jc-kb-avoid')
    clearTerminalLayerHeight()
    return
  }

  // 布局视口高度（ICB）：原生把键盘内边距下发到内容视图后，它会真的变小
  const layoutH = root.clientHeight
  layoutMaxH = Math.max(layoutMaxH, layoutH)
  const layoutShrink = Math.max(0, layoutMaxH - layoutH)

  // 键盘上沿（布局坐标系）取以下候选里最小的那个：
  // 1) visualViewport 底边；2) innerHeight；3) 原生上报键盘高度反推的「视口高 − 键盘高」。
  // 第 3 条是关键：部分机型 WebView 视口完全不随键盘变化，前两条都测不到，
  // 只有原生报过来的键盘高度能把「键盘上沿」算出来（见 MainActivity.notifyKeyboardToWebView）
  const candidates = [vv.height + vv.offsetTop, window.innerHeight]
  const nativeKb = nativeKeyboardCssPx()
  if (nativeKb > 0) candidates.push(window.innerHeight - nativeKb)
  const visibleBottom = Math.min(...candidates)
  const covered = body.getBoundingClientRect().bottom - visibleBottom
  // 前端还需要自己补的收短量 = 可视区被挡掉的高度 − 布局已经让出的高度。
  // 必须扣掉「已让出」的部分：原生 resize 生效时（MainActivity 把 IME 高度做成内边距）
  // 布局本身已经收好了，若可视视口再报一次键盘就会重复扣成两个键盘高度（终端被压成一条）
  const extra = Math.max(0, covered - layoutShrink)
  // jc-kb-avoid：本组件实测出的「前端还需补位」标记，供 CSS 隐藏底部导航（与
  // mobileKeyboard.ts 的 jc-kb-open 互为备份，任一命中即可）
  const avoid = extra > KB_MIN_COVERED
  root.classList.toggle('jc-kb-avoid', avoid)
  if (!avoid) {
    clearTerminalLayerHeight()
    return
  }

  const h = Math.max(
    KB_MIN_LAYER_H,
    Math.round(visibleBottom - layer.getBoundingClientRect().top) - KB_SAFE_MARGIN
  )
  if (h === kbAppliedH) return
  kbAppliedH = h
  layer.style.height = `${h}px`
  // 图层本身是 inset-0（top/bottom 都是 0），同时给 height 属于过度约束，
  // 显式让 bottom 失效，高度说了算
  layer.style.bottom = 'auto'
}

/** 旋转屏：布局视口全高变化，重新取基线 */
function resetLayoutBaseline(): void {
  layoutMaxH = document.documentElement.clientHeight
  scheduleTerminalLayerHeight()
}

/** 合并到每帧一次：键盘滑入/滑出的动画期间 vv 事件非常密集 */
function scheduleTerminalLayerHeight(): void {
  if (kbRaf) return
  kbRaf = requestAnimationFrame(() => {
    kbRaf = 0
    syncTerminalLayerHeight()
  })
}

/** 键盘几何订阅的退订函数（仅移动端订阅过） */
let offKbMetrics: (() => void) | null = null

if (isMobile) {
  // 布局视口基线在挂载时取一次（此刻键盘必然未弹出），供 layoutShrink 判定使用
  layoutMaxH = document.documentElement.clientHeight
  // 旋转屏后布局视口全高变化，重置基线，避免旧最大值让 layoutShrink 恒大于 0
  window.addEventListener('orientationchange', resetLayoutBaseline)
  // 主路径：订阅 mobileKeyboard 的「数值已刷新」回调 —— 事件驱动与焦点期轮询兜底
  // 都会走到这里，是键盘动画过程中跟随高度变化最可靠的一路
  offKbMetrics = onKeyboardMetricsChange(scheduleTerminalLayerHeight)
  // 次路径：自己再听一遍视口事件与状态翻转事件，覆盖 mobileKeyboard 未安装的情形
  const vv = window.visualViewport
  vv?.addEventListener('resize', scheduleTerminalLayerHeight)
  vv?.addEventListener('scroll', scheduleTerminalLayerHeight)
  window.addEventListener('resize', scheduleTerminalLayerHeight)
  document.addEventListener(KB_TOGGLE_EVENT, scheduleTerminalLayerHeight)
}
// 切到终端页时补算一次（键盘可能已经开着）；子页盖住终端时则清掉
watch([activePage, topMobileSubPage], () => scheduleTerminalLayerHeight())

// ===== 移动端左右滑动切页 =====
/** 页面宿主元素：手势起点的祖先链检查到它为止 */
const hostEl = ref<HTMLElement | null>(null)
/** 正在播放的切页动效方向（空串 = 不动效）；播完清空，下次才能重新触发同名动画 */
const pageAnim = ref<'' | 'next' | 'prev'>('')
/** 动效时长（与 mobile.css 中的 animation 时长一致） */
const PAGE_SLIDE_MS = 260
let pageAnimTimer: ReturnType<typeof setTimeout> | null = null
/** 当前页的动效类（新页从左侧 / 右侧平移到位的动画） */
const pageAnimClass = computed(() =>
  isMobile && pageAnim.value
    ? (pageAnim.value === 'next' ? 'is-slide-next' : 'is-slide-prev')
    : undefined
)
/** 手势起点（CSS 像素） */
let swipeStartX = 0
let swipeStartY = 0
/** 本次触摸是否在做切页手势（起点被排除时为 false） */
let swipeTracking = false
/** 触发切页所需的最小水平位移（太小会把点击/纵向滚动误判成滑动） */
const SWIPE_MIN_X = 70
/** 水平位移需明显大于垂直位移，避免与页面纵向滚动冲突 */
const SWIPE_RATIO = 1.6

/**
 * 触摸起点是否落在「可横向滚动」的容器内。
 * 这类区域的左右滑动是在滚内容（典型：SFTP 文件列表 / 终端横向滚动），
 * 必须让位给内容滚动，不能触发切页。自下而上找祖先，命中即可。
 */
function inHorizontalScroller(start: HTMLElement | null): boolean {
  let el: HTMLElement | null = start
  while (el && el !== hostEl.value) {
    const overflowX = getComputedStyle(el).overflowX
    if ((overflowX === 'auto' || overflowX === 'scroll') && el.scrollWidth - el.clientWidth > 4) {
      return true
    }
    el = el.parentElement
  }
  return false
}

/** 是否禁止滑动切页：子页打开、弹窗打开、FAB 遮罩打开时都不切 */
function isSwipeBlocked(): boolean {
  if (dockStore.mobileSubPages.length > 0) return true
  // EP 弹窗关闭后 DOM 常驻（display:none），需排除隐藏的 overlay
  const overlays = document.querySelectorAll('.el-overlay')
  for (const el of Array.from(overlays)) {
    if ((el as HTMLElement).style.display !== 'none') return true
  }
  return !!document.querySelector('.m-fan-mask')
}

function onPageTouchStart(e: TouchEvent): void {
  swipeTracking = false
  if (!isMobile || chromeHidden.value) return
  if (e.touches.length !== 1) return
  // 滑块与显式标记的区域不参与切页手势（它们有自己的横向交互）
  const target = e.target as HTMLElement | null
  if (target?.closest?.('.el-slider, [data-swipe-ignore]')) return
  // 起点在可横向滚动的容器里（如 SFTP 文件列表）→ 用户在滚内容，不切页
  if (inHorizontalScroller(target)) return
  if (isSwipeBlocked()) return
  const t = e.touches[0]
  swipeStartX = t.clientX
  swipeStartY = t.clientY
  swipeTracking = true
}

function onPageTouchEnd(e: TouchEvent): void {
  if (!swipeTracking) return
  swipeTracking = false
  const t = e.changedTouches[0]
  if (!t) return
  const dx = t.clientX - swipeStartX
  const dy = t.clientY - swipeStartY
  if (Math.abs(dx) < SWIPE_MIN_X || Math.abs(dx) < Math.abs(dy) * SWIPE_RATIO) return
  // 左滑 → 下一页；右滑 → 上一页
  switchAdjacentPage(dx < 0 ? 1 : -1)
}

/** 按底部导航顺序切到相邻页（滑动方向由 store 的 setActivePage 记录，驱动页面平移动效） */
function switchAdjacentPage(step: number): void {
  const order = MOBILE_PAGE_ORDER as readonly string[]
  const idx = order.indexOf(dockStore.activePage)
  if (idx < 0) return
  const next = idx + step
  if (next < 0 || next >= order.length) return
  dockStore.clearMobileSubPages()
  dockStore.setActivePage(order[next])
}

// 页面切换 → 播放一次「新页整体从侧边平移到位」的动效
watch(activePage, () => {
  if (!isMobile) return
  const dir = dockStore.pageSlide
  if (pageAnimTimer) { clearTimeout(pageAnimTimer); pageAnimTimer = null }
  pageAnim.value = dir
  if (!dir) return
  pageAnimTimer = setTimeout(() => {
    pageAnim.value = ''
    pageAnimTimer = null
  }, PAGE_SLIDE_MS)
})
/** 子页 key：栈深 + 目标会话，换会话/进出栈都触发重新挂载，表单按新参数初始化 */
const mobileSubPageKey = computed(() =>
  `${dockStore.mobileSubPages.length}-${topMobileSubPage.value?.hostId ?? 'new'}-${topMobileSubPage.value?.defaultParentId ?? ''}`
)

// 精简/全屏模式下 Dock 会整体隐藏、无法切页：强制回到终端页，避免停在其它页面出不来
watch(chromeHidden, (hidden) => {
  if (hidden) {
    dockStore.clearMobileSubPages()
    dockStore.setActivePage(DEFAULT_DOCK_PAGE)
  }
})

const emit = defineEmits<{ (e: 'openHost', host: HostItem): void; (e: 'menuAction', key: string): void }>()

/**
 * 向子页面注入「连接会话」入口。
 * 移动端「会话」页是页面注册表里的独立组件（注册表渲染不带 props/emits 管道），
 * 用 provide/inject 让它可以复用与桌面会话面板完全相同的连接链路（→ MainWindow → TerminalWorkspace）。
 */
provide(OPEN_HOST_KEY, (host: HostItem) => emit('openHost', host))

// ===== 窗口控制（自定义标题栏按钮）=====
const isMaximized = ref(false)
/** 窗口保持最前（置顶）状态 */
const alwaysOnTop = ref(false)
async function onMinimize() { await getCurrentWindow().minimize() }
async function onToggleMax() {
  const win = getCurrentWindow()
  if (isMaximized.value) {
    await win.unmaximize()
    isMaximized.value = false
  } else {
    await win.maximize()
    isMaximized.value = true
  }
}
async function onClose() { await getCurrentWindow().close() }
async function onToggleAlwaysTop() {
  try {
    const next = !alwaysOnTop.value
    await getCurrentWindow().setAlwaysOnTop(next)
    alwaysOnTop.value = next
  } catch { /* 置顶切换失败忽略 */ }
}

/** 精简/全屏模式拖拽方案：
 * - 顶部 3px 透明条，光标移到顶部时显示半透明拖拽条
 * - 隐藏时 pointer-events:none，不影响文字选中
 * - Alt 按住时全窗口可拖拽（遮罩层 -webkit-app-region:drag） */
const dragBarVisible = ref(false)
const altDragging = ref(false)
const DRAG_BAR_HEIGHT = 28

function onZenMouseMove(e: MouseEvent) {
  // 仅精简模式出现拖拽条：全屏模式不显示（模板同样以 app.zenMode 控制）
  if (!app.zenMode) return
  if (altDragging.value) return
  dragBarVisible.value = e.clientY <= DRAG_BAR_HEIGHT
}

function onKeyDown(e: KeyboardEvent) {
  if (!app.zenMode) return
  if (e.key === 'Alt' && !e.repeat) {
    altDragging.value = true
    dragBarVisible.value = false
  }
}
function onKeyUp(e: KeyboardEvent) {
  if (e.key === 'Alt') {
    altDragging.value = false
  }
}

// ===== 精简模式底部提示 =====
/** 「按住 Alt 键可以拖拽移动窗口」提示停留时长（毫秒） */
const ZEN_TIP_DURATION = 10_000
const zenTipVisible = ref(false)
let zenTipTimer: ReturnType<typeof setTimeout> | null = null

function clearZenTipTimer(): void {
  if (zenTipTimer) {
    clearTimeout(zenTipTimer)
    zenTipTimer = null
  }
}
/** 进入精简模式：显示提示，10 秒后自动隐藏（移动端无需拖拽提示，直接不显示） */
function showZenTip(): void {
  clearZenTipTimer()
  if (!canDragWindow) return
  zenTipVisible.value = true
  zenTipTimer = setTimeout(() => {
    zenTipTimer = null
    zenTipVisible.value = false
  }, ZEN_TIP_DURATION)
}
function hideZenTip(): void {
  clearZenTipTimer()
  zenTipVisible.value = false
}

/** 每次进入精简模式都提示一次；退出精简模式立即收起 */
watch(() => app.zenMode, (zen) => {
  if (zen) showZenTip()
  else hideZenTip()
})

function onMenuAction(key: string) { emit('menuAction', key) }

onMounted(async () => {
  try { isMaximized.value = await getCurrentWindow().isMaximized() } catch { /* ignore */ }
  // macOS：监听后端原生菜单（Tauri Menu API 创建）点击事件，id 复用 TopMenuBar 的 key，
  // 走和 HTML 菜单完全相同的处理链路（onMenuAction → emit menuAction → MainWindow 处理）
  try {
    const { listen } = await import('@tauri-apps/api/event')
    await listen<string>('menu:action', (event) => {
      const key = event.payload
      if (typeof key === 'string' && key) onMenuAction(key)
    })
  } catch { /* 非 Tauri 环境或权限异常：忽略 */ }
  try { alwaysOnTop.value = await getCurrentWindow().isAlwaysOnTop() } catch { /* ignore */ }
  // 精简模式拖拽相关监听仅桌面端需要（移动端无 Alt 键、无窗口拖拽，注册了也是空转）
  if (canDragWindow) {
    window.addEventListener('mousemove', onZenMouseMove, true)
    window.addEventListener('keydown', onKeyDown, true)
    window.addEventListener('keyup', onKeyUp, true)
  }
})

onBeforeUnmount(() => {
  window.removeEventListener('mousemove', onZenMouseMove, true)
  window.removeEventListener('keydown', onKeyDown, true)
  window.removeEventListener('keyup', onKeyUp, true)
  clearZenTipTimer()
  if (pageAnimTimer) { clearTimeout(pageAnimTimer); pageAnimTimer = null }
  // 软键盘避让监听（仅移动端装过）
  offKbMetrics?.()
  offKbMetrics = null
  const vv = window.visualViewport
  vv?.removeEventListener('resize', scheduleTerminalLayerHeight)
  vv?.removeEventListener('scroll', scheduleTerminalLayerHeight)
  window.removeEventListener('resize', scheduleTerminalLayerHeight)
  window.removeEventListener('orientationchange', resetLayoutBaseline)
  document.removeEventListener(KB_TOGGLE_EVENT, scheduleTerminalLayerHeight)
  if (kbRaf) { cancelAnimationFrame(kbRaf); kbRaf = 0 }
  document.documentElement.classList.remove('jc-kb-avoid')
})
</script>

<style scoped>
.layout-root {
  background-color: var(--jc-bg);
}

/* 自定义窗口标题栏：整体可拖拽 */
.window-title-bar {
  height: 28px;
  background: var(--jc-bg-soft);
  border-bottom: 1px solid var(--jc-border);
  display: flex;
  align-items: center;
  user-select: none;
  flex-shrink: 0;
  -webkit-app-region: drag;
  position: relative;
}
/* macOS：自绘标题栏内容从 x=78 开始排布，让出原生三圆点宽度（三圆点由 Overlay 标题栏提供，
 * 绘制在 webview 之上，HTML 遮不住，只能靠留空共存）。 */
.window-title-bar.is-mac {
  padding-left: 78px;
}
/* 拖拽槽：显式声明 drag，避免子元素继承断链导致拖窗失效 */
.title-bar-drag-area {
  -webkit-app-region: drag;
  height: 100%;
}

/* macOS 红黄绿三圆点：浮在窗口内容上方（不参与 flex 流），与原生 macOS 行为一致。
 * Logo / 菜单栏仍从 x=0 开始排布，靠 padding-left 让出三圆点宽度。 */
.mac-traffic-lights {
  position: absolute;
  top: 0;
  left: 12px;
  height: 100%;
  display: flex;
  align-items: center;
  gap: 8px;
  -webkit-app-region: no-drag;
  pointer-events: auto;
  z-index: 10;
}
.mac-light {
  width: 12px;
  height: 12px;
  border-radius: 50%;
  border: none;
  padding: 0;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  -webkit-app-region: no-drag;
  pointer-events: auto;
  position: relative;
  transition: filter 0.1s;
}
.mac-light-close { background: #ff5f57; }
.mac-light-min   { background: #febc2e; }
.mac-light-max   { background: #28c840; }
.mac-light svg {
  position: relative;
  z-index: 1;
  opacity: 0;
  transition: opacity 0.1s;
}
.mac-light:hover svg { opacity: 1; }
.mac-light:hover { filter: brightness(1.1); }
/* 中间空白区域拖拽已由父级 drag 覆盖，无需重复声明 */
.window-controls {
  display: flex;
  height: 100%;
  -webkit-app-region: no-drag;
}
.wc-btn {
  width: 46px;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  border: none;
  color: var(--jc-text-muted);
  cursor: pointer;
  -webkit-app-region: no-drag;
  transition: background 0.15s, color 0.15s;
}
.wc-btn:hover {
  background: var(--jc-bg-hover);
  color: var(--jc-text);
}
.wc-btn.wc-close:hover {
  background: #e81123;
  color: #fff;
}
/* 置顶按钮激活态：强调色 */
.wc-btn.wc-pin-active {
  color: var(--jc-accent);
}

/* 精简模式拖拽条：默认完全透明且 pointer-events:none（不影响文字选中）；
 * 光标移到顶部 4px 区域时显示半透明条并启用 drag */
.zen-drag-bar {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  height: 28px;
  z-index: 9999;
  pointer-events: none;
  -webkit-app-region: no-drag;
  background: transparent;
  transition: background 0.15s, opacity 0.15s;
  opacity: 0;
}
.zen-drag-bar.is-visible {
  pointer-events: auto;
  -webkit-app-region: drag;
  background: var(--jc-accent);
  opacity: 0.35;
}
/* Alt 按住时全窗口拖拽遮罩 */
.zen-drag-overlay {
  position: fixed;
  inset: 0;
  z-index: 9998;
  pointer-events: auto;
  -webkit-app-region: drag;
  background: transparent;
}

/* 精简模式底部提示：居中悬浮，pointer-events:none 不干扰终端操作 */
.zen-tip {
  position: fixed;
  left: 50%;
  bottom: 24px;
  transform: translateX(-50%);
  z-index: 9999;
  padding: 8px 16px;
  border-radius: 6px;
  background: rgba(0, 0, 0, 0.75);
  color: #fff;
  font-size: 12px;
  line-height: 1.4;
  white-space: nowrap;
  pointer-events: none;
  box-shadow: 0 4px 14px rgba(0, 0, 0, 0.3);
  animation: zen-tip-in 0.2s ease;
}
@keyframes zen-tip-in {
  from { opacity: 0; transform: translate(-50%, 6px); }
  to { opacity: 1; transform: translate(-50%, 0); }
}

.panel-aside {
  transition: width 0.25s ease;
}
.dock-bottom,
.dock-center-top {
  transition: height 0.25s ease;
}
</style>

<style>
/* 拖拽分隔条调整尺寸时禁用过渡，避免滞后跟随 */
body.split-dragging .panel-aside,
body.split-dragging .dock-bottom,
body.split-dragging .dock-center-top {
  transition: none !important;
}
</style>