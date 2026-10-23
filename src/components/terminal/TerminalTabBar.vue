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
  <div ref="barRef" class="terminal-tabbar relative isolate z-30 flex items-center h-8 bg-[var(--jc-bg-soft)] border-b border-[var(--jc-border)] select-none">
    <!-- 吸入光效（左）：首个 Tab 滚出可视区时出现，发光方向向右；透明悬浮不占位、不遮挡文字 -->
    <div v-if="firstHidden" class="suck-light suck-light--left" aria-hidden="true">
      <div class="suck-light__glow"></div>
    </div>

    <!-- Tab 项 -->
    <div ref="scrollRef" class="tab-scroll flex items-center h-full overflow-x-auto scrollbar-hide" @scroll="updateAddPosition">
      <div
        v-for="(tab, index) in tabs"
        :key="tab.id"
        class="tab-item relative flex items-center gap-1.5 h-full px-3 text-xs cursor-pointer border-r border-[var(--jc-border)] shrink-0"
        :class="tab.id === activeTabId ? 'tab-item--active' : 'tab-item--idle'"
        @mousedown="onTabMouseDown($event, tab)"
        @contextmenu.prevent.stop="openTabMenu($event, tab)"
        @dblclick.stop
      >
        <!-- 插入指示器（拖拽排序时） -->
        <div
          v-if="drag.dragging && drag.insertIndex === index"
          class="absolute left-0 top-1 bottom-1 w-0.5 bg-blue-500 rounded z-10"
        ></div>
        <el-icon :size="11" :color="tab.connected ? 'var(--jc-success)' : 'var(--jc-text-muted)'">
          <CircleCheckFilled v-if="tab.connected" />
          <CircleCloseFilled v-else />
        </el-icon>
        <span class="max-w-[160px] truncate">{{ tab.title }}</span>
        <el-icon
          size="12"
          class="tab-close text-[var(--jc-text-muted)] hover:text-[var(--jc-text)] hover:bg-[var(--jc-bg-hover)] rounded p-0.5"
          @mousedown.stop
          @click.stop="emit('close', tab.id)"
        >
          <Close />
        </el-icon>
      </div>
      <!-- 末尾插入指示器 -->
      <div
        v-if="drag.dragging && drag.insertIndex >= tabs.length"
        class="w-0.5 self-stretch my-1 bg-blue-500 rounded shrink-0"
      ></div>

      <!-- 新建按钮（跟随态）：紧贴最后一个 Tab，仅当末位 Tab 完整可见时渲染于此 -->
      <button
        v-if="!addPinned && CAP.localTerminal()"
        class="tab-add shrink-0 flex items-center justify-center w-7 h-6 mx-1 rounded text-[var(--jc-text-soft)] hover:text-[var(--jc-text)] hover:bg-[var(--jc-bg-hover)] transition-colors"
        title="新建本地终端"
        @click="emit('add')"
      >
        <el-icon size="14"><Plus /></el-icon>
      </button>
    </div>

    <!-- 吸入光效（右）：透明悬浮在滚动区右缘，不占位、不遮挡文字 -->
    <div v-if="addPinned" class="suck-light" aria-hidden="true">
      <div class="suck-light__glow"></div>
    </div>

    <!-- 新建按钮（固定态）：末位 Tab 被挤出可视区时，固定在滚动区右侧、紧贴分屏按钮组左边 -->
    <button
      v-if="addPinned && CAP.localTerminal()"
      class="tab-add shrink-0 flex items-center justify-center w-7 h-6 mx-0.5 rounded text-[var(--jc-text-soft)] hover:text-[var(--jc-text)] hover:bg-[var(--jc-bg-hover)] transition-colors"
      title="新建本地终端"
      @click="emit('add')"
    >
      <el-icon size="14"><Plus /></el-icon>
    </button>

    <!-- 分屏工具按钮（右侧固定）：点击已激活的按钮即退出分屏。
         移动端整组隐藏（见 mobile.css）：窄屏分屏不可用，且会挤占 Tab 空间 -->
    <div class="split-tools ml-auto flex items-center shrink-0 pr-1 gap-0.5">
      <button
        v-for="btn in splitButtons"
        :key="btn.mode"
        class="flex items-center justify-center w-7 h-6 rounded transition-colors"
        :class="splitMode === btn.mode ? 'text-[var(--jc-accent)] bg-[var(--jc-accent)]/15' : 'text-[var(--jc-text-soft)] hover:text-[var(--jc-text)] hover:bg-[var(--jc-bg-hover)]'"
        :title="btn.tooltip"
        @click="onSplitClick(btn.mode)"
      >
        <el-icon size="14"><component :is="btn.icon" /></el-icon>
      </button>
    </div>

    <!-- Tab 右键菜单 -->
    <div
      v-if="menu.visible"
      ref="menuRef"
      class="fixed z-[1000] min-w-[150px] py-1 rounded-md bg-[var(--jc-bg-hover)] border border-[var(--jc-border)] shadow-xl text-xs"
      :style="{ left: menu.x + 'px', top: menu.y + 'px' }"
      @click.stop
      @contextmenu.prevent.stop
    >
      <template v-for="(item, i) in menuItems" :key="i">
        <div v-if="item.divider" class="my-1 border-t border-[var(--jc-border)]"></div>
        <div v-else class="tab-menu__item" :class="{ 'tab-menu__item--disabled': item.disabled }" @click="onMenuAction(item.key)">
          {{ item.label }}
        </div>
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 终端 Tab 栏
 * 作者：JC
 * - 「+」新建按钮动态定位：默认紧贴最后一个 Tab；当 Tab 溢出且末位 Tab 被挤出可视区时，
 *   固定到滚动区右侧（紧贴分屏按钮组左边）保底可见；滚回末尾或 Tab 减少不再溢出时恢复跟随
 * - 自定义 mousedown 拖拽：栏内排序（reorder）；拖出窗口外 → detach（跨窗口迁移/新开窗口）
 * - 右侧分屏工具按钮：左右/上下/网格/退出
 */
import { ref, reactive, computed, h, defineComponent, watch, nextTick, onMounted, onBeforeUnmount } from 'vue'
import { Plus, Close, CircleCheckFilled, CircleCloseFilled } from '@element-plus/icons-vue'
import type { TerminalTab, SplitMode } from '@/stores/terminal'
import { useSettingsStore } from '@/stores/settings'
import { invoke } from '@/api'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { setUiFrozen } from '@/utils/uiFreeze'
import { CAP } from '@/utils/platform'

const props = defineProps<{
  tabs: TerminalTab[]
  activeTabId: string
  splitMode: SplitMode
}>()

const emit = defineEmits<{
  (e: 'select', id: string): void
  (e: 'close', id: string): void
  (e: 'add'): void
  (e: 'reorder', id: string, toIndex: number): void
  /** 拖出窗口：由工作区处理跨窗口迁移/新开窗口 */
  (e: 'detach', tab: TerminalTab, clientX: number, clientY: number): void
  (e: 'splitMode', mode: SplitMode): void
  /** 右键菜单动作：重连/复制会话/复制地址/关闭左侧/关闭右侧/关闭其他/全部关闭 */
  (e: 'tabMenuAction', action: TabMenuAction, tabId: string): void
}>()

/** 全局设置（鼠标中键关闭标签页开关等） */
const prefs = useSettingsStore()

type TabMenuAction = 'reconnect' | 'duplicate' | 'copyAddress' | 'close' | 'closeLeft' | 'closeRight' | 'closeOthers' | 'closeAll'

// ===== 「+」按钮动态定位（跟随态 / 固定态） =====
const scrollRef = ref<HTMLElement | null>(null)
/** true = 末位 Tab 不可见，「+」固定在滚动区右侧（分屏按钮左边） */
const addPinned = ref(false)
/** true = 首位 Tab 滚出可视区，左侧显示吸入光效 */
const firstHidden = ref(false)

function updateAddPosition() {
  const el = scrollRef.value
  if (!el) { addPinned.value = false; firstHidden.value = false; return }
  firstHidden.value = el.scrollLeft > 2
  const overflow = el.scrollWidth > el.clientWidth
  const atEnd = el.scrollLeft + el.clientWidth >= el.scrollWidth - 2
  const shouldPin = overflow && !atEnd
  if (shouldPin === addPinned.value) return
  addPinned.value = shouldPin
  // 从固定态恢复跟随且处于末尾：滚到最右，保证内联「+」完整可见
  if (!shouldPin && atEnd) {
    nextTick(() => {
      const sc = scrollRef.value
      if (sc) sc.scrollLeft = sc.scrollWidth
    })
  }
}

let scrollAreaObserver: ResizeObserver | null = null
onMounted(async () => {
  scrollAreaObserver = new ResizeObserver(updateAddPosition)
  if (scrollRef.value) scrollAreaObserver.observe(scrollRef.value)
  updateAddPosition()
  // 常驻监听跨窗口拖拽事件：本窗口既可能是拖拽源，也可能是被拖回的吸附目标。
  // 必须在挂载时注册 —— 若只在拖拽开始时才注册，就只有源窗口能收到事件，
  // 拖回其它窗口（如从独立窗口拖回主窗口）时目标窗口收不到 move，也就没有吸附。
  try {
    myLabel = getCurrentWindow().label
    ghostBeginOff = await listen('tab-ghost:begin', () => { void measureGhostGeom() })
    ghostMoveOff = await listen<{ x: number; y: number }>('tab-ghost:move', ev => onGhostMove(ev.payload))
    ghostDropOff = await listen<{ x: number; y: number; sourceLabel: string }>('tab-ghost:drop', ev => onGhostDrop(ev.payload))
  } catch { /* 非 Tauri 环境或权限异常：忽略 */ }
})
onBeforeUnmount(() => {
  scrollAreaObserver?.disconnect()
  scrollAreaObserver = null
  ghostBeginOff?.(); ghostBeginOff = null
  ghostMoveOff?.(); ghostMoveOff = null
  ghostDropOff?.(); ghostDropOff = null
  stopGhostDrag()
})

// Tab 增减：重算「+」位置；新增时自动滚到末尾让新 Tab 可见
let prevTabCount = 0
watch(() => props.tabs.length, (len) => {
  const added = len > prevTabCount
  prevTabCount = len
  nextTick(() => {
    updateAddPosition()
    if (added && scrollRef.value) scrollRef.value.scrollLeft = scrollRef.value.scrollWidth
  })
})

// ===== 分屏按钮（统一线性图标风格） =====
const svgAttrs = {
  viewBox: '0 0 14 14', width: 14, height: 14,
  fill: 'none', stroke: 'currentColor',
  'stroke-width': '1.2', 'stroke-linecap': 'round', 'stroke-linejoin': 'round'
}
/** 左右分屏 */
const IconSplitH = defineComponent({
  render: () => h('svg', svgAttrs, [
    h('rect', { x: 1, y: 2, width: 12, height: 10, rx: 1 }),
    h('line', { x1: 7, y1: 2, x2: 7, y2: 12 })
  ])
})
/** 上下分屏 */
const IconSplitV = defineComponent({
  render: () => h('svg', svgAttrs, [
    h('rect', { x: 1, y: 2, width: 12, height: 10, rx: 1 }),
    h('line', { x1: 1, y1: 7, x2: 13, y2: 7 })
  ])
})
/** 网格分屏 */
const IconSplitGrid = defineComponent({
  render: () => h('svg', svgAttrs, [
    h('rect', { x: 1, y: 2, width: 12, height: 10, rx: 1 }),
    h('line', { x1: 7, y1: 2, x2: 7, y2: 12 }),
    h('line', { x1: 1, y1: 7, x2: 13, y2: 7 })
  ])
})
/** 分屏按钮：点击已激活的按钮即退出分屏（不再单独提供退出按钮） */
const splitButtons = computed(() => [
  { mode: 'h' as SplitMode, icon: IconSplitH, tooltip: '左右分屏（再次点击退出）' },
  { mode: 'v' as SplitMode, icon: IconSplitV, tooltip: '上下分屏（再次点击退出）' },
  { mode: 'grid' as SplitMode, icon: IconSplitGrid, tooltip: '网格分屏（再次点击退出）' }
])

function onSplitClick(mode: SplitMode) {
  emit('splitMode', mode === props.splitMode ? 'none' : mode)
}

// ===== 右键菜单 =====
const menu = reactive({ visible: false, x: 0, y: 0, tabId: '' })
const menuRef = ref<HTMLElement | null>(null)
const menuTab = computed(() => props.tabs.find(t => t.id === menu.tabId) ?? null)

interface MenuItem { key: TabMenuAction; label: string; disabled?: boolean; divider?: never }
interface MenuDivider { key?: never; label?: never; disabled?: never; divider: true }

const menuItems = computed<(MenuItem | MenuDivider)[]>(() => {
  const tab = menuTab.value
  const idx = tab ? props.tabs.findIndex(t => t.id === tab.id) : -1
  const n = props.tabs.length
  return [
    { key: 'reconnect', label: '重新连接', disabled: !tab || tab.connected },
    { key: 'duplicate', label: '复制会话' },
    { key: 'copyAddress', label: '复制地址', disabled: !tab?.host },
    { divider: true },
    { key: 'close', label: '关闭标签页' },
    { key: 'closeLeft', label: '关闭左侧', disabled: idx <= 0 },
    { key: 'closeRight', label: '关闭右侧', disabled: idx < 0 || idx >= n - 1 },
    { key: 'closeOthers', label: '关闭其他', disabled: n <= 1 },
    { key: 'closeAll', label: '全部关闭', disabled: n === 0 }
  ]
})

/** 全局 mousedown：仅当点击落在菜单外部时关闭；
 *  不能用 once+capture 直接关——否则菜单 DOM 会在 click 派发前被移除，菜单项 @click 永远不触发 */
function onDocMenuMouseDown(e: MouseEvent) {
  if (menuRef.value && e.target instanceof Node && menuRef.value.contains(e.target)) return
  closeTabMenu()
}

function openTabMenu(e: MouseEvent, tab: TerminalTab) {
  emit('select', tab.id)
  menu.tabId = tab.id
  menu.visible = true
  // 靠边钳制，避免菜单溢出视口
  menu.x = Math.min(e.clientX, window.innerWidth - 170)
  menu.y = Math.min(e.clientY, window.innerHeight - 260)
  nextTick(() => {
    document.addEventListener('mousedown', onDocMenuMouseDown, true)
  })
}

function closeTabMenu() {
  menu.visible = false
  menu.tabId = ''
  document.removeEventListener('mousedown', onDocMenuMouseDown, true)
}

function onMenuAction(key: TabMenuAction) {
  const tabId = menu.tabId
  closeTabMenu()
  if (!tabId) return
  if (key === 'close') emit('close', tabId)
  else emit('tabMenuAction', key, tabId)
}

// ===== 拖拽（mousedown 自定义实现） =====
const barRef = ref<HTMLElement | null>(null)

interface DragState {
  dragging: boolean
  tabId: string
  title: string
  startX: number
  startY: number
  x: number
  y: number
  insertIndex: number
  tab: TerminalTab | null
}

const drag = reactive<DragState>({
  dragging: false, tabId: '', title: '', startX: 0, startY: 0, x: 0, y: 0, insertIndex: 0, tab: null
})

const DRAG_THRESHOLD = 6
/** 本次拖拽是否由本窗口冻结了后台任务（仅本窗口自己负责解冻，避免误清别人的冻结态） */
let frozenByTabDrag = false
/** 冻结安全阀定时器 */
let dragFreezeTimer: number | null = null
/** 本窗口 label（判断自己是否为拖拽源 / 吸附宿主） */
let myLabel = ''
/** 拖拽事件订阅注销句柄（常驻，挂载时注册） */
let ghostBeginOff: UnlistenFn | null = null
let ghostMoveOff: UnlistenFn | null = null
let ghostDropOff: UnlistenFn | null = null
/** 本窗口几何 + Tab 栏/标签位置快照（每次拖拽开始时测量一次，全程复用） */
let ghostGeom: {
  originX: number
  originY: number
  scale: number
  barRect: DOMRect | null
  itemRects: DOMRect[]
} | null = null

/** 结束拖拽：让 Rust 跟随循环退出并隐藏浮层（监听为常驻，不注销） */
function stopGhostDrag() {
  invoke('tab_ghost_cancel').catch(() => {})
}

/**
 * 测量本窗口几何与 Tab 栏/标签位置。
 * 每次拖拽开始（tab-ghost:begin 广播）时各窗口各测一次并全程复用：
 * 拖拽中布局不变，避免每 40ms 触发 getBoundingClientRect 的强制重排。
 */
async function measureGhostGeom() {
  try {
    const win = getCurrentWindow()
    const pos = await win.outerPosition()
    const scale = await win.scaleFactor()
    const barEl = barRef.value
    ghostGeom = {
      originX: pos.x,
      originY: pos.y,
      scale,
      barRect: barEl?.getBoundingClientRect() ?? null,
      itemRects: barEl
        ? Array.from(barEl.querySelectorAll<HTMLElement>('.tab-item')).map(el => el.getBoundingClientRect())
        : []
    }
  } catch { ghostGeom = null }
}

/** 拖拽移动（全局广播）：落点命中本窗口 Tab 栏 → 吸附到插入位置并显示插入指示器 */
function onGhostMove(p: { x: number; y: number }) {
  const g = ghostGeom
  if (!g?.barRect) return
  const cx = (p.x - g.originX) / g.scale
  const cy = (p.y - g.originY) / g.scale
  drag.x = cx
  drag.y = cy
  drag.insertIndex = computeInsertIndex(cx, g.itemRects)
  const inBar = cx >= g.barRect.left && cx <= g.barRect.right &&
    cy >= g.barRect.top - 20 && cy <= g.barRect.bottom + 20
  if (!inBar) {
    drag.dragging = false
    invoke('tab_ghost_unsnap', { owner: myLabel }).catch(() => {})
    return
  }
  // 本窗口成为吸附目标：显示插入指示器 + 浮层贴合插入位置
  drag.dragging = true
  const { itemRects, barRect } = g
  let snapClientX: number
  if (drag.insertIndex < itemRects.length) {
    snapClientX = itemRects[drag.insertIndex].left
  } else {
    snapClientX = itemRects.length ? itemRects[itemRects.length - 1].right : barRect.left
  }
  invoke('tab_ghost_snap', {
    x: Math.round(g.originX + snapClientX * g.scale),
    y: Math.round(g.originY + barRect.top * g.scale),
    owner: myLabel
  }).catch(() => {})
}

/** 松手（全局广播）：各窗口清理本地状态；只有源窗口执行落点判定 */
function onGhostDrop(p: { x: number; y: number; sourceLabel: string }) {
  const isSource = p.sourceLabel === myLabel
  const g = ghostGeom
  const cx = g ? (p.x - g.originX) / g.scale : 0
  const cy = g ? (p.y - g.originY) / g.scale : 0
  const tabSnapshot = drag.tab
  const insertIndex = drag.insertIndex
  resetDrag()
  ghostGeom = null
  // 仅源窗口终止 Rust 跟随循环，其余窗口只做本地清理。
  // 注意：不 await —— 跟随线程本就会在检测到松手后自行退出；
  // await 反而会让 emit('detach') 推迟到 IPC 应答回来，曾因主线程/运行时竞态导致落点判定不触发。
  if (isSource) invoke('tab_ghost_cancel').catch(() => {})
  if (!isSource || !tabSnapshot || !g?.barRect) return
  const inBar = cx >= g.barRect.left && cx <= g.barRect.right &&
    cy >= g.barRect.top - 20 && cy <= g.barRect.bottom + 20
  if (inBar) emit('reorder', tabSnapshot.id, insertIndex)
  else emit('detach', tabSnapshot, cx, cy)
}

/** 读取当前主题实际生效的颜色（hex → rgb 分量），用于让浮层与界面同风格 */
function cssColorRgb(name: string, fallback: [number, number, number]): [number, number, number] {
  try {
    const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim()
    if (!v.startsWith('#')) return fallback
    let hex = v.slice(1)
    if (hex.length === 3) hex = hex.split('').map(c => c + c).join('')
    const n = parseInt(hex, 16)
    if (Number.isNaN(n)) return fallback
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255]
  } catch { return fallback }
}

/** 估算标题像素宽度，让浮层宽度贴近真实标签（图标 11 + 间距 6 + 左右内边距 24 + 余量 8） */
function estimateTitleWidth(title: string): number {
  try {
    const cvs = document.createElement('canvas')
    const ctx = cvs.getContext('2d')
    if (!ctx) return 160
    ctx.font = '12px system-ui, -apple-system, "Segoe UI", sans-serif'
    return Math.min(Math.max(Math.round(ctx.measureText(title).width) + 49, 90), 320)
  } catch { return 160 }
}

/**
 * 开始拖拽：把起始位置/尺寸/主题交给 Rust，由其启动独立跟随循环（8ms/轮 ≈ 125Hz，零 IPC 往返）。
 * 后续的跟随、吸附（onGhostMove）、落点判定（onGhostDrop）都由常驻监听处理。
 */
async function beginDrag(tab: TerminalTab, startClientX: number, startClientY: number) {
  let originX = 0
  let originY = 0
  let scale = 1
  try {
    const win = getCurrentWindow()
    const pos = await win.outerPosition()
    originX = pos.x
    originY = pos.y
    scale = await win.scaleFactor()
  } catch { /* 取窗口几何失败按 scale=1 估算 */ }

  await invoke('tab_ghost_begin', {
    title: tab.title,
    sourceLabel: myLabel,
    x: Math.round(originX + startClientX * scale),
    y: Math.round(originY + startClientY * scale),
    width: estimateTitleWidth(tab.title),
    // 主题色：浮层是独立 webview，必须显式同步源窗口的配色，否则会显示默认浅色
    theme: {
      bg: cssColorRgb('--jc-bg', [30, 30, 30]),
      text: cssColorRgb('--jc-text', [229, 231, 235]),
      accent: cssColorRgb('--jc-accent', [96, 165, 250]),
      success: cssColorRgb('--jc-success', [52, 211, 153])
    }
  }).catch(() => {})
}

function onTabMouseDown(e: MouseEvent, tab: TerminalTab) {
  // 鼠标中键（滚轮）关闭标签页：受「偏好设置-鼠标中键关闭标签页」开关控制
  if (e.button === 1) {
    if (prefs.settings.middleCloseTab) {
      e.preventDefault()
      emit('close', tab.id)
    }
    return
  }
  if (e.button !== 0) return
  // 单击即选中
  emit('select', tab.id)
  const startX = e.clientX
  const startY = e.clientY
  let started = false

  const onMove = (ev: MouseEvent) => {
    if (started) return // 已进入拖拽 → 由全局轮询接管
    if (Math.abs(ev.clientX - startX) + Math.abs(ev.clientY - startY) < DRAG_THRESHOLD) return
    started = true
    drag.dragging = true
    drag.tabId = tab.id
    drag.title = tab.title
    drag.tab = tab
    drag.x = ev.clientX
    drag.y = ev.clientY
    drag.insertIndex = computeInsertIndex(ev.clientX)
    freezeDuringDrag()
    beginDrag(tab, ev.clientX, ev.clientY)
  }

  const onUp = () => {
    document.removeEventListener('mousemove', onMove)
    document.removeEventListener('mouseup', onUp)
    // 未达阈值 = 单击（select 已派发）；拖拽中的松手由轮询检测（鼠标可能已出窗口）
  }

  document.addEventListener('mousemove', onMove)
  document.addEventListener('mouseup', onUp)
}

/**
 * 按 Tab 元素中点计算插入位置
 * @param cachedRects 拖拽期间可传入预先测好的矩形，避免逐次 getBoundingClientRect 触发强制重排
 */
function computeInsertIndex(clientX: number, cachedRects?: DOMRect[]): number {
  const list = cachedRects ??
    Array.from(barRef.value?.querySelectorAll<HTMLElement>('.tab-item') ?? []).map(el => el.getBoundingClientRect())
  let index = props.tabs.length
  for (let i = 0; i < list.length; i++) {
    const r = list[i]
    if (clientX < r.left + r.width / 2) { index = i; break }
  }
  return index
}

/**
 * 拖拽期间冻结本窗口的周期性后台任务（目录轮询 / SSH cwd 同步 / 指标采样等）：
 * 这些任务每次都要走 IPC 或 SSH 往返并触发重渲染，会和浮层跟随循环抢主线程。
 * 带超时安全阀：松手事件由 Rust 跟随循环广播，若该循环异常退出则不会再收到事件，
 * 超时兜底解冻，避免后台任务被永久冻结。
 */
function freezeDuringDrag() {
  setUiFrozen(true)
  frozenByTabDrag = true
  if (dragFreezeTimer !== null) window.clearTimeout(dragFreezeTimer)
  dragFreezeTimer = window.setTimeout(() => resetDrag(), 30000)
}

function resetDrag() {
  if (dragFreezeTimer !== null) { window.clearTimeout(dragFreezeTimer); dragFreezeTimer = null }
  if (frozenByTabDrag) {
    frozenByTabDrag = false
    setUiFrozen(false)
  }
  drag.dragging = false
  drag.tabId = ''
  drag.title = ''
  drag.tab = null
  drag.insertIndex = 0
}
</script>

<style scoped>
.scrollbar-hide::-webkit-scrollbar { display: none; }
.scrollbar-hide { -ms-overflow-style: none; scrollbar-width: none; }

.tab-item--idle {
  background-color: var(--jc-bg-soft);
  color: var(--jc-text-muted);
}
.tab-item--idle:hover {
  color: var(--jc-text);
}
.tab-item--active {
  background-color: var(--jc-bg);
  color: var(--jc-text);
  box-shadow: inset 0 -2px 0 0 var(--jc-accent);
}
.tab-close {
  font-size: 12px;
}

/* ===== Tab 右键菜单 ===== */
.tab-menu__item {
  padding: 6px 14px;
  color: var(--jc-text);
  cursor: pointer;
  white-space: nowrap;
}
.tab-menu__item:hover {
  background-color: var(--jc-border);
  color: var(--jc-text);
}
.tab-menu__item--disabled {
  color: var(--jc-text-muted);
  cursor: default;
  pointer-events: none;
}
</style>
