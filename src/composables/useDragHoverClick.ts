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
 * 拖拽悬停点击（Hover-Click）
 * 作者：JC
 * 拖拽文件过程中，悬停在带 [data-hover-click] 标记的元素（工具栏按钮 / 文件系统文件夹）
 * 上达到阈值（默认 800ms）即视为点击该元素（文件夹=进入，按钮=触发）。
 * 两种拖拽源均支持：
 * - 应用内拖拽（HTML5 DnD）：全局 dragover 监听，Workspace 挂载时 initDragHoverClick() 一次；
 * - 本地 OS 文件拖入（tauri://drag-over，无 DOM dragover）：由 Workspace 将坐标传给
 *   osDragHoverAt() 命中元素并走同一套计时逻辑。
 */
import { reactive } from 'vue'

const HOVER_DELAY_MS = 800
const PROGRESS_TICK_MS = 40

/** 悬停点击进度状态（供进度环 UI 绑定） */
export const dragHoverClickState = reactive({
  active: false,
  x: 0,
  y: 0,
  progress: 0
})

let currentEl: HTMLElement | null = null
let clickTimer: ReturnType<typeof setTimeout> | null = null
let progressTimer: ReturnType<typeof setInterval> | null = null
let started = false

function clearHover(): void {
  currentEl = null
  if (clickTimer) { clearTimeout(clickTimer); clickTimer = null }
  if (progressTimer) { clearInterval(progressTimer); progressTimer = null }
  dragHoverClickState.active = false
  dragHoverClickState.progress = 0
}

/** 核心：悬停目标变化时重启计时；命中阈值后合成点击 */
function updateHover(el: HTMLElement | null): void {
  if (el === currentEl) return
  clearHover()
  currentEl = el
  if (!el) return
  const start = Date.now()
  dragHoverClickState.active = true
  const r0 = el.getBoundingClientRect()
  dragHoverClickState.x = r0.left + r0.width / 2
  dragHoverClickState.y = r0.top + r0.height / 2
  progressTimer = setInterval(() => {
    dragHoverClickState.progress = Math.min(1, (Date.now() - start) / HOVER_DELAY_MS)
    const r = el.getBoundingClientRect()
    dragHoverClickState.x = r.left + r.width / 2
    dragHoverClickState.y = r.top + r.height / 2
  }, PROGRESS_TICK_MS)
  clickTimer = setTimeout(() => {
    const targetEl = el
    clearHover()
    targetEl.click()
  }, HOVER_DELAY_MS)
}

function onDragOver(e: DragEvent): void {
  const target = e.target as HTMLElement | null
  const el = target?.closest?.('[data-hover-click]') as HTMLElement | null
  updateHover(el ?? null)
}

function onDragEnd(): void {
  clearHover()
}

/**
 * OS 文件拖拽悬停（tauri://drag-over）：按窗口内逻辑坐标命中标记元素。
 * OS 拖拽不触发 DOM dragover，需由 Workspace 将事件坐标转发到这里。
 */
export function osDragHoverAt(x: number, y: number): void {
  const el = (document.elementFromPoint(x, y) as HTMLElement | null)?.closest?.('[data-hover-click]') as HTMLElement | null
  updateHover(el ?? null)
}

/** OS 拖拽离开窗口 / 取消时清理 */
export function osDragHoverClear(): void {
  clearHover()
}

/** 初始化全局监听（幂等） */
export function initDragHoverClick(): void {
  if (started) return
  started = true
  document.addEventListener('dragover', onDragOver, true)
  document.addEventListener('dragend', onDragEnd)
  document.addEventListener('drop', onDragEnd, true)
  document.addEventListener('dragleave', (e) => {
    // 拖拽离开窗口时清理
    if (!e.relatedTarget) clearHover()
  }, true)
}
