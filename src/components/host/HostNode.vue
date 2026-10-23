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
  <div class="host-node">
    <!-- 文件夹节点 -->
    <div
      v-if="node.type === 'folder'"
      ref="nodeRef"
      :data-host-node-id="node.id"
      :data-host-node-type="node.type"
      :data-host-node-expanded="expanded"
      class="relative flex items-center gap-1.5 px-2 py-1 mx-1 rounded hover:bg-[var(--jc-bg-hover)] cursor-pointer text-sm text-[var(--jc-text)] group transition-colors"
      :class="rowClass"
      :style="{ paddingLeft: indentStep * 14 + 8 + 'px', '--node-indent': indentStep * 14 + 'px' }"
      @mousedown="handleMouseDown"
      @click="onFolderClick($event)"
      @contextmenu.prevent.stop="onContextMenu($event)"
      title="分组 - 拖拽可移动，右键菜单更多操作"
    >
      <!-- 插入位置指示线 -->
      <div v-if="dropBefore" class="drop-indicator" :style="{ left: indicatorLeft + 'px' }"></div>
      <div v-if="dropAfter" class="drop-indicator drop-indicator-bottom" :style="{ left: indicatorLeft + 'px' }"></div>

      <el-icon
        size="10"
        class="transition-transform duration-200 shrink-0"
        :class="expanded ? 'rotate-90' : ''"
      >
        <ArrowRight />
      </el-icon>
      <el-icon size="14" :color="folderIconColor" class="shrink-0">
        <FolderOpened v-if="expanded" />
        <Folder v-else />
      </el-icon>
      <span class="flex-1 truncate font-medium">{{ node.name }}</span>
      <span class="text-[10px] text-[var(--jc-text-muted)] opacity-0 group-hover:opacity-100 transition-opacity shrink-0">
        {{ childCount }} 项
      </span>
      <el-icon
        size="12"
        class="cursor-pointer text-[var(--jc-text-muted)] opacity-0 group-hover:opacity-100 hover:text-blue-400 transition-opacity shrink-0"
        title="编辑"
        @click.stop="onEditClick"
      >
        <EditPen />
      </el-icon>
    </div>

    <!-- 会话节点 -->
    <div
      v-else
      ref="nodeRef"
      :data-host-node-id="node.id"
      :data-host-node-type="node.type"
      class="relative flex items-center gap-2 px-2 py-1.5 mx-1 rounded hover:bg-[var(--jc-bg-hover)] cursor-pointer text-sm text-[var(--jc-text)] group transition-colors"
      :class="[rowClass, { 'host-node--child': level > 0 }]"
      :style="{ paddingLeft: indentStep * 14 + 24 + 'px', '--node-indent': indentStep * 14 + 'px' }"
      :title="node.name + ' — ' + node.addr"
      @mousedown="onSessionMouseDown"
      @click="onSessionClick"
      @dblclick="isMobile ? undefined : onOpenNode"
      @contextmenu.prevent.stop="onSessionContextMenu"
      @touchstart.passive="onTouchStart"
      @touchmove="onTouchMove"
      @touchend="onTouchEnd"
      @touchcancel="onTouchCancel"
    >
      <!-- 插入位置指示线 -->
      <div v-if="dropBefore" class="drop-indicator" :style="{ left: indicatorLeft + 'px' }"></div>
      <div v-if="dropAfter" class="drop-indicator drop-indicator-bottom" :style="{ left: indicatorLeft + 'px' }"></div>

      <el-icon size="14" :color="isSelected ? '#3b82f6' : 'var(--jc-text-muted)'" class="shrink-0">
        <Monitor />
      </el-icon>
      <!-- 移动端：会话名与「用户名@IP」分上下两行（触屏行更高，信息更好扫读）；
           桌面端保持原单行 + 右侧地址列 -->
      <div v-if="isMobile" class="min-w-0 flex-1">
        <div class="host-node__name truncate">{{ node.name }}</div>
        <div v-if="nodeSubLine" class="host-node__sub truncate">{{ nodeSubLine }}</div>
      </div>
      <span v-else class="flex-1 truncate">{{ node.name }}</span>
      <span v-if="!isMobile" class="text-[10px] text-[var(--jc-text-muted)] truncate shrink-0">{{ formatAddr(node.addr) }}</span>
      <!-- 移动端：右侧箭头，明示「整行可点开」（桌面端双击/右键语义不同，不加） -->
      <el-icon v-if="isMobile" class="host-node__chev shrink-0" size="14">
        <ArrowRight />
      </el-icon>
      <!-- 编辑按钮：不占位，悬浮在行最右侧（hover 时显示） -->
      <el-icon
        size="12"
        class="absolute right-1.5 top-1/2 -translate-y-1/2 cursor-pointer text-[var(--jc-text-soft)] opacity-0 group-hover:opacity-100 hover:text-blue-400 transition-opacity bg-[var(--jc-bg-hover)] rounded"
        title="编辑"
        @click.stop="onEditClick"
      >
        <EditPen />
      </el-icon>
    </div>

    <!-- 递归子节点 + 空分组提示：外层 Transition 统一做高度展开/折叠动画（两端一致）
         （DOM 上多包一层 .host-node__children，仅用于动画收拢，不改变节点的数据层级） -->
    <Transition
      name="host-children"
      @enter="onChildrenEnter"
      @after-enter="onChildrenAfterEnter"
      @leave="onChildrenLeave"
      @after-leave="onChildrenAfterLeave"
    >
      <div v-if="node.type === 'folder' && expanded" class="host-node__children">
        <HostNode
          v-for="child in children"
          :key="child.id"
          :node="child"
          :level="level + 1"
          :selected-ids="selectedIds"
          @open="h => $emit('open', h)"
          @edit="h => $emit('edit', h)"
          @select="(id, ev) => $emit('select', id, ev)"
          @move="(id, tid, pos) => $emit('move', id, tid, pos)"
          @context-menu="(n, e) => $emit('context-menu', n, e)"
        />

        <!-- 空文件夹提示 -->
        <div
          v-if="children.length === 0"
          class="text-[10px] text-[var(--jc-text-muted)] italic py-1"
          :style="{ paddingLeft: (indentStep + 1) * 14 + 24 + 'px' }"
        >
          空分组
        </div>
      </div>
    </Transition>
  </div>
</template>

<script setup lang="ts">
/**
 * 会话树节点（递归）
 * 作者：JC
 * 支持拖拽排序、父子级管理、展开/折叠、选中、右键菜单。
 *
 * 拖拽实现：mousedown/mousemove/mouseup 手动接管（见 useHostDrag）。
 * 不用 HTML5 DnD 是因为 Tauri/WebView2 下 dropEffect 不可靠，会显示禁止光标。
 */
import { ref, computed, onMounted, onBeforeUnmount } from 'vue'
import { ArrowRight, FolderOpened, Folder, Monitor, EditPen } from '@element-plus/icons-vue'
import type { HostItem } from '@/stores/host'
import { useHostDrag, consumeSuppressClick, type DropPosition } from '@/composables/useHostDrag'
import { isMobileWebview } from '@/utils/platform'

const props = defineProps<{
  node: HostItem & { children?: HostItem[] }
  level: number
  /** 当前多选中的节点 id 列表（Ctrl/Shift 批量选择） */
  selectedIds?: string[]
}>()
const emit = defineEmits<{
  (e: 'open', host: HostItem): void
  (e: 'edit', node: HostItem): void
  /** 选中节点；event 用于识别 Ctrl / Shift 修饰键 */
  (e: 'select', id: string, event?: MouseEvent): void
  /**
   * 拖放完成。
   * - targetId 非空：放到该节点位置（由 position 决定 before/inner/after）
   * - targetId 为 null：拖到列表空白处，等价于「移到顶级目录末尾」
   */
  (e: 'move', draggedId: string, targetId: string | null, position: DropPosition): void
  (e: 'context-menu', node: HostItem, event: MouseEvent): void
}>()

const { dragState, onNodeMouseDown } = useHostDrag()

/** 是否运行在移动端 WebView：移动端采用「单击连接 / 长按右键」触屏交互，
 *  PC 端保持原「单击选中 / 双击连接 / 右键菜单 / 拖拽排序」不变 */
const isMobile = isMobileWebview()

const expanded = ref(true)
const nodeRef = ref<HTMLElement | null>(null)
const folderIconColor = computed(() => '#fbbf24')

const isFolder = computed(() => props.node.type === 'folder')
const isSelected = computed(() => !!props.selectedIds?.includes(props.node.id))

/** 会话保存的登录用户名：config.username 优先，缺失时从 addr 的 user@host 前缀取 */
const nodeUsername = computed(() => {
  const fromCfg = props.node.config?.username?.trim()
  if (fromCfg) return fromCfg
  const addr = props.node.addr || ''
  const at = addr.indexOf('@')
  return at > 0 ? addr.slice(0, at) : ''
})

/** 移动端会话行第二行：用户名@IP（如 root@192.168.1.10）；无用户名时只显示 IP */
const nodeSubLine = computed(() => {
  const ip = formatAddr(props.node.addr || '')
  return nodeUsername.value ? `${nodeUsername.value}@${ip}` : ip
})

// ===== 拖拽视觉状态 =====
const isDragging = computed(() => dragState.value.dragging && dragState.value.draggedId === props.node.id)
const isOver = computed(() => dragState.value.targetId === props.node.id && dragState.value.legal)
const isForbidden = computed(() => dragState.value.targetId === props.node.id && dragState.value.forbidden)
const dropBefore = computed(() => isOver.value && dragState.value.position === 'before')
const dropAfter = computed(() => isOver.value && dragState.value.position === 'after')
const dropInner = computed(() => isOver.value && dragState.value.position === 'inner')

/** 行样式：选中 / 拖拽中 / 置入内部 / 非法目标 */
const rowClass = computed(() => ({
  'is-selected': isSelected.value && !dropInner.value,
  'is-dragging': isDragging.value,
  'drop-inner': dropInner.value,
  'drop-forbidden': isForbidden.value,
  'bg-[var(--jc-bg-hover)]': isFolder.value && expanded.value && !dropInner.value
}))

/**
 * 缩进层级封顶：目录最多三层（3 级分组 + 其内的会话），
 * 第 4 层及更深的历史数据不再继续右移，避免手机窄屏下内容被挤出可视区。
 * props.level 是树的 0 基深度，故上限取 3（0/1/2 = 三个目录层，3 = 第 3 层目录内的会话）。
 */
const MAX_INDENT_STEP = 3
const indentStep = computed(() => Math.min(props.level, MAX_INDENT_STEP))

/** 指示线起点：与行内缩进对齐 */
const indicatorLeft = computed(() => indentStep.value * 14 + (isFolder.value ? 8 : 24))

const children = computed(() => props.node.children || [])
const childCount = computed(() => children.value.length)

// ===== 自动展开：监听 useHostDrag 通过 CustomEvent('host-auto-expand') 触发的延时展开 =====
function onAutoExpand() {
  expanded.value = true
}
onMounted(() => {
  nodeRef.value?.addEventListener('host-auto-expand', onAutoExpand)
})
onBeforeUnmount(() => {
  nodeRef.value?.removeEventListener('host-auto-expand', onAutoExpand)
})

// ===== 交互 =====

function toggleExpand() {
  expanded.value = !expanded.value
}

// ===== 展开 / 折叠动画（移动端与 PC 统一）=====
/**
 * 高度动画：enter 由 0 撑到内容高度，leave 反向收拢。
 *
 * 为什么用 JS 钩子而不是纯 CSS：子节点容器高度是 auto（内容行数不定），
 * CSS 无法从 0 过渡到 auto；JS 里取一次 scrollHeight 作为目标高度即可。
 * 动画结束（after-enter / after-leave）必须清掉内联 height，
 * 否则后续增删子项时容器高度会被这一次的快照值锁死。
 */
const TREE_ANIM_MS = 200

/** 起始：高 0 + 溢出裁剪，强制回流后再过渡到内容高度 */
function onChildrenEnter(el: Element): void {
  const box = el as HTMLElement
  box.style.overflow = 'hidden'
  box.style.height = '0px'
  void box.offsetHeight
  box.style.transition = `height ${TREE_ANIM_MS}ms ease`
  box.style.height = box.scrollHeight + 'px'
}

/** 展开结束：移除内联样式，恢复自适应高度 */
function onChildrenAfterEnter(el: Element): void {
  const box = el as HTMLElement
  box.style.transition = ''
  box.style.height = ''
  box.style.overflow = ''
}

/** 起始：先锁定当前内容高度，下一帧再过渡到 0 */
function onChildrenLeave(el: Element): void {
  const box = el as HTMLElement
  box.style.overflow = 'hidden'
  box.style.height = box.scrollHeight + 'px'
  void box.offsetHeight
  box.style.transition = `height ${TREE_ANIM_MS}ms ease`
  box.style.height = '0px'
}

/** 折叠结束：清除内联样式，避免复用 DOM 时残留 */
function onChildrenAfterLeave(el: Element): void {
  const box = el as HTMLElement
  box.style.transition = ''
  box.style.height = ''
  box.style.overflow = ''
}

function onFolderClick(e: MouseEvent) {
  // 拖拽刚结束时 mouseup 落在某节点行上会顺带触发 click，借 suppressClick 屏蔽
  if (consumeSuppressClick()) return
  // 按住 Ctrl/Shift 时是「批量选择」语义，不展开/折叠（与文件管理器一致）
  const multi = e.ctrlKey || e.metaKey || e.shiftKey
  if (!multi) toggleExpand()
  emit('select', props.node.id, e)
}

function onSelectNode(e: MouseEvent) {
  if (consumeSuppressClick()) return
  emit('select', props.node.id, e)
}

/** 双击判定窗口（ms）：同一节点两次点击间隔小于该值即视为双击 */
const DOUBLE_CLICK_MS = 400
/** 连接去重窗口（ms）：同一节点在该时间内只发起一次连接 */
const OPEN_DEDUP_MS = 400
let lastClickId = ''
let lastClickAt = 0
let lastOpenId = ''
let lastOpenAt = 0

/** 发起连接的统一出口。双击可能同时命中「单击计时判定」与浏览器 dblclick 事件，
 *  没有去重会开出两个标签；有了去重，两条路径并存也不会重复连接。 */
function emitOpen() {
  const now = Date.now()
  if (lastOpenId === props.node.id && now - lastOpenAt < OPEN_DEDUP_MS) return
  lastOpenId = props.node.id
  lastOpenAt = now
  emit('open', props.node)
}

function onOpenNode() {
  if (consumeSuppressClick()) return
  emitOpen()
}

function onEditClick() {
  // 编辑图标 click 由 @click.stop 阻止冒泡到行 click，无需消费 suppress
  emit('edit', props.node)
}

function onContextMenu(e: MouseEvent) {
  emit('context-menu', props.node, e)
}

// ===== 移动端触屏交互：单击连接、长按右键菜单（PC 端不进入此分支）=====
/** 长按阈值（ms）：超过即视为右键而非点击 */
const LONG_PRESS_MS = 450
/** 移动容差（px）：手指滑动超过则判定为滚动，取消长按 */
const MOVE_TOLERANCE = 10
const suppressTap = ref(false)
let pressTimer: ReturnType<typeof setTimeout> | null = null
let touchStartX = 0
let touchStartY = 0

function clearPressTimer() {
  if (pressTimer !== null) {
    clearTimeout(pressTimer)
    pressTimer = null
  }
}

/** 移动端 mousedown：不触发拖拽排序（避免合成事件干扰滚动/点击），交由 touch 事件处理 */
function onSessionMouseDown(e: MouseEvent) {
  if (!isMobile) handleMouseDown(e)
}

/** 移动端单击 = 连接进入终端；PC 端单击 = 选中、双击 = 连接。
 *  PC 的双击除了浏览器 dblclick 事件，这里再用「同节点 400ms 内二次点击」兜底判定 ——
 *  拖拽接管（mousedown 上 preventDefault + document 级 mouseup 清理）等情形下
 *  dblclick 事件有可能被吞掉，两条路径都走 emitOpen（内部去重），不会漏连也不会开两个标签。 */
function onSessionClick(e?: MouseEvent) {
  if (!isMobile) {
    if (consumeSuppressClick()) return
    const now = Date.now()
    const isDouble = !!e && lastClickId === props.node.id && now - lastClickAt <= DOUBLE_CLICK_MS
    lastClickId = props.node.id
    lastClickAt = now
    if (isDouble) { emitOpen(); return }
    onSelectNode(e as MouseEvent)
    return
  }
  if (suppressTap.value) {
    suppressTap.value = false
    return
  }
  onOpenNode()
}

/** 移动端：原生 contextmenu 由长按触发，PC 端才走原生右键 */
function onSessionContextMenu(e: MouseEvent) {
  if (isMobile) return
  onContextMenu(e)
}

/** 触摸开始：启动长按计时器 */
function onTouchStart(e: TouchEvent) {
  if (!isMobile) return
  const t = e.touches[0]
  if (!t) return
  touchStartX = t.clientX
  touchStartY = t.clientY
  clearPressTimer()
  pressTimer = setTimeout(() => {
    pressTimer = null
    suppressTap.value = true
    // 用长按点坐标触发右键菜单（父级 openContextMenu 仅取 clientX/clientY）
    onContextMenu({ clientX: t.clientX, clientY: t.clientY } as MouseEvent)
  }, LONG_PRESS_MS)
}

/** 触摸移动：判定为滚动则取消长按 */
function onTouchMove(e: TouchEvent) {
  if (pressTimer === null) return
  const t = e.touches[0]
  if (!t) return
  if (Math.abs(t.clientX - touchStartX) > MOVE_TOLERANCE || Math.abs(t.clientY - touchStartY) > MOVE_TOLERANCE) {
    clearPressTimer()
  }
}

function onTouchEnd() {
  clearPressTimer()
}

function onTouchCancel() {
  clearPressTimer()
}

function formatAddr(addr: string): string {
  const parts = addr.split('@')
  if (parts.length >= 2) return parts[1].split(':')[0]
  return addr
}

// ===== 拖拽入口：mousedown → useHostDrag 全局接管 =====

function handleMouseDown(e: MouseEvent) {
  onNodeMouseDown(
    e,
    { id: props.node.id, type: props.node.type },
    (draggedId, targetId, position) => {
      emit('move', draggedId, targetId, position)
    }
  )
}
</script>

<style scoped>
/* 插入位置指示线 */
.drop-indicator {
  position: absolute;
  top: 0;
  right: 4px;
  height: 2px;
  background-color: #3b82f6;
  border-radius: 1px;
  box-shadow: 0 0 4px rgba(59, 130, 246, 0.9);
  pointer-events: none;
  z-index: 5;
}
.drop-indicator::before {
  content: '';
  position: absolute;
  left: -3px;
  top: -3px;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background-color: #3b82f6;
}
.drop-indicator-bottom {
  top: auto;
  bottom: 0;
}

/* 置入文件夹内部：整行高亮 + 内描边 */
.drop-inner {
  background-color: rgba(59, 130, 246, 0.2) !important;
  box-shadow: inset 0 0 0 1px #3b82f6;
}
/* 选中态 */
.is-selected {
  background-color: rgba(59, 130, 246, 0.16) !important;
}
/* 拖拽中的源节点 */
.is-dragging {
  opacity: 0.4;
}
/* 非法放置目标（拖进自己的子孙会成环） */
.drop-forbidden {
  box-shadow: inset 0 0 0 1px #ef4444;
}
</style>

<style>
/* 拖拽进行中：覆盖节点行的 cursor-pointer，确保 grabbing/ not-allowed 可见 */
body.host-dragging [data-host-node-id] {
  cursor: grabbing !important;
}
body.host-dragging [data-host-node-id].drop-forbidden {
  cursor: not-allowed !important;
}
</style>