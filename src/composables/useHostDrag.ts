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
 * 会话树拖拽（手动实现，不依赖 HTML5 DnD）
 * 作者：JC
 *
 * 为什么不用 HTML5 drag-and-drop：Tauri/WebView2 环境下，HTML5 DnD 的
 * dropEffect 在很多情况下不被浏览器采纳，表现为光标一直显示为禁止（且
 * drop 事件不再触发）。改用 mousedown/mousemove/mouseup 手动接管，
 * 光标与目标预览完全自控，不受 WebView2 行为差异影响。
 *
 * 交互参照 jc-rag 后台管理「知识库列表」：拖拽过程中实时显示目标位置预览
 * （插入指示线 / 置入高亮）。状态需模块级共享（mousemove/mouseup 挂在
 * document 上；HostNode 是递归组件，不能依赖实例字段）。
 */
import { ref } from 'vue'
import { useHostStore } from '@/stores/host'

export type DropPosition = 'before' | 'inner' | 'after'

interface DragState {
  dragging: boolean
  draggedId: string | null
  targetId: string | null
  position: DropPosition | null
  /** 目标可放置：显示目标位置预览 */
  legal: boolean
  /** 目标非法（拖进自己的子孙会成环）：显示禁止 */
  forbidden: boolean
}

const dragState = ref<DragState>({
  dragging: false,
  draggedId: null,
  targetId: null,
  position: null,
  legal: false,
  forbidden: false
})

/** 被拖节点 id（必须在模块作用域：mousemove 在 document 监听） */
let draggedId: string | null = null
let onDropCallback: ((draggedId: string, targetId: string | null, position: DropPosition) => void) | null = null
let startX = 0
let startY = 0
let startedDrag = false
let mouseMoveHandler: ((e: MouseEvent) => void) | null = null
let mouseUpHandler: ((e: MouseEvent) => void) | null = null

/** 移动超过该像素阈值才正式视为拖拽（避免点击误触） */
const DRAG_THRESHOLD = 5

/** 文件夹三态判定：上下各 30% 为 before/after，中间 40% 为置入内部 */
const EDGE_RATIO = 0.3

/** 悬停在折叠文件夹上准备置入时的自动展开延时(ms) */
const AUTO_EXPAND_DELAY = 700

let expandTimer: ReturnType<typeof setTimeout> | null = null
let expandTargetId: string | null = null

const MAX_DEPTH = 1000

/**
 * 拖拽结束的 mouseup 会落在某节点行上，紧接着会触发 click。
 * 标记让节点的 click 处理器忽略一次，避免拖拽完成后误改选中态。
 */
let suppressNextClick = false

export function consumeSuppressClick(): boolean {
  if (suppressNextClick) {
    suppressNextClick = false
    return true
  }
  return false
}

export function useHostDrag() {
  const hostStore = useHostStore()

  function isDescendantOf(ancestorId: string, targetId: string): boolean {
    let cur = hostStore.hosts.find(h => h.id === targetId)
    let guard = 0
    while (cur && cur.parentId && guard++ < MAX_DEPTH) {
      if (cur.parentId === ancestorId) return true
      cur = hostStore.hosts.find(h => h.id === cur!.parentId)
    }
    return false
  }

  /**
   * 判定放置目标
   * - legal=true：可放置，显示目标位置预览
   * - forbidden=true：拖进自己的子孙（成环），显示禁止
   * - 两者皆 false：拖到自己身上，静默忽略（保持 move 光标，不打断拖拽）
   */
  function evaluateTarget(targetId: string): { legal: boolean; forbidden: boolean } {
    if (!draggedId) return { legal: false, forbidden: false }
    if (draggedId === targetId) return { legal: false, forbidden: false }
    if (isDescendantOf(draggedId, targetId)) return { legal: false, forbidden: true }
    return { legal: true, forbidden: false }
  }

  /**
   * 目录层级上限（最多三层）判定：算出这次放置后节点会落到哪个父级，再问 store。
   * - inner：放进目标分组里 → 父级就是目标本身
   * - before/after：与目标同级 → 父级是目标的父级
   * 超限时按「禁止」处理（红色描边 + not-allowed 光标），而不是悄悄落位；
   * 会话不占层级，任何位置都允许。
   */
  function exceedsFolderDepth(targetId: string, position: DropPosition): boolean {
    if (!draggedId) return false
    if (position === 'inner') return !hostStore.canMoveUnder(draggedId, targetId)
    const target = hostStore.hosts.find(h => h.id === targetId)
    if (!target) return false
    return !hostStore.canMoveUnder(draggedId, target.parentId)
  }

  /**
   * 计算放置位置
   * - 文件夹：上 30% → before，下 30% → after，中间 → inner
   * - 会话：上半 → before，下半 → after（会话不能作为父级）
   */
  function computePosition(el: HTMLElement, clientY: number, isFolder: boolean): DropPosition {
    const rect = el.getBoundingClientRect()
    const ratio = rect.height > 0 ? (clientY - rect.top) / rect.height : 0.5
    if (isFolder) {
      if (ratio < EDGE_RATIO) return 'before'
      if (ratio > 1 - EDGE_RATIO) return 'after'
      return 'inner'
    }
    return ratio < 0.5 ? 'before' : 'after'
  }

  function setTarget(
    targetId: string | null,
    position: DropPosition | null,
    legal: boolean,
    forbidden: boolean
  ) {
    dragState.value = { ...dragState.value, targetId, position, legal, forbidden }
  }

  function updateCursor(forbidden: boolean) {
    document.body.style.cursor = forbidden ? 'not-allowed' : 'grabbing'
  }

  function clearExpandTimer() {
    if (expandTimer) {
      clearTimeout(expandTimer)
      expandTimer = null
    }
    expandTargetId = null
  }

  /**
   * 悬停在折叠文件夹上（且准备置入内部）时延时自动展开。
   * 同一目标不重置计时：mousemove 高频触发，重置会让计时永不到期。
   */
  function scheduleExpand(targetId: string, shouldExpand: boolean, expand: () => void) {
    if (!shouldExpand) {
      clearExpandTimer()
      return
    }
    if (expandTargetId === targetId) return
    clearExpandTimer()
    expandTargetId = targetId
    expandTimer = setTimeout(() => {
      expandTimer = null
      expandTargetId = null
      expand()
    }, AUTO_EXPAND_DELAY)
  }

  function startDrag() {
    startedDrag = true
    dragState.value = {
      dragging: true,
      draggedId: draggedId,
      targetId: null,
      position: null,
      legal: false,
      forbidden: false
    }
    document.body.style.cursor = 'grabbing'
    document.body.style.userSelect = 'none'
    document.body.classList.add('host-dragging')
  }

  function cleanup() {
    if (mouseMoveHandler) document.removeEventListener('mousemove', mouseMoveHandler)
    if (mouseUpHandler) document.removeEventListener('mouseup', mouseUpHandler)
    mouseMoveHandler = null
    mouseUpHandler = null
    onDropCallback = null
    clearExpandTimer()
    draggedId = null
    startedDrag = false
    dragState.value = {
      dragging: false,
      draggedId: null,
      targetId: null,
      position: null,
      legal: false,
      forbidden: false
    }
    document.body.style.cursor = ''
    document.body.style.userSelect = ''
    document.body.classList.remove('host-dragging')
  }

  function takeDropResult(): { draggedId: string; targetId: string; position: DropPosition } | null {
    const st = dragState.value
    if (!st.draggedId || !st.targetId || !st.position || !st.legal) return null
    return { draggedId: st.draggedId, targetId: st.targetId, position: st.position }
  }

  function onMouseMove(e: MouseEvent) {
    if (!draggedId) return
    if (!startedDrag) {
      const dx = e.clientX - startX
      const dy = e.clientY - startY
      if (Math.abs(dx) < DRAG_THRESHOLD && Math.abs(dy) < DRAG_THRESHOLD) return
      startDrag()
    }

    // 找鼠标下方的节点行：依赖 HostNode 行上的 data-host-node-id 属性
    const el = document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null
    if (!el) {
      setTarget(null, null, false, false)
      updateCursor(false)
      return
    }
    const row = el.closest('[data-host-node-id]') as HTMLElement | null
    if (!row) {
      setTarget(null, null, false, false)
      updateCursor(false)
      return
    }
    const targetId = row.getAttribute('data-host-node-id')!

    // 拖到自己身上：静默忽略，不显示预览
    if (targetId === draggedId) {
      setTarget(null, null, false, false)
      updateCursor(false)
      return
    }

    const isFolder = row.getAttribute('data-host-node-type') === 'folder'
    const position = computePosition(row, e.clientY, isFolder)
    const { legal, forbidden } = evaluateTarget(targetId)
    // 成环 或 超过「目录最多三层」 → 统一按禁止处理
    const blocked = forbidden || exceedsFolderDepth(targetId, position)
    setTarget(targetId, position, legal && !blocked, blocked)
    updateCursor(blocked)

    // 延时展开折叠文件夹（通过自定义事件让 HostNode 自行更新 expanded）
    const expanded = row.getAttribute('data-host-node-expanded') === 'true'
    scheduleExpand(
      targetId,
      position === 'inner' && isFolder && !expanded,
      () => { row.dispatchEvent(new CustomEvent('host-auto-expand')) }
    )
  }

  function onMouseUp(e: MouseEvent) {
    const wasDragging = dragState.value.dragging
    if (wasDragging && draggedId) {
      suppressNextClick = true
      const result = takeDropResult()
      if (result) {
        onDropCallback?.(result.draggedId, result.targetId, result.position)
      } else if (!dragState.value.forbidden) {
        // 无有效目标（拖到空白/列表底部）：移到顶级目录末尾。
        // 「禁止」目标（成环 / 超过三层）不落位 —— 否则会被悄悄挪到根目录，行为难以预期
        onDropCallback?.(draggedId, null, 'after')
      }
    }
    cleanup()
  }

  /**
   * 节点 mousedown：记录起点，全局监听 mousemove/mouseup。
   * 移动超过阈值后正式启动拖拽；否则视为普通点击（click 仍会触发）。
   */
  function onNodeMouseDown(
    e: MouseEvent,
    node: { id: string; type: string },
    onDrop: (draggedId: string, targetId: string | null, position: DropPosition) => void
  ) {
    if (e.button !== 0) return
    // 阻止文本选择；不阻止 click 触发（click 由 mousedown+mouseup 序列独立决定）
    e.preventDefault()
    startX = e.clientX
    startY = e.clientY
    draggedId = node.id
    startedDrag = false
    onDropCallback = onDrop

    mouseMoveHandler = (ev: MouseEvent) => onMouseMove(ev)
    mouseUpHandler = (ev: MouseEvent) => onMouseUp(ev)
    document.addEventListener('mousemove', mouseMoveHandler)
    document.addEventListener('mouseup', mouseUpHandler)
  }

  return {
    dragState,
    onNodeMouseDown
  }
}