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
 * 面板自由拖拽 + 磁吸吸附控制器
 * 作者：JC
 * 交互：grip 手柄 mousedown → 全屏遮罩浮现 3 个磁吸区（左/右/底）→ 鼠标进入高亮 → mouseup 调 store.movePanel
 * 磁吸区尺寸与当前各面板实际大小完全重合（未占用 dock 用默认占位）。
 */
import { reactive, readonly } from 'vue'
import { useLayoutStore, type PanelId, type Dock } from '@/stores/layout'

interface DragState {
  active: boolean
  panelId: PanelId | null
  /** 当前磁吸高亮 dock */
  hoverDock: Dock | null
  /** 鼠标位置 */
  x: number
  y: number
}

const state = reactive<DragState>({
  active: false,
  panelId: null,
  hoverDock: null,
  x: 0,
  y: 0
})

interface ZoneRect { left: number; top: number; width: number; height: number }

/** 三个磁吸区矩形（像素）：与当前面板实际尺寸完全重合 */
export function getZoneRects(): Record<Dock, ZoneRect> {
  const store = useLayoutStore()
  const w = window.innerWidth
  const h = window.innerHeight
  const gap = 6
  // 左/右：当前面板宽度（折叠态 24px，与折叠按钮一致）；dock 未占用时用默认 22%
  const leftW = store.leftPanelId
    ? (store.leftCollapsed ? 24 : store.leftWidth)
    : Math.round(w * 0.22)
  const rightW = store.rightPanelId
    ? (store.rightCollapsed ? 24 : store.rightWidth)
    : Math.round(w * 0.22)
  // 底部：当前面板高度（折叠态 24px）；未占用时默认 30%
  const bottomH = store.bottomPanelId
    ? (store.bottomCollapsed ? 24 : Math.max(24, Math.round(h * (100 - store.centerTopPercent) / 100)))
    : Math.round(h * 0.3)
  return {
    left: { left: gap, top: gap, width: leftW, height: h - gap * 2 },
    right: { left: w - rightW - gap, top: gap, width: rightW, height: h - gap * 2 },
    bottom: { left: gap + leftW, top: h - bottomH - gap, width: Math.max(0, w - leftW - rightW - gap * 2), height: bottomH }
  }
}

function inRect(r: ZoneRect, x: number, y: number): boolean {
  return x >= r.left && x <= r.left + r.width && y >= r.top && y <= r.top + r.height
}

function computeHover(x: number, y: number): Dock | null {
  const rects = getZoneRects()
  // 优先底部（与左右重叠区）
  if (inRect(rects.bottom, x, y)) return 'bottom'
  if (inRect(rects.left, x, y)) return 'left'
  if (inRect(rects.right, x, y)) return 'right'
  return null
}

function onMouseMove(e: MouseEvent): void {
  if (!state.active) return
  state.x = e.clientX
  state.y = e.clientY
  state.hoverDock = computeHover(e.clientX, e.clientY)
}

function onMouseUp(): void {
  if (!state.active) return
  const store = useLayoutStore()
  if (state.panelId && state.hoverDock) {
    store.movePanel(state.panelId, state.hoverDock)
  }
  state.active = false
  state.panelId = null
  state.hoverDock = null
  document.removeEventListener('mousemove', onMouseMove)
  document.removeEventListener('mouseup', onMouseUp)
  document.body.classList.remove('panel-dragging')
}

/** 开始拖拽（grip mousedown 调用） */
export function startPanelDrag(panelId: PanelId, e: MouseEvent): void {
  e.preventDefault()
  e.stopPropagation()
  state.active = true
  state.panelId = panelId
  state.x = e.clientX
  state.y = e.clientY
  state.hoverDock = computeHover(e.clientX, e.clientY)
  document.addEventListener('mousemove', onMouseMove)
  document.addEventListener('mouseup', onMouseUp)
  document.body.classList.add('panel-dragging')
}

export function usePanelDrag() {
  return {
    state: readonly(state)
  }
}
