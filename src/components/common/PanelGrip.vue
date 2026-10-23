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
    class="panel-grip"
    title="拖拽到左侧/右侧/底部"
    @mousedown="onDown"
  >
    <span class="dot"></span>
    <span class="dot"></span>
    <span class="dot"></span>
    <span class="dot"></span>
    <span class="dot"></span>
    <span class="dot"></span>
  </div>
</template>

<script setup lang="ts">
/**
 * 面板拖拽手柄（6 点 grip）
 * 作者：JC
 */
import { startPanelDrag } from '@/composables/usePanelDrag'
import type { PanelId } from '@/stores/layout'

const props = defineProps<{ panelId: PanelId }>()

function onDown(e: MouseEvent) {
  startPanelDrag(props.panelId, e)
}
</script>

<style scoped>
.panel-grip {
  display: grid;
  grid-template-columns: 6px 6px;
  grid-template-rows: 6px 6px 6px;
  gap: 2px;
  padding: 4px 6px;
  cursor: grab;
  border-radius: 4px;
  transition: background-color 0.15s;
}
.panel-grip:hover {
  background-color: rgba(59, 130, 246, 0.15);
}
.panel-grip:active {
  cursor: grabbing;
}
.dot {
  width: 3px;
  height: 3px;
  border-radius: 50%;
  background-color: #6b7280;
}
.panel-grip:hover .dot {
  background-color: #3b82f6;
}
</style>
