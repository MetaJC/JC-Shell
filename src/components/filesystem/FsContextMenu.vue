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
  <Teleport to="body">
    <div v-if="items.length" class="fs-ctx-root fixed" :style="rootStyle" @contextmenu.prevent>
      <ul class="fs-ctx-menu">
        <li
          v-for="(item, idx) in items"
          :key="item.key"
          ref="menuItems"
          :data-idx="idx"
          class="fs-ctx-item"
          :class="{ 'fs-ctx-disabled': item.disabled, 'fs-ctx-danger': item.danger }"
          @mouseenter="hoverKey = item.key"
          @click="onItemClick(item)"
        >
          <el-icon v-if="item.icon" :size="menuIconSize" class="shrink-0"><component :is="item.icon" /></el-icon>
          <span class="flex-1 truncate" :title="item.label">{{ displayLabel(item.label) }}</span>
          <el-icon v-if="item.children?.length" :size="subArrowSize" class="shrink-0 text-[var(--jc-text-muted)]"><ArrowRight /></el-icon>
        </li>
      </ul>
    </div>
    <!-- 独立 Teleport 渲染子菜单，避免父级 stacking context 限制 -->
    <div
      v-if="activeSubMenu"
      class="fs-ctx-sub-wrap fixed z-[1400]"
      :style="subMenuStyle"
    >
      <ul class="fs-ctx-menu">
        <li
          v-for="sub in activeSubMenu.children"
          :key="sub.key"
          class="fs-ctx-item"
          :class="{ 'fs-ctx-disabled': sub.disabled, 'fs-ctx-danger': sub.danger }"
          @click="onItemClick(sub)"
        >
          <el-icon v-if="sub.icon" :size="menuIconSize" class="shrink-0"><component :is="sub.icon" /></el-icon>
          <span class="flex-1 truncate" :title="sub.label">{{ displayLabel(sub.label) }}</span>
        </li>
      </ul>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * 文件系统右键菜单（通用两级菜单，独立 Teleport 渲染子菜单）
 * 作者：JC
 * 点击外部 / Esc / 滚动 / 窗口缩放时关闭；子菜单悬停展开，右侧空间不足时向左弹出。
 */
import { ref, computed, onMounted, onBeforeUnmount, watch, nextTick } from 'vue'
import { ArrowRight } from '@element-plus/icons-vue'
import type { Component } from 'vue'
import { isMobileWebview } from '@/utils/platform'

export interface FsMenuItem {
  key: string
  label: string
  icon?: Component
  disabled?: boolean
  danger?: boolean
  children?: FsMenuItem[]
}

const props = defineProps<{ x: number; y: number; items: FsMenuItem[] }>()
const emit = defineEmits<{ (e: 'select', key: string): void; (e: 'close'): void }>()

const hoverKey = ref('')

/** 移动端放大倍率：触摸端文字/行高/内边距整体等比例放大，桌面端恒为 1（完全不变） */
const ctxScale = isMobileWebview() ? 1.5 : 1
/** 缩放前基准（与 CSS 中 calc(var(--ctx-s) * Npx) 保持一致，供防出屏计算使用） */
const CTX_W = 220
const CTX_ROW_H = 28
const CTX_PAD = 8
const menuIconSize = Math.round(13 * ctxScale)
const subArrowSize = Math.round(10 * ctxScale)

/**
 * 超长标签中间省略：保留开头（动作名+文件名前缀）与结尾（格式后缀+引号），
 * 如 压缩为 "河南省不涉及相关审批的承诺书.tar.gz" → 压缩为 "河南省不…书.tar.gz"
 */
function displayLabel(label: string): string {
  const MAX = 26
  if (label.length <= MAX) return label
  const keepStart = 12
  const keepEnd = 11
  return label.slice(0, keepStart) + '…' + label.slice(-keepEnd)
}
const menuItems = ref<HTMLElement[]>([])
const subMenuTop = ref(0)
const subMenuLeft = ref(0)
const subMenuFlipLeft = ref(false)

/** 菜单位置：防出屏（基准 220px 宽、每项 28px 高，按 ctxScale 等比放大） */
const pos = computed(() => {
  const w = CTX_W * ctxScale
  const h = props.items.length * CTX_ROW_H * ctxScale + CTX_PAD
  return {
    x: Math.min(props.x, window.innerWidth - w - 8),
    y: Math.min(props.y, window.innerHeight - h - 8)
  }
})

const rootStyle = computed(() => ({
  left: pos.value.x + 'px',
  top: pos.value.y + 'px',
  zIndex: 1300,
  // 供 CSS calc 等比缩放（文字/行高/内边距/圆角）
  '--ctx-s': String(ctxScale)
}))

/** 当前悬停的子菜单数据 */
const activeSubMenu = computed(() => {
  if (!hoverKey.value) return null
  const item = props.items.find(i => i.key === hoverKey.value)
  if (!item || !item.children?.length) return null
  return item
})

/** 子菜单样式：独立定位到 body，防出屏 */
const subMenuStyle = computed(() => {
  const w = CTX_W * ctxScale
  const h = (activeSubMenu.value?.children?.length || 0) * CTX_ROW_H * ctxScale + CTX_PAD
  let left = subMenuLeft.value
  let top = subMenuTop.value
  if (left + w > window.innerWidth - 8) {
    left = Math.max(8, window.innerWidth - w - 8)
  }
  if (top + h > window.innerHeight - 8) {
    top = Math.max(8, window.innerHeight - h - 8)
  }
  return {
    left: left + 'px',
    top: top + 'px',
    zIndex: 1400,
    '--ctx-s': String(ctxScale)
  }
})

/** 计算子菜单的弹出位置（相对于父菜单项） */
function updateSubMenuPos() {
  if (!hoverKey.value) return
  const itemIdx = props.items.findIndex(i => i.key === hoverKey.value)
  if (itemIdx < 0) return
  const itemEl = menuItems.value[itemIdx]
  if (!itemEl) return
  const rect = itemEl.getBoundingClientRect()
  let left = rect.right + 2
  let top = rect.top - 4
  const w = CTX_W * ctxScale
  if (left + w > window.innerWidth - 8) {
    left = rect.left - w - 2
    subMenuFlipLeft.value = true
  } else {
    subMenuFlipLeft.value = false
  }
  subMenuLeft.value = left
  subMenuTop.value = top
}

watch(hoverKey, async () => {
  if (hoverKey.value) {
    await nextTick()
    updateSubMenuPos()
  }
})

function onItemClick(item: FsMenuItem) {
  if (item.disabled || item.children?.length) return
  emit('select', item.key)
  emit('close')
}

function onGlobalDown(e: MouseEvent) {
  const root = document.querySelector('.fs-ctx-root')
  const sub = document.querySelector('.fs-ctx-sub-wrap')
  const targets = [root, sub].filter(Boolean) as HTMLElement[]
  if (targets.some(t => e.target instanceof Node && t.contains(e.target))) return
  emit('close')
}
function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') emit('close')
}

onMounted(() => {
  requestAnimationFrame(() => {
    window.addEventListener('mousedown', onGlobalDown, true)
    window.addEventListener('keydown', onKey, true)
    window.addEventListener('resize', () => emit('close'), { once: true })
    window.addEventListener('blur', () => emit('close'), { once: true })
  })
})
onBeforeUnmount(() => {
  window.removeEventListener('mousedown', onGlobalDown, true)
  window.removeEventListener('keydown', onKey, true)
})
</script>

<style scoped>
.fs-ctx-menu {
  /* 通过 --ctx-s 等比缩放（移动端 1.5、桌面端 1，桌面完全不变） */
  min-width: calc(180px * var(--ctx-s, 1)); max-width: calc(260px * var(--ctx-s, 1));
  margin: 0; padding: calc(4px * var(--ctx-s, 1)); list-style: none;
  background: var(--jc-bg-soft);
  border: 1px solid var(--jc-border);
  border-radius: calc(6px * var(--ctx-s, 1));
  box-shadow: 0 6px 24px rgba(0, 0, 0, .45);
  overflow: visible;
}
.fs-ctx-item {
  position: relative;
  display: flex; align-items: center; gap: calc(6px * var(--ctx-s, 1));
  height: calc(28px * var(--ctx-s, 1)); padding: 0 calc(10px * var(--ctx-s, 1));
  font-size: calc(12px * var(--ctx-s, 1)); color: var(--jc-text);
  border-radius: calc(4px * var(--ctx-s, 1)); cursor: pointer; white-space: nowrap;
  overflow: visible;
}
.fs-ctx-item:hover { background: var(--jc-bg-hover); }
.fs-ctx-disabled { opacity: .4; cursor: not-allowed; }
.fs-ctx-disabled:hover { background: transparent; }
.fs-ctx-danger { color: #f87171; }
.fs-ctx-danger:hover { background: rgba(239, 68, 68, .15); }
</style>
