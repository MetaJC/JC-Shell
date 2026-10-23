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
  <div v-if="state.active" class="drag-overlay">
    <!-- 三个磁吸区 -->
    <div
      v-for="d in docks"
      :key="d.dock"
      class="drop-zone"
      :class="{ active: state.hoverDock === d.dock }"
      :style="zoneStyle(d.dock)"
    >
      <div class="zone-label">
        <el-icon size="22">
          <component :is="d.icon" />
        </el-icon>
        <span>{{ d.label }}</span>
      </div>
    </div>
    <!-- 跟随鼠标的浮窗 -->
    <div class="ghost-window" :style="{ left: state.x + 14 + 'px', top: state.y + 14 + 'px' }">
      <el-icon size="14"><Rank /></el-icon>
      <span class="ml-1.5 text-xs">{{ panelLabel }}</span>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 拖拽磁吸预览遮罩
 * 作者：JC
 */
import { computed, type Component, type CSSProperties } from 'vue'
import { usePanelDrag, getZoneRects } from '@/composables/usePanelDrag'
import type { Dock } from '@/stores/layout'
import { Rank, ArrowLeft, ArrowRight, ArrowDown } from '@element-plus/icons-vue'

const { state } = usePanelDrag()

const docks: Array<{ dock: Dock; label: string; icon: Component }> = [
  { dock: 'left', label: '左侧', icon: ArrowLeft as unknown as Component },
  { dock: 'right', label: '右侧', icon: ArrowRight as unknown as Component },
  { dock: 'bottom', label: '底部', icon: ArrowDown as unknown as Component }
]

const PANEL_LABELS: Record<string, string> = {
  hostList: '主机列表',
  fileSystem: '文件系统',
  deviceInfo: '设备信息'
}

const panelLabel = computed(() => {
  const id = state.panelId
  return id ? PANEL_LABELS[id] ?? id : ''
})

/** 磁吸区尺寸：与当前各面板实际大小完全重合 */
const zoneRects = computed(() => getZoneRects())
function zoneStyle(dock: Dock): CSSProperties {
  const r = zoneRects.value[dock]
  return { left: r.left + 'px', top: r.top + 'px', width: r.width + 'px', height: r.height + 'px' }
}
</script>

<style scoped>
.drag-overlay {
  position: fixed;
  inset: 0;
  z-index: 9999;
  pointer-events: none;
  background: rgba(0, 0, 0, 0.15);
}
.drop-zone {
  position: absolute;
  border: 2px dashed rgba(59, 130, 246, 0.5);
  background: rgba(59, 130, 246, 0.08);
  border-radius: 8px;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 0.15s ease;
  color: rgba(147, 197, 253, 0.7);
}
.drop-zone.active {
  border-color: #3b82f6;
  background: rgba(59, 130, 246, 0.25);
  color: #fff;
  box-shadow: 0 0 0 4px rgba(59, 130, 246, 0.18);
  transform: scale(1.02);
}
.zone-label {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  font-weight: 600;
  letter-spacing: 0.05em;
}
.ghost-window {
  position: fixed;
  display: flex;
  align-items: center;
  padding: 6px 10px;
  background: rgba(37, 37, 38, 0.92);
  border: 1px solid #3b82f6;
  border-radius: 6px;
  color: #e5e7eb;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
}
</style>

<style>
body.panel-dragging {
  cursor: grabbing !important;
  user-select: none !important;
  -webkit-user-select: none !important;
}
body.panel-dragging * {
  pointer-events: none !important;
}
</style>
