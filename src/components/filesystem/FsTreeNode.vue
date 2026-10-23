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
  <div>
    <div v-for="n in nodes" :key="n.path" class="fs-tree-node">
      <div
        class="flex items-center gap-1 py-0.5 pr-2 cursor-pointer hover:bg-[var(--jc-bg-hover)] select-none"
        :class="rowClass(n)"
        :style="{ paddingLeft: (6 + level * 14 * zoom) + 'px', fontSize: 13 * zoom + 'px' }"
        :data-hover-click="n.type === 'dir' ? '' : undefined"
        :data-fs-dir="n.type === 'dir' ? n.path : undefined"
        :draggable="canDrag"
        @click="onRowClick(n)"
        @dblclick.stop="onRowDblClick(n)"
        @contextmenu.prevent.stop="onContextMenu(n, $event)"
        @dragstart="onRowDragStart(n, $event)"
        @dragend="onRowDragEnd(n)"
        @dragover="onRowDragOver(n, $event)"
        @dragleave="onRowDragLeave(n)"
        @drop="onRowDrop(n, $event)"
      >
        <span
          class="inline-flex items-center justify-center text-[var(--jc-text-soft)] shrink-0 hover:text-gray-100 cursor-pointer"
          :style="{ width: 18 * zoom + 'px', height: 18 * zoom + 'px' }"
          @click.stop="onCaretClick(n)"
        >
          <el-icon
            v-if="n.type === 'dir'"
            :size="13 * zoom"
            class="transition-transform duration-150"
            :class="{ 'rotate-90': n.expanded }"
          ><ArrowRight /></el-icon>
        </span>
        <el-icon :size="13 * zoom" :color="iconColor(n)" class="shrink-0">
          <Folder v-if="n.type === 'dir'" />
          <Link v-else-if="n.type === 'symlink'" />
          <Document v-else />
        </el-icon>
        <span
          class="truncate flex-1"
          :class="textClass(n)"
        >{{ n.name }}</span>
        <span v-if="n.type !== 'dir'" class="text-[var(--jc-text-muted)] shrink-0" :style="{ fontSize: 10 * zoom + 'px' }">{{ formatSizeShort(n.size) }}</span>
      </div>
      <FsTreeNode
        v-if="n.expanded && n.children && n.children.length > 0"
        class="fs-tree-children"
        :nodes="n.children"
        :level="level + 1"
        :selected-path="selectedPath"
        :current-path="currentPath"
        :active-dir-paths="activeDirPaths"
        :zoom="zoom"
        :can-drag="canDrag"
        @select="onSelect"
        @toggle="onToggle"
        @open="onOpen"
        @contextmenu="onContextMenu"
        @row-drag-start="onRowDragStart"
        @row-drag-over="onRowDragOver"
        @row-drag-leave="onRowDragLeave"
        @row-drop="onRowDrop"
        @row-drag-end="onRowDragEnd"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 文件系统树形节点（递归组件）
 * 作者：JC
 * 单击行：选中（目录同时触发 open 用于联动）；单击箭头/双击行：展开折叠
 * 高亮规则：当前目录（pathStack 最深层）高亮；父级已打开目录低亮
 */
import { Folder, Document, Link, ArrowRight } from '@element-plus/icons-vue'
import { formatSizeShort } from '@/utils/format'
import { iconColor, type FileNode } from './fs-types'

const props = withDefaults(defineProps<{
  nodes: FileNode[]
  level?: number
  selectedPath?: string
  currentPath?: string
  activeDirPaths?: string[]
  zoom?: number
  /** 是否允许将该行作为拖拽源（连接状态 / 非重命名） */
  canDrag?: boolean
}>(), { level: 0, selectedPath: '', currentPath: '', activeDirPaths: () => [], zoom: 1, canDrag: false })

const emit = defineEmits<{
  (e: 'select', node: FileNode): void
  (e: 'toggle', node: FileNode): void
  (e: 'open', node: FileNode): void
  (e: 'contextmenu', node: FileNode, ev: MouseEvent): void
  (e: 'row-drag-start', node: FileNode, ev: DragEvent): void
  (e: 'row-drag-over', node: FileNode, ev: DragEvent): void
  (e: 'row-drag-leave', node: FileNode): void
  (e: 'row-drop', node: FileNode, ev: DragEvent): void
  (e: 'row-drag-end', node: FileNode): void
}>()

/** 行背景：当前目录高亮 > 父级已打开低亮 > 选中 > 默认 */
function rowClass(n: FileNode) {
  if (n.path === props.currentPath) return 'bg-blue-600/30'
  if (props.activeDirPaths.includes(n.path)) return 'bg-blue-600/10'
  if (props.selectedPath === n.path) return 'bg-blue-600/20'
  return ''
}

/** 文字颜色：当前目录蓝色加粗 > 父级已打开浅蓝 > 普通 */
function textClass(n: FileNode) {
  if (n.path === props.currentPath) return 'text-blue-300 font-medium'
  if (props.activeDirPaths.includes(n.path)) return 'text-blue-200/70'
  return 'text-[var(--jc-text)]'
}

function onRowClick(n: FileNode) {
  emit('select', n)
  if (n.type === 'dir') emit('open', n)
}
function onRowDblClick(n: FileNode) {
  if (n.type === 'dir') emit('toggle', n)
  else emit('open', n)
}
function onContextMenu(n: FileNode, ev: MouseEvent) {
  emit('select', n)
  emit('contextmenu', n, ev)
}
function onCaretClick(n: FileNode) {
  if (n.type === 'dir') emit('toggle', n)
}
function onSelect(n: FileNode) { emit('select', n) }
function onToggle(n: FileNode) { emit('toggle', n) }
function onOpen(n: FileNode) { emit('open', n) }
function onRowDragStart(n: FileNode, ev: DragEvent) { emit('row-drag-start', n, ev) }
function onRowDragOver(n: FileNode, ev: DragEvent) { emit('row-drag-over', n, ev) }
function onRowDragLeave(n: FileNode) { emit('row-drag-leave', n) }
function onRowDrop(n: FileNode, ev: DragEvent) { emit('row-drop', n, ev) }
function onRowDragEnd(n: FileNode) { emit('row-drag-end', n) }
</script>

<style scoped>
/* 展开/折叠丝滑化（60 帧）：
   1) 屏外节点跳过渲染（content-visibility）：大目录展开时浏览器只绘制可见行，
      展开/滚动的每帧渲染成本与「屏外有多少行」彻底解耦；
      contain-intrinsic-size 给未渲染块一个行高估值，避免滚动条跳动（auto 记忆实测尺寸）。
   2) 子节点块挂载动画只用 opacity + transform（合成层属性，全程不触发重排）；
      不做 height 动画 —— 高度动画每帧都重排整棵子树，正是低帧率的典型根因。
   3) 箭头旋转本来就是 transform 过渡（见模板 transition-transform），天然走合成层。 */
.fs-tree-node {
  content-visibility: auto;
  contain-intrinsic-size: auto 24px;
}
.fs-tree-children {
  animation: fs-tree-in 0.16s ease-out;
}
@keyframes fs-tree-in {
  from { opacity: 0; transform: translateY(-4px); }
  to { opacity: 1; transform: none; }
}
</style>
