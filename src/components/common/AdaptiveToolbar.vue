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
    ref="rootRef"
    class="adaptive-toolbar flex items-center h-full"
    :class="stretch ? ['flex-1 min-w-0', { 'justify-end': align === 'right' }] : 'shrink-0'"
  >
    <!-- 可见按钮区域：收缩不拉伸，超出部分隐藏，由 calculate 决定显示数量 -->
    <div ref="containerRef" class="flex items-center shrink-0 overflow-hidden">
      <template v-for="(btn, index) in visibleButtons" :key="btn.key || index">
        <div v-if="btn.divider" class="toolbar-divider" :title="btn.tooltip"></div>
        <el-button
          v-else
          size="small"
          text
          class="toolbar-btn"
          :class="{ 'toolbar-btn--active': btn.active, 'toolbar-btn--disabled': btn.disabled }"
          :title="btn.tooltip"
          :disabled="btn.disabled"
          data-hover-click
          @click="btn.onClick"
        >
          <el-icon v-if="btn.icon">
            <component :is="btn.icon" />
          </el-icon>
          <span v-if="btn.text" class="ml-1">{{ btn.text }}</span>
        </el-button>
      </template>
    </div>
    <!-- 折叠溢出按钮（右侧箭头） -->
    <template v-if="collapsedButtons.length > 0">
      <div class="relative shrink-0" @mouseenter="showOverflow = true" @mouseleave="showOverflow = false">
        <el-button size="small" text class="toolbar-btn">
          <el-icon><DArrowRight /></el-icon>
        </el-button>
        <!-- 悬浮溢出工具栏：行式按钮，与正常工具栏样式一致；深色阴影浮起 -->
        <div
          v-show="showOverflow"
          class="overflow-toolbar absolute top-full right-0 z-50 mt-0.5 flex items-center gap-0.5 bg-[var(--jc-bg-hover)] border border-[var(--jc-border)] rounded py-0.5 px-0.5 whitespace-nowrap"
          :style="{ boxShadow: '0 8px 24px rgba(0, 0, 0, 0.45), 0 2px 6px rgba(0, 0, 0, 0.35)' }"
          @mouseenter="showOverflow = true"
          @mouseleave="showOverflow = false"
        >
          <template v-for="(btn, index) in collapsedButtons" :key="btn.key || index">
            <div v-if="btn.divider" class="toolbar-divider" :title="btn.tooltip"></div>
            <el-button
              v-else
              size="small"
              text
              class="toolbar-btn"
              :class="{ 'toolbar-btn--active': btn.active, 'toolbar-btn--disabled': btn.disabled }"
              :title="btn.tooltip"
              :disabled="btn.disabled"
              data-hover-click
              @click="btn.onClick"
            >
              <el-icon v-if="btn.icon">
                <component :is="btn.icon" />
              </el-icon>
              <span v-if="btn.text" class="ml-1">{{ btn.text }}</span>
            </el-button>
          </template>
        </div>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
/**
 * 自适应折叠工具栏
 * 作者：JC
 * 宽度足够时全量显示；宽度不足时右侧出现箭头，悬浮展开行式溢出工具栏。
 */
import { ref, computed, onMounted, onBeforeUnmount, watch, nextTick } from 'vue'
import { DArrowRight } from '@element-plus/icons-vue'
import type { Component } from 'vue'

export interface ToolbarButton {
  key: string
  icon?: Component
  text?: string
  tooltip?: string
  active?: boolean
  /** 置灰且不可点击（如无法后退/前进/上级目录时） */
  disabled?: boolean
  /** 为 true 时渲染为竖线分隔符，不触发点击 */
  divider?: boolean
  onClick: () => void
}

const props = withDefaults(
  defineProps<{
    buttons: ToolbarButton[]
    align?: 'left' | 'right'
    /** 是否占据剩余宽度（true：作为头部右侧主工具条；false：内容自适应宽度，可与外部按钮紧贴组合） */
    stretch?: boolean
  }>(),
  { align: 'left', stretch: true }
)

const rootRef = ref<HTMLElement | null>(null)
const containerRef = ref<HTMLElement | null>(null)
const showAll = ref(true)
const showOverflow = ref(false)

/** 单按钮实测宽度（排除分隔线）：全量渲染时从 DOM 测量，避免固定常量与实际样式脱节 */
const perBtnWidth = ref(26)
const COLLAPSE_BTN_WIDTH = 26
const MARGIN = 2
/** 分隔线实际占位宽度（与 .toolbar-divider 样式保持一致） */
const DIVIDER_WIDTH = 9

const visibleButtons = computed(() => {
  if (showAll.value) return props.buttons
  return props.buttons.slice(0, visibleCount.value)
})

const collapsedButtons = computed(() => {
  if (showAll.value) return []
  return props.buttons.slice(visibleCount.value)
})

const visibleCount = ref(props.buttons.length)

/**
 * 紧凑模式取「有效布局父级」：跳过 display:contents 的包装层。
 * 这类元素不生成盒模型，clientWidth 恒为 0（如 FileSystemBrowser 的 .fs-mobile-toolbar
 * 在 PC 端就是 display:contents），直接当父级测宽会把可用宽度算成 0，
 * 导致全部按钮被错误收进溢出菜单。逐层向上找到第一个真实盒模型的祖先为止。
 */
function layoutParent(): HTMLElement | null {
  let el: HTMLElement | null = rootRef.value?.parentElement ?? null
  while (el && getComputedStyle(el).display === 'contents') {
    el = el.parentElement
  }
  return el
}

function calculate() {
  // 拉伸模式：根元素占头部剩余宽度（不随内容变化），切断“按钮减少 → 容器变窄 → 继续收纳”反馈环；
  // 紧凑模式（stretch=false）：根元素宽度即内容宽，以父级可用宽度为基准判定收纳
  if (!rootRef.value) return
  const dividerCount = props.buttons.filter(b => b.divider).length
  const realCount = props.buttons.length - dividerCount
  if (realCount === 0) return
  // 全量渲染时重新实测单按钮宽度（仅算按钮，分隔线单独计）
  if (showAll.value && containerRef.value && containerRef.value.scrollWidth > 0) {
    const dividerTotal = dividerCount * DIVIDER_WIDTH
    const realTotal = Math.max(0, containerRef.value.scrollWidth - dividerTotal)
    perBtnWidth.value = Math.max(1, realTotal / realCount)
  }
  const totalWidth = realCount * perBtnWidth.value + dividerCount * DIVIDER_WIDTH
  let available: number
  if (props.stretch) {
    available = rootRef.value.clientWidth - MARGIN - 2
  } else {
    const parent = layoutParent()
    available = (parent ? parent.clientWidth : rootRef.value.clientWidth) - MARGIN - 2
  }

  if (totalWidth <= available) {
    showAll.value = true
  } else {
    showAll.value = false
    // 可见项数：按累计宽度累加（按钮和分隔线宽度不同），到溢出箭头宽度前停止
    const effectiveWidth = available - COLLAPSE_BTN_WIDTH
    let acc = 0
    let count = 0
    for (const btn of props.buttons) {
      const w = btn.divider ? DIVIDER_WIDTH : perBtnWidth.value
      if (acc + w > effectiveWidth) break
      acc += w
      count++
    }
    visibleCount.value = count
  }
}

let resizeObserver: ResizeObserver | null = null

onMounted(async () => {
  await nextTick()
  calculate()
  resizeObserver = new ResizeObserver(() => calculate())
  if (rootRef.value) resizeObserver.observe(rootRef.value)
  // 紧凑模式下可用宽度取决于有效布局父级，同步监听其尺寸变化
  // （跳过 display:contents 包装层：0 尺寸的包装层永远不触发回调，窗口变化会漏算）
  if (!props.stretch) {
    const layoutParentEl = layoutParent()
    if (layoutParentEl) resizeObserver.observe(layoutParentEl)
  }
})

watch(() => props.buttons, async () => {
  await nextTick()
  calculate()
}, { deep: true })

onBeforeUnmount(() => {
  if (resizeObserver) resizeObserver.disconnect()
})
</script>

<style scoped>
.toolbar-btn {
  min-width: 0;
  padding: 4px 6px !important;
  color: var(--jc-text) !important;
  font-size: 13px !important;
  height: 26px !important;
}
/* 紧凑排列：去掉 Element Plus 默认的相邻按钮 12px 外边距 */
.toolbar-btn + .toolbar-btn {
  margin-left: 0 !important;
}
.toolbar-btn:hover {
  color: var(--jc-text) !important;
  background-color: rgba(255, 255, 255, 0.08) !important;
}
.toolbar-btn--active {
  color: #60a5fa !important;
  background-color: rgba(96, 165, 250, 0.15) !important;
}
.toolbar-btn--active:hover {
  color: #93bbfd !important;
  background-color: rgba(96, 165, 250, 0.25) !important;
}
/* 置灰禁用态：降低不透明度并禁止指针交互 */
.toolbar-btn--disabled,
.toolbar-btn--disabled:hover {
  color: var(--jc-text-muted, #6b7280) !important;
  opacity: 0.4;
  cursor: not-allowed !important;
  background-color: transparent !important;
}
/* 竖线分隔：用于不同功能分组隔离 */
.toolbar-divider {
  width: 1px;
  height: 16px;
  background-color: var(--jc-border);
  margin: 0 4px;
  flex-shrink: 0;
  pointer-events: none;
}
.overflow-toolbar {
  animation: fadeIn 0.12s ease-out;
}
@keyframes fadeIn {
  from { opacity: 0; transform: translateY(-4px); }
  to { opacity: 1; transform: translateY(0); }
}
</style>
