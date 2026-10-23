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
  <el-dialog
    :model-value="modelValue"
    title="卡片管理"
    width="560"
    append-to-body
    class="jc-dialog"
    @update:model-value="v => emit('update:modelValue', v)"
  >
    <div class="text-xs text-[var(--jc-text-muted)] mb-2">
      首页展示与全部卡片互斥：同一卡片只会出现在其中一个区域。首页卡片可拖拽调整顺序；点击卡片右上角
      <span class="inline-flex w-4 h-4 rounded-full bg-red-500 text-[var(--jc-text)] text-[10px] items-center justify-center">x</span>
      移出首页；点击全部卡片右上角
      <span class="inline-flex w-4 h-4 rounded-full bg-green-500 text-[var(--jc-text)] text-[10px] items-center justify-center">+</span>
      添加到首页。
    </div>

    <!-- 首页展示区：可拖拽排序，x 常显 -->
    <div class="mb-4">
      <div class="text-sm font-semibold text-[var(--jc-text)] mb-2">首页展示</div>
      <div data-drag-container class="grid grid-cols-5 gap-3 p-3 min-h-[80px] bg-[var(--jc-bg-hover)] border border-dashed border-[var(--jc-border)] rounded-lg">
        <div
          v-for="(card, index) in localCards"
          :key="card.id"
          class="card-icon-wrap cursor-move"
          :class="{ 'card-icon-wrap--dragging': dragIndex === index }"
          :data-drag-index="index"
          @mousedown="startDrag($event, index)"
        >
          <div class="flex flex-col items-center gap-1">
            <!-- 图标盒：x 锚定在图标右上角 -->
            <div class="relative">
              <span class="remove-btn absolute -top-1.5 -right-1.5 w-4 h-4 rounded-full bg-red-500 text-[var(--jc-text)] text-[10px] flex items-center justify-center cursor-pointer z-10" @click="removeFromHome(card.id)">
                x
              </span>
              <div class="w-12 h-12 rounded-lg flex items-center justify-center" :style="{ backgroundColor: getCardColor(card.id) }">
                <el-icon size="24" color="#fff">
                  <component :is="getCardIcon(card.id)" />
                </el-icon>
              </div>
            </div>
            <span class="text-[11px] text-[var(--jc-text)]">{{ getCardTitle(card.id) }}</span>
          </div>
        </div>
      </div>
      <div v-if="localCards.length === 0" class="text-center text-[var(--jc-text-muted)] text-xs py-3">
        暂无卡片，从下方添加
      </div>
    </div>

    <!-- 全部卡片区：仅展示未在首页的卡片（与首页互斥），+ 常显 -->
    <div>
      <div class="text-sm font-semibold text-[var(--jc-text)] mb-2">全部卡片</div>
      <div class="grid grid-cols-5 gap-3 p-3 bg-[var(--jc-bg)] border border-[var(--jc-border)] rounded-lg">
        <div
          v-for="card in poolCards"
          :key="card.id"
          class="card-icon-wrap"
        >
          <div class="flex flex-col items-center gap-1">
            <!-- 图标盒：+ 锚定在图标右上角 -->
            <div class="relative">
              <span class="add-btn absolute -top-1.5 -right-1.5 w-4 h-4 rounded-full bg-green-500 text-[var(--jc-text)] text-[10px] flex items-center justify-center cursor-pointer z-10" @click="addToHome(card.id)">
                +
              </span>
              <div class="w-12 h-12 rounded-lg flex items-center justify-center" :style="{ backgroundColor: getCardColor(card.id) }">
                <el-icon size="24" color="#fff">
                  <component :is="getCardIcon(card.id)" />
                </el-icon>
              </div>
            </div>
            <span class="text-[11px] text-[var(--jc-text)]">{{ getCardTitle(card.id) }}</span>
          </div>
        </div>
        <div v-if="poolCards.length === 0" class="col-span-5 text-center text-[var(--jc-text-muted)] text-xs py-3">
          所有卡片都已在首页展示
        </div>
      </div>
    </div>

    <template #footer>
      <div class="flex justify-between items-center w-full">
        <el-button text size="small" @click="resetCards">恢复默认</el-button>
        <div>
          <el-button size="small" @click="close">取消</el-button>
          <el-button size="small" type="primary" @click="apply">保存</el-button>
        </div>
      </div>
    </template>
  </el-dialog>
</template>

<script setup lang="ts">
/**
 * 设备卡片设置弹窗（图标网格布局）
 * 作者：JC
 * 首页展示 / 全部卡片互斥：卡片只属于其中一个区域；首页卡片支持原生拖拽排序。
 */
import { ref, watch, computed } from 'vue'
import { ElDialog, ElButton, ElIcon } from 'element-plus'
import { Monitor, Cpu, Odometer, PieChart, Connection, Operation, MagicStick } from '@element-plus/icons-vue'
import { useLayoutStore, type DeviceCardConfig, type DeviceCardId } from '@/stores/layout'
import { useDragSort } from '@/composables/useDragSort'

const props = defineProps<{ modelValue: boolean }>()
const emit = defineEmits<{ (e: 'update:modelValue', v: boolean): void }>()

const layout = useLayoutStore()

/** 首页展示卡片（本地副本，保存时写回 store） */
const localCards = ref<DeviceCardConfig[]>([])

watch(() => props.modelValue, v => {
  if (v) {
    localCards.value = layout.deviceCards
      .filter(c => c.visible)
      .map(c => ({ ...c }))
  }
})

// 默认卡片全集（固定六张 + AI 助手）
const DEFAULT_IDS: DeviceCardId[] = ['system', 'cpu', 'mem', 'disk', 'net', 'proc', 'aiAssistant']

/** 全部卡片区：与首页互斥，仅列出未在首页的卡片 */
const poolCards = computed<DeviceCardConfig[]>(() => {
  const homeIds = new Set(localCards.value.map(c => c.id))
  return DEFAULT_IDS
    .filter(id => !homeIds.has(id))
    .map(id => {
      const existing = layout.deviceCards.find(c => c.id === id)
      return existing ? { ...existing } : { id, visible: false, collapsed: false }
    })
})

/** 首页卡片拖拽排序（指针拖拽：mousedown/mousemove） */
const { dragIndex, startDrag } = useDragSort((from, to) => {
  const arr = [...localCards.value]
  const [moved] = arr.splice(from, 1)
  arr.splice(to, 0, moved)
  localCards.value = arr
})

function getCardTitle(id: DeviceCardId): string {
  const map: Record<DeviceCardId, string> = {
    system: '系统信息', cpu: 'CPU', mem: '内存', disk: '磁盘', net: '网络', proc: '进程', aiAssistant: 'AI 助手'
  }
  return map[id]
}

function getCardIcon(id: DeviceCardId) {
  const map = { system: Monitor, cpu: Cpu, mem: Odometer, disk: PieChart, net: Connection, proc: Operation, aiAssistant: MagicStick }
  return map[id]
}

function getCardColor(id: DeviceCardId): string {
  const map: Record<DeviceCardId, string> = {
    system: '#3b82f6', cpu: '#ef4444', mem: '#10b981', disk: '#f59e0b', net: '#8b5cf6', proc: '#0ea5e9', aiAssistant: '#d946ef'
  }
  return map[id]
}

function removeFromHome(id: DeviceCardId) {
  localCards.value = localCards.value.filter(c => c.id !== id)
}

function addToHome(id: DeviceCardId) {
  if (localCards.value.some(c => c.id === id)) return
  const existing = layout.deviceCards.find(c => c.id === id)
  localCards.value.push(existing ? { ...existing, visible: true } : { id, visible: true, collapsed: false })
}

function apply() {
  const homeIds = new Set(localCards.value.map(c => c.id))
  // 完整列表 = 首页顺序在前 + 其余卡片保持原顺序在后（互斥：首页内 visible，其余不可见）
  const finalList: DeviceCardConfig[] = localCards.value.map(c => ({ ...c, visible: true }))
  for (const c of layout.deviceCards) {
    if (!homeIds.has(c.id)) finalList.push({ ...c, visible: false })
  }
  layout.reorderCards(finalList.map(c => c.id))
  finalList.forEach(c => {
    const target = layout.deviceCards.find(x => x.id === c.id)
    if (target) {
      target.visible = c.visible
      target.collapsed = c.collapsed
    }
  })
  layout.persist()
  close()
}

function resetCards() {
  // AI 助手卡片遵循"默认关闭"原则，恢复默认时不自动展示
  localCards.value = DEFAULT_IDS
    .filter(id => id !== 'aiAssistant')
    .map(id => ({ id, visible: true, collapsed: false }))
}

function close(): void {
  emit('update:modelValue', false)
}
</script>

<style scoped>
.card-icon-wrap {
  width: 100%;
  display: flex;
  justify-content: center;
  user-select: none;
}
.card-icon-wrap--dragging {
  opacity: 0.5;
}
.card-icon-wrap .remove-btn:hover {
  background-color: #dc2626;
}
.card-icon-wrap .add-btn:hover {
  background-color: #059669;
}
</style>
