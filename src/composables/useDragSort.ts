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
 * 指针拖拽排序控制器
 * 作者：JC
 * 背景：vuedraggable 4.x 与 Vue 3.5 不兼容；HTML5 DnD 在 Tauri WebView（Windows）
 * 下被文件拖放处理劫持，两者均无法启动拖拽。
 * 方案：mousedown/mousemove 指针拖拽（与应用内面板磁吸拖拽同一机制，已验证可靠）。
 *
 * 用法：
 * - 排序项容器需有 data-drag-container 属性；
 * - 每个排序项需有 data-drag-index 属性（绑定当前索引，v-for 响应式更新）；
 * - 在拖拽起点（整项或手柄）绑定 @mousedown="startDrag($event, index)"；
 * - 可用 dragIndex 渲染拖拽中的视觉反馈。
 */
import { ref } from 'vue'

/** 位移超过该阈值才真正开始拖拽（避免影响普通点击） */
const START_THRESHOLD = 4

export function useDragSort(commit: (from: number, to: number) => void) {
  /** 正在拖拽的项索引（-1 表示无） */
  const dragIndex = ref(-1)

  function startDrag(e: MouseEvent, index: number): void {
    if (e.button !== 0) return
    e.preventDefault()
    const startX = e.clientX
    const startY = e.clientY
    let active = false
    const container = (e.currentTarget as HTMLElement | null)?.closest('[data-drag-container]')
      ?? document.querySelector('[data-drag-container]')

    function onMove(ev: MouseEvent): void {
      if (!active) {
        if (Math.abs(ev.clientX - startX) + Math.abs(ev.clientY - startY) < START_THRESHOLD) return
        active = true
        dragIndex.value = index
        document.body.classList.add('drag-sorting')
      }
      if (!container) return
      // 指针下的排序项（索引随换位响应式更新）
      const target = (document.elementFromPoint(ev.clientX, ev.clientY) as HTMLElement | null)
        ?.closest('[data-drag-index]') as HTMLElement | null
      if (!target || !container.contains(target)) return
      const to = parseInt(target.dataset.dragIndex ?? '-1', 10)
      const from = dragIndex.value
      if (isNaN(to) || to < 0 || to === from) return
      // 指针越过目标项中线才换位，避免来回抖动
      const rect = target.getBoundingClientRect()
      const midX = rect.left + rect.width / 2
      const midY = rect.top + rect.height / 2
      const horizontal = Math.abs(ev.clientX - midX) > Math.abs(ev.clientY - midY)
      const crossing = horizontal
        ? (to < from ? ev.clientX < midX : ev.clientX > midX)
        : (to < from ? ev.clientY < midY : ev.clientY > midY)
      if (!crossing) return
      commit(from, to)
      dragIndex.value = to
    }

    function onUp(): void {
      document.removeEventListener('mousemove', onMove)
      document.removeEventListener('mouseup', onUp)
      document.body.classList.remove('drag-sorting')
      dragIndex.value = -1
    }

    document.addEventListener('mousemove', onMove)
    document.addEventListener('mouseup', onUp)
  }

  return { dragIndex, startDrag }
}
