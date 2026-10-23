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
  <div
    class="split-bar"
    :class="[orientation, { dragging: isDragging, 'split-bar--inactive': inactive }]"
    @mousedown="startDrag"
  >
    <div class="split-bar-grip"></div>
  </div>
</template>

<script setup lang="ts">
import { ref, onBeforeUnmount } from 'vue'

/**
 * 可拖拽分隔条
 * orientation: 'vertical' = 竖线，左右拖拽（col-resize）
 *              'horizontal' = 横线，上下拖拽（row-resize）
 */
interface Props {
  orientation: 'vertical' | 'horizontal'
  /** 非激活（面板折叠）：尺寸过渡为 0，配合折叠/展开动画 */
  inactive?: boolean
}

const props = defineProps<Props>()

const emit = defineEmits<{ (e: 'drag', delta: number): void }>()

const isDragging = ref(false)
let lastPos = 0

function getPos(e: MouseEvent): number {
  return props.orientation === 'vertical' ? e.clientX : e.clientY
}

function startDrag(e: MouseEvent) {
  e.preventDefault()
  e.stopPropagation()
  isDragging.value = true
  lastPos = getPos(e)
  document.addEventListener('mousemove', onDrag)
  document.addEventListener('mouseup', stopDrag)
  document.body.classList.add('split-dragging')
  // 按方向区分光标：竖线左右拖 → col-resize，横线上下拖 → row-resize
  document.body.classList.add(props.orientation === 'vertical' ? 'split-dragging-x' : 'split-dragging-y')
}

function onDrag(e: MouseEvent) {
  if (!isDragging.value) return
  const delta = getPos(e) - lastPos
  lastPos = getPos(e)
  emit('drag', delta)
}

function stopDrag() {
  isDragging.value = false
  document.removeEventListener('mousemove', onDrag)
  document.removeEventListener('mouseup', stopDrag)
  document.body.classList.remove('split-dragging')
  document.body.classList.remove('split-dragging-x', 'split-dragging-y')
}

onBeforeUnmount(() => {
  document.removeEventListener('mousemove', onDrag)
  document.removeEventListener('mouseup', stopDrag)
  document.body.classList.remove('split-dragging')
  document.body.classList.remove('split-dragging-x', 'split-dragging-y')
})
</script>

<style scoped>
.split-bar {
  position: relative;
  flex-shrink: 0;
  z-index: 10;
  transition: background-color 0.15s, width 0.25s ease, height 0.25s ease;
}
.split-bar--inactive {
  pointer-events: none;
}
.split-bar--inactive.vertical {
  width: 0;
}
.split-bar--inactive.horizontal {
  height: 0;
}
/* 用 :not() 排除折叠态：否则与上面的 .split-bar--inactive 特异性相同、
   且本条在后面，会把折叠态的宽度重新拉回 5px（分隔条收不回去）。
   配合 .split-bar 自带的 width/height transition，折叠时平滑收为 0。 */
.split-bar.vertical:not(.split-bar--inactive) {
  width: 5px;
  cursor: col-resize;
}
.split-bar.horizontal:not(.split-bar--inactive) {
  height: 5px;
  cursor: row-resize;
}
.split-bar:hover {
  background-color: #3b82f6;
}
.split-bar.dragging {
  background-color: #2563eb;
}
.split-bar-grip {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  background-color: #6b7280;
  border-radius: 2px;
  opacity: 0;
  transition: opacity 0.15s;
}
.split-bar.vertical .split-bar-grip {
  width: 2px;
  height: 32px;
}
.split-bar.horizontal .split-bar-grip {
  width: 32px;
  height: 2px;
}
.split-bar:hover .split-bar-grip {
  opacity: 1;
}
</style>

<style>
body.split-dragging {
  cursor: col-resize !important;
  user-select: none !important;
  -webkit-user-select: none !important;
}
/* 按拖拽方向覆盖光标（放在后面、特异性更高，覆盖上面的默认 col-resize） */
body.split-dragging.split-dragging-x {
  cursor: col-resize !important;
}
body.split-dragging.split-dragging-y {
  cursor: row-resize !important;
}
body.split-dragging * {
  pointer-events: none !important;
}
</style>
