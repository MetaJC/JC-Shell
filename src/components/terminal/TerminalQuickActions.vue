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
  <!-- 终端右下角悬浮快捷操作：AI Agent / 精简模式 / 全屏（F11）/ 重连（断连时按 R） -->
  <div class="term-quick-actions">
    <!-- 启动引导提示：指向 AI Agent 按钮；浮层独立于半透明按钮组，保持完全不透明 -->
    <div v-if="showAgentTip && !settingsOpen" class="qa-tip" @click="hideAgentTip">
      <span>点击开启 AI Agent 模式</span>
      <button class="qa-tip-close" @click.stop="hideAgentTip" aria-label="关闭提示">×</button>
      <div class="qa-tip-arrow" />
    </div>

    <!-- Agent 模式开启提示：按钮上方，6 秒后自动消失 -->
    <div v-if="showAgentOnTip && !settingsOpen" class="qa-tip qa-tip--on">
      <span>Agent 模式已开启，再次点击可关闭</span>
      <div class="qa-tip-arrow" />
    </div>

    <!-- 移动端快捷设置面板：在按钮组上方横向排开「字号 / 字体颜色 / 终端颜色」，
         每项为「名称在上、纵向挡位滑杆在下」，拖动即时生效（TerminalView 监听设置实时重排） -->
    <div v-if="canQuickSettings && settingsOpen" class="qs-panel" data-swipe-ignore @contextmenu.prevent>
      <div class="qs-item">
        <div class="qs-item__label">字号</div>
        <el-slider
          class="qs-slider"
          vertical
          height="224px"
          :min="FONT_QUICK_MIN"
          :max="FONT_QUICK_MAX"
          :step="1"
          show-stops
          :model-value="fontSize"
          @input="onFontSize"
        />
        <div class="qs-item__val">{{ fontSize }}</div>
      </div>

      <div class="qs-item qs-item--color">
        <div class="qs-item__label">字体颜色</div>
        <el-slider
          class="qs-slider"
          vertical
          height="224px"
          :min="0"
          :max="FG_COLORS.length - 1"
          :step="1"
          show-stops
          :model-value="fgIndex"
          :style="{ '--qs-grad': gradientOf(FG_COLORS) }"
          @input="onFgChange"
        />
        <div class="qs-item__val"><span class="qs-swatch" :style="{ background: FG_COLORS[fgIndex] }" /></div>
      </div>

      <div class="qs-item qs-item--color">
        <div class="qs-item__label">终端颜色</div>
        <el-slider
          class="qs-slider"
          vertical
          height="224px"
          :min="0"
          :max="BG_COLORS.length - 1"
          :step="1"
          show-stops
          :model-value="bgIndex"
          :style="{ '--qs-grad': gradientOf(BG_COLORS) }"
          @input="onBgChange"
        />
        <div class="qs-item__val"><span class="qs-swatch" :style="{ background: BG_COLORS[bgIndex] }" /></div>
      </div>
    </div>

    <div class="qa-buttons">
      <button
        class="qa-btn"
        :class="{ 'qa-active qa-agent-on': agent }"
        :title="agent ? '关闭 AI Agent 模式，恢复普通终端输入（Ctrl+Shift+I）' : '开启 AI Agent 模式：终端中直接输入自然语言（Ctrl+Shift+I）'"
        @click="emit('toggle-agent')"
      >
        <el-icon><MagicStick /></el-icon>
      </button>
      <!-- 精简模式：隐藏标题栏/停靠面板，是桌面窗口概念；移动端没有窗口边框，
           该按钮改成「清屏」（扫把），这才是触屏端的高频操作 -->
      <button
        v-if="!isMobile"
        class="qa-btn"
        :class="{ 'qa-active': zen }"
        :title="zen ? '退出精简模式' : '精简模式：只显示终端'"
        @click="emit('toggle-zen')"
      >
        <el-icon><Expand v-if="zen" /><Fold v-else /></el-icon>
      </button>
      <button
        v-else
        class="qa-btn"
        title="清屏：清空当前终端的显示内容"
        aria-label="清屏"
        @click="emit('clear')"
      >
        <!-- 扫把：Element Plus 无内置图标，用内联 SVG。
             Lucide 线框扫把（用户选定）：斜握长柄直达右上角 + 圆角扇形刷头 + 刷面斜线 -->
        <svg
          class="qa-svg"
          viewBox="0 0 24 24"
          width="16"
          height="16"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <!-- 手柄（斜握，直达右上角） -->
          <path d="M13.5 10.5 22 2" />
          <!-- 刷头（圆角扇形） -->
          <path d="M14.734 13.841a2 2 0 00-.314-2.42L12.58 9.58a2 2 0 00-2.421-.314l-7.657 4.461A1 1 0 002.3 15.3l6.403 6.403a1 1 0 001.571-.204z" />
          <!-- 刷面斜线 -->
          <path d="m7.699 10.7 5.602 5.601" />
          <!-- 底部鬃毛短线 -->
          <path d="m5 18 2-2" />
        </svg>
      </button>
      <!-- 快捷设置（字号 / 颜色）：移动端专属，桌面用设置页调整 -->
      <button
        v-if="canQuickSettings"
        class="qa-btn"
        :class="{ 'qa-active': settingsOpen }"
        title="终端快捷设置：字号 / 字体颜色 / 终端颜色"
        @click="settingsOpen = !settingsOpen"
      >
        <el-icon><Setting /></el-icon>
      </button>
      <!-- 全屏（F11）：桌面窗口概念，移动端不展示（能力开关见 utils/platform.ts） -->
      <button
        v-if="canFullscreen"
        class="qa-btn"
        :class="{ 'qa-active': fullscreen }"
        :title="fullscreen ? '退出全屏（F11）' : '全屏（F11）'"
        @click="emit('toggle-fullscreen')"
      >
        <el-icon><FullScreen /></el-icon>
      </button>
      <!-- 重连：移动端入口在标签菜单，悬浮按钮不展示 -->
      <button
        v-if="canReconnect"
        class="qa-btn"
        :class="{ 'qa-alert': disconnected }"
        title="重连当前终端（断连时按 R）"
        @click="emit('reconnect')"
      >
        <el-icon><RefreshRight /></el-icon>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 终端悬浮快捷操作（右下角）
 * 作者：JC
 * 精简模式（隐藏标题栏/停靠面板/Tab 标签）、全屏（F11）、断连重连（R）。
 * 由 TerminalWorkspace 挂载，动作通过 emit 交给工作区处理。
 * 移动端：仅保留「AI Agent / 精简模式」两个按钮 —— 全屏与重连属桌面窗口操作，
 * 且移动端屏幕窄，按钮过多会遮挡终端内容。
 */
import { ref, onMounted, onBeforeUnmount, watch } from 'vue'
import { Fold, Expand, FullScreen, RefreshRight, MagicStick, Setting } from '@element-plus/icons-vue'
import { CAP, isMobileWebview } from '@/utils/platform'
import { useAppStore } from '@/stores/app'
import { useSettingsStore } from '@/stores/settings'
import { effFontSize, settingsFontSize } from '@/utils/terminalFont'

const props = defineProps<{
  /** AI Agent 模式是否开启（按钮高亮态） */
  agent: boolean
  /** 精简模式是否开启（按钮高亮态） */
  zen: boolean
  /** 窗口是否处于全屏（按钮高亮态） */
  fullscreen: boolean
  /** 当前激活终端是否已断连（重连按钮高亮提示） */
  disconnected: boolean
}>()

const emit = defineEmits<{
  (e: 'toggle-agent'): void
  (e: 'toggle-zen'): void
  (e: 'toggle-fullscreen'): void
  (e: 'reconnect'): void
  /** 清屏（移动端扫把按钮）：清空当前终端显示内容 */
  (e: 'clear'): void
}>()

// 能力开关：平台在启动时即固定（dev 覆盖需刷新页面），故只在 setup 求值一次
const canFullscreen = CAP.windowFullscreen()
const canReconnect = CAP.terminalQuickReconnect()

const appStore = useAppStore()
const showAgentTip = ref(false)
onMounted(() => {
  if (!appStore.agentTipShown) {
    showAgentTip.value = true
    appStore.markAgentTipShown()
    setTimeout(() => { showAgentTip.value = false }, 6000)
  }
})
function hideAgentTip() { showAgentTip.value = false }

/** Agent 模式开启提示：进入 Agent 模式时在按钮上方显示 6 秒后自动消失 */
const AGENT_ON_TIP_DURATION = 6000
const showAgentOnTip = ref(false)
let agentOnTipTimer: ReturnType<typeof setTimeout> | null = null

function clearAgentOnTipTimer(): void {
  if (agentOnTipTimer) {
    clearTimeout(agentOnTipTimer)
    agentOnTipTimer = null
  }
}

watch(() => props.agent, (on) => {
  clearAgentOnTipTimer()
  if (!on) {
    // 退出 Agent 模式：立即收起提示
    showAgentOnTip.value = false
    return
  }
  // 与启动引导提示互斥，避免两条提示叠在一起
  showAgentTip.value = false
  showAgentOnTip.value = true
  agentOnTipTimer = setTimeout(() => {
    agentOnTipTimer = null
    showAgentOnTip.value = false
  }, AGENT_ON_TIP_DURATION)
})

onBeforeUnmount(() => {
  clearAgentOnTipTimer()
  document.removeEventListener('pointerdown', onDocPointerDown, true)
})

// ===== 移动端专属差异 =====
/** 是否移动端：决定按钮集合（精简模式 ↔ 清屏、是否显示快捷设置面板） */
const isMobile = isMobileWebview()
// ===== 移动端快捷设置面板（字号 / 字体颜色 / 终端颜色）=====
/** 仅移动端提供：桌面端用设置页调整，悬浮按钮已有 4 个，不再增加 */
const canQuickSettings = isMobile
const settingsOpen = ref(false)

/**
 * 点击「按钮组以外的任意区域」收起面板。
 * 用捕获阶段的 pointerdown 而不是遮罩层：一是无需铺一层挡住终端的蒙版（点击仍能正常落到终端），
 * 二是触摸/鼠标统一走 pointer 事件，无需分别处理。
 */
function onDocPointerDown(e: PointerEvent): void {
  if (!settingsOpen.value) return
  const el = e.target as HTMLElement | null
  // 面板与齿轮按钮都在 .term-quick-actions 内：点它们不关闭（齿轮自身负责切换）
  if (el?.closest?.('.term-quick-actions')) return
  settingsOpen.value = false
}
watch(settingsOpen, (open) => {
  if (open) document.addEventListener('pointerdown', onDocPointerDown, true)
  else document.removeEventListener('pointerdown', onDocPointerDown, true)
})

const settingsStore = useSettingsStore()

/** 字号挡位：6 ~ 22。滑杆显示的是「实际渲染字号」，
 *  移动端渲染时会 -4，故写回设置时用 settingsFontSize 换算，保证所见即所得 */
const FONT_QUICK_MIN = 6
const FONT_QUICK_MAX = 22
const fontSize = ref(clampFont(effFontSize(settingsStore.settings.terminalFontSize)))

function clampFont(v: number): number {
  return Math.min(FONT_QUICK_MAX, Math.max(FONT_QUICK_MIN, Math.round(v)))
}
/** 拖动即时生效：滑杆值 → 设置值（移动端 +4）写入设置，TerminalView 会实时重排 */
function onFontSize(v: number | number[]): void {
  const val = Array.isArray(v) ? v[0] : v
  fontSize.value = val
  settingsStore.patchSettings({ terminalFontSize: settingsFontSize(val) })
}
// 设置页改字号后，面板滑杆跟随（例如设置页把字号调到 20，面板显示 16）
watch(() => settingsStore.settings.terminalFontSize, (s) => {
  fontSize.value = clampFont(effFontSize(s))
})

/** 颜色挡位：
 *  - 字体色（14 档）：中性色 + 高可读的绿/青/黄为主，从亮到暗排列；
 *  - 终端底色（17 档）：去掉扎眼的高饱和色（亮蓝 / 紫 / 玫红），改为
 *    「中性灰黑/暗紫 + 沉稳深色 + 马卡龙浅色 + 米黄/白」四段，色相彼此可辨；
 *    深色段保证终端长久观看不刺眼，浅色段供浅色主题使用。
 *  滑杆底部为列表首项。 */
const FG_COLORS = [
  '#ffffff', '#f3f4f6', '#cbd5e1', '#d4d4d4', '#ffeaa7', '#ebcb8b', '#a3be8c',
  '#b5e853', '#88c0d0', '#7fdbff', '#6b7280', '#4b5563', '#1f2937', '#000000'
]
const BG_COLORS = [
  // 中性灰黑 / 暗紫
  '#000000', '#2c2c2c', '#2b0f2b',
  // 沉稳深色（低饱和，久看不刺眼）
  '#1e3a8a', '#0891b2', '#15803d', '#4d7c0f', '#a16207', '#b91c1c',
  // 马卡龙浅色
  '#f8d7da', '#d6e4f0', '#d8f3dc', '#fdf3d0', '#e6d9f7', '#d9eeee',
  // 米黄 / 纯白
  '#fdf6e3', '#ffffff'
]

const fgIndex = ref(Math.max(0, FG_COLORS.indexOf(appStore.terminalFgColor)))
const bgIndex = ref(Math.max(0, BG_COLORS.indexOf(appStore.terminalBgColor)))

/** 纵向滑杆的色带背景：底部为 index 0，顶部为最后一项 */
function gradientOf(colors: string[]): string {
  return `linear-gradient(to top, ${colors.join(', ')})`
}

function onFgChange(v: number | number[]): void {
  const i = Array.isArray(v) ? v[0] : v
  fgIndex.value = i
  appStore.setTerminalFg(FG_COLORS[i] || '')
}
function onBgChange(v: number | number[]): void {
  const i = Array.isArray(v) ? v[0] : v
  bgIndex.value = i
  appStore.setTerminalBg(BG_COLORS[i] || '')
}
// 设置页/其它窗口改色后，面板滑杆跟随
watch(() => appStore.terminalFgColor, (c) => {
  const i = FG_COLORS.indexOf(c)
  if (i >= 0) fgIndex.value = i
})
watch(() => appStore.terminalBgColor, (c) => {
  const i = BG_COLORS.indexOf(c)
  if (i >= 0) bgIndex.value = i
})
</script>

<style scoped>
.term-quick-actions {
  position: absolute;
  right: 12px;
  bottom: 12px;
  z-index: 30;
  display: flex;
  gap: 6px;
}
.qa-buttons {
  display: flex;
  gap: 6px;
  /* 默认低透明度不挡终端文字，鼠标悬停时恢复完全可见（仅作用于按钮，提示浮层不受影响） */
  opacity: 0.4;
  transition: opacity 0.15s;
}
.qa-buttons:hover {
  opacity: 1;
}
.qa-btn {
  width: 30px;
  height: 30px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 50%;
  border: 1px solid var(--jc-border);
  background: color-mix(in srgb, var(--jc-bg-panel) 75%, transparent);
  color: var(--jc-text-muted);
  cursor: pointer;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.25);
  transition: background 0.15s, color 0.15s, border-color 0.15s;
}
.qa-btn:hover {
  background: var(--jc-bg-hover);
  color: var(--jc-text);
}
/* 已生效态（精简模式/全屏开启中）：强调色描边 */
.qa-btn.qa-active {
  color: var(--jc-accent);
  border-color: var(--jc-accent);
}
/* Agent 模式开启：强调色实心高亮，与普通开启态区分 */
.qa-btn.qa-agent-on {
  background: var(--jc-accent);
  color: #fff;
  border-color: var(--jc-accent);
}
.qa-btn.qa-agent-on:hover {
  background: var(--jc-accent);
  color: #fff;
}

/* 启动引导提示：指向 AI Agent 按钮（完全不透明，独立于半透明按钮组） */
.qa-tip {
  position: absolute;
  bottom: calc(100% + 8px);
  left: 0;
  display: flex;
  align-items: center;
  white-space: nowrap;
  padding: 6px 10px;
  border-radius: 6px;
  background: var(--jc-accent);
  color: #fff;
  font-size: 12px;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.25);
  pointer-events: auto;
  cursor: default;
}
.qa-tip-close {
  margin-left: 8px;
  padding: 0 2px;
  background: transparent;
  border: none;
  color: #fff;
  font-size: 14px;
  line-height: 1;
  cursor: pointer;
  opacity: 0.8;
}
.qa-tip-close:hover { opacity: 1; }
/* Agent 模式开启提示：文案较长，改为右对齐，避免右端溢出窗口被裁剪 */
.qa-tip--on {
  left: auto;
  right: 0;
}
/* 箭头仍对准最左侧的 Agent 按钮中心（按钮组宽 138px，首个按钮中心距左端 15px） */
.qa-tip--on .qa-tip-arrow {
  left: calc(100% - 123px);
}
.qa-tip-arrow {
  position: absolute;
  bottom: -5px;
  left: 15px;
  width: 10px;
  height: 10px;
  background: var(--jc-accent);
  transform: rotate(45deg);
}
/* 断连提示：重连按钮红色高亮 */
.qa-btn.qa-alert {
  color: #f56c6c;
  border-color: #f56c6c;
}

/* ===== 移动端快捷设置面板 ===== */
/* 锚在按钮组正上方（按钮组贴右下角），横向排开三个设置项 */
.qs-panel {
  position: absolute;
  right: 0;
  bottom: calc(100% + 10px);
  display: flex;
  gap: 14px;
  padding: 12px 14px;
  border: 1px solid var(--jc-border);
  border-radius: 14px;
  background: color-mix(in srgb, var(--jc-bg-panel) 96%, transparent);
  box-shadow: 0 8px 24px rgba(0, 0, 0, .38);
}
.qs-item {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  width: 56px;
}
.qs-item__label {
  font-size: 12px;
  line-height: 1;
  color: var(--jc-text-soft);
  white-space: nowrap;
}
.qs-item__val {
  font-size: 12px;
  line-height: 1;
  color: var(--jc-text);
  height: 14px;
  display: flex;
  align-items: center;
}
/* 当前颜色样例（色块） */
.qs-swatch {
  display: block;
  width: 16px;
  height: 12px;
  border-radius: 3px;
  border: 1px solid var(--jc-border-strong);
}
/* 纵向滑杆：EP 的 `.el-slider.is-vertical { height: 100% }` 与组件自身样式同优先度，
   谁在后谁生效不可控 —— 这里直接 !important 锁死高度，保证滑杆足够长、可拖拽 */
.qs-slider {
  height: 224px !important;
}
/* 颜色项：把滑杆轨道画成渐变色带（比单色细线更直观），拖动按钮直接压在其上。
   注意：跑道被加宽后，EP 按默认 6px 跑道算出的按钮/刻度偏移会让它们整体偏左，
   故这里让跑道与按钮、刻度都以「滑杆中心」对齐（margin: 0 auto + left: 50%） */
.qs-item--color :deep(.el-slider__runway) {
  width: 16px;
  margin: 0 auto;
  border-radius: 8px;
  background: var(--qs-grad);
}
.qs-item--color :deep(.el-slider__bar) {
  background: transparent;
}
.qs-item--color :deep(.el-slider__button-wrapper) {
  left: 50%;
  transform: translate(-50%, 50%);
}
.qs-item--color :deep(.el-slider__button) {
  width: 20px;
  height: 20px;
  border-width: 2px;
  box-shadow: 0 1px 4px rgba(0, 0, 0, .45);
}
/* 挡位刻度点：位于色带正中；色带上会被渐变盖住，故用半透明黑保证可见 */
.qs-item--color :deep(.el-slider__stop) {
  left: 50%;
  margin-left: -1.5px;
  transform: translateY(50%);
  background: rgba(0, 0, 0, .35);
  width: 3px;
  height: 3px;
}
</style>
