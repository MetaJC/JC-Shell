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
    class="side-dock shrink-0 relative h-full min-h-0"
    :class="{ 'side-dock--pinned': pinned, 'side-dock--open': pinned || open }"
    :style="{ width: (pinned ? DOCK_WIDTH : HINT_WIDTH) + 'px', '--dock-w': DOCK_WIDTH + 'px' }"
    @mouseenter="onEnter"
    @mouseleave="onLeave"
  >
    <!-- 收起态：贴窗口最左缘的极窄提示条 -->
    <el-tooltip v-if="!pinned" :content="t('dock.hint')" placement="right" :show-after="0" :hide-after="0" :show-arrow="false">
      <div class="side-dock__hint">
        <span class="side-dock__grip" />
      </div>
    </el-tooltip>

    <!-- 展开态：功能条 -->
    <div class="side-dock__panel">
      <!-- 顶部：图钉 → 快捷切换「固定显示 / 自动隐藏」 -->
      <el-tooltip :content="pinned ? t('dock.unpin') : t('dock.pin')" placement="right" :show-after="0" :hide-after="0" :show-arrow="false">
        <button
          type="button"
          class="side-dock__pin"
          :class="{ 'is-on': pinned }"
          @click="togglePin"
        >
          <svg class="side-dock__pin-icon" viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
            <path
              fill="currentColor"
              d="M16 9V4h1c.55 0 1-.45 1-1s-.45-1-1-1H7c-.55 0-1 .45-1 1s.45 1 1 1h1v5c0 1.66-1.34 3-3 3v2h5.97v7l1 1 1-1v-7H19v-2c-1.66 0-3-1.34-3-3z"
            />
          </svg>
        </button>
      </el-tooltip>
      <div class="side-dock__divider" />

      <!-- 功能按钮（纯图标，文字见 tooltip，无延迟立即显示）
           注意：选中态类名必须挂在「普通元素」上，不能挂在 el-tooltip 内部会被克隆的按钮上，
           否则切换页面时旧按钮的选中态可能不刷新，出现两个按钮同时高亮 -->
      <div
        v-for="item in dockItems"
        :key="item.key"
        class="side-dock__slot"
        :class="{ 'is-active': activePage === item.key }"
      >
        <el-tooltip :content="t(item.labelKey)" placement="right" :show-after="0" :hide-after="0" :show-arrow="false">
          <button type="button" class="side-dock__item" @click="onItemClick(item)">
            <!-- 终端：小电脑，屏幕里画 >_ 提示符（终端的经典符号） -->
            <svg
              v-if="item.key === 'terminal'"
              viewBox="0 0 24 24"
              :width="ICON_SIZE"
              :height="ICON_SIZE"
              fill="none"
              stroke="currentColor"
              stroke-width="1.7"
              aria-hidden="true"
            >
              <rect x="2.5" y="4" width="19" height="13" rx="2" />
              <path d="M9 21h6M12 17v4" stroke-linecap="round" />
              <path d="M6.5 8.5l3 2.5-3 2.5" stroke-linecap="round" stroke-linejoin="round" />
              <path d="M12.5 13.5h5" stroke-linecap="round" />
            </svg>
            <el-icon v-else :size="ICON_SIZE"><component :is="item.icon" /></el-icon>
          </button>
        </el-tooltip>
      </div>

      <!-- 设置：与头像菜单「设置」一致，打开统一设置窗口 -->
      <div class="side-dock__slot">
        <el-tooltip :content="t('dock.settings')" placement="right" :show-after="0" :hide-after="0" :show-arrow="false">
          <button type="button" class="side-dock__item" @click="onSettings">
            <el-icon :size="ICON_SIZE"><Setting /></el-icon>
          </button>
        </el-tooltip>
      </div>

      <!-- 底部：用户头像（隐藏昵称，悬停 Tip 显示昵称；点击弹出账户菜单） -->
      <div class="side-dock__divider" />
      <div class="side-dock__user">
        <UserAvatar :collapsed="true" />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 侧边 Dock 栏（窗口最左缘，贯穿全高的竖条）
 * 作者：JC
 * 目前只实现「外观 + 自动隐藏/固定显示」的交互与占位按钮，具体功能后续接入。
 */
import { computed, onBeforeUnmount, ref, type Component } from 'vue'
import { Switch, UploadFilled, Setting } from '@element-plus/icons-vue'
import { useSettingsStore } from '@/stores/settings'
import { useI18nStore } from '@/stores/i18n'
import { useDockStore } from '@/stores/dock'
import { openSettings } from '@/composables/useSettingsOpener'
import UserAvatar from '@/components/user/UserAvatar.vue'
import { SIDE_DOCK_WIDTH as DOCK_WIDTH, SIDE_DOCK_HINT_WIDTH as HINT_WIDTH } from './sideDockConfig'

interface DockItem {
  key: string
  /** i18n key：按钮不显示文字，仅作 tooltip 提示 */
  labelKey: string
  /** 图标组件；terminal 用内联 SVG（带 >_ 提示符的小电脑），可不传 */
  icon?: Component
}
/** 功能图标尺寸：随 Dock 宽度等比适配 */
const ICON_SIZE = Math.round(DOCK_WIDTH * 0.55)
/** 悬停后展开前的停留时间（“稍作停留”才展开，避免鼠标扫过误触） */
const OPEN_DELAY = 280
/** 移出后延迟收起（留足时间把鼠标移回 Dock，避免刚离开就消失） */
const CLOSE_DELAY = 600

const settings = useSettingsStore()
const dockStore = useDockStore()
const t = useI18nStore().t

/** 当前选中的页面（Dock 按钮即页面 Tab，选中项高亮） */
const activePage = computed(() => dockStore.activePage)

/**
 * 固定显示（设置项）：true = 常驻展开并挤压后续布局；false = 自动隐藏。
 * 注：本组件只服务桌面端（移动端由底部 MobileTabBar 承载导航），
 * 所以这里不需要为「触摸端没有 hover」做特判。
 */
const pinned = computed(() => !!settings.settings.sideDockPinned)
/** 自动隐藏模式下是否已展开 */
const open = ref(false)
/** 鼠标是否停在 Dock 上（用于取消固定时判断要不要保持展开） */
const hovering = ref(false)

let openTimer: number | null = null
let closeTimer: number | null = null

function clearTimers() {
  if (openTimer !== null) { window.clearTimeout(openTimer); openTimer = null }
  if (closeTimer !== null) { window.clearTimeout(closeTimer); closeTimer = null }
}

function onEnter() {
  hovering.value = true
  clearTimers()
  if (pinned.value || open.value) return
  openTimer = window.setTimeout(() => { openTimer = null; open.value = true }, OPEN_DELAY)
}

function onLeave() {
  hovering.value = false
  clearTimers()
  if (pinned.value) return
  closeTimer = window.setTimeout(() => { closeTimer = null; open.value = false }, CLOSE_DELAY)
}

/** 顶部图钉：切换「固定显示 / 自动隐藏」（写入设置，自动持久化 + 跨窗口同步） */
function togglePin() {
  const next = !pinned.value
  settings.patchSettings({ sideDockPinned: next })
  // 取消固定时鼠标若还停在 Dock 上，保持展开，避免“一点就消失”
  if (!next && hovering.value) open.value = true
}

/** 切换页面：Dock 按钮即页面 Tab */
function onItemClick(item: DockItem) {
  dockStore.setActivePage(item.key)
}

/** 设置：打开统一设置窗口（与头像菜单「设置」、Ctrl+, 完全一致） */
function onSettings() {
  void openSettings()
}

/** 页面列表（后续新增页面：这里加一项 + 在 components/pages/index.ts 登记组件） */
const dockItems: DockItem[] = [
  { key: 'terminal', labelKey: 'dock.terminal' },
  { key: 'ftp', labelKey: 'dock.ftp', icon: UploadFilled },
  { key: 'portForward', labelKey: 'dock.portForward', icon: Switch },
]

onBeforeUnmount(clearTimers)
</script>

<style scoped>
.side-dock {
  /* 宽度由内联样式驱动（收起 8px / 固定 40px），这里只做过渡；
     overflow 必须可见，自动隐藏展开时功能条才能盖到右侧面板上 */
  transition: width 0.18s ease;
  overflow: visible;
  /* 注意：z-index 会创建层叠上下文，头像菜单的 z-index:200 被关在本上下文内、
     对外按本值参与比较。必须高于终端区的所有浮动层（状态条/快捷按钮 30、
     agent ghost 渲染层 100），否则菜单会被它们盖住；仍低于锁屏/EP 弹层(9999+)。 */
  z-index: 200;
}

/* ===== 收起态：贴窗口最左缘的极窄提示条 ===== */
.side-dock__hint {
  position: absolute;
  top: 0;
  left: 0;
  width: 8px;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  background: linear-gradient(to right, var(--jc-bg-soft), transparent);
  border-right: 1px solid var(--jc-border);
  transition: opacity 0.15s ease, background 0.15s ease;
}
.side-dock__hint:hover {
  background: linear-gradient(to right, var(--jc-bg-hover), transparent);
}
.side-dock__grip {
  width: 3px;
  height: 40px;
  border-radius: 2px;
  background: var(--jc-text-muted);
  opacity: 0.55;
}
.side-dock__hint:hover .side-dock__grip {
  background: var(--jc-accent);
  opacity: 0.9;
}
.side-dock--open .side-dock__hint {
  opacity: 0;
  pointer-events: none;
}

/* ===== 展开态：功能条（宽度与 Dock 同源，避免不一致导致遮挡） =====
   自动隐藏时：绝对定位锚在左缘，展开后向右覆盖左侧栏一点（不触发终端重排）
   固定显示时：Dock 自身占宽，功能条与之等宽、不越界 → 只挤压后续布局，不遮挡内容 */
.side-dock__panel {
  position: absolute;
  top: 0;
  left: 0;
  width: var(--dock-w, 40px);
  height: 100%;
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  padding: 8px 4px;
  background: var(--jc-bg-panel);
  border-right: 1px solid var(--jc-border);
  box-shadow: 6px 0 16px rgba(0, 0, 0, 0.18);
  opacity: 0;
  transform: translateX(-6px);
  pointer-events: none;
  transition: opacity 0.16s ease, transform 0.16s ease;
}
.side-dock--open .side-dock__panel {
  opacity: 1;
  transform: none;
  pointer-events: auto;
}
/* 固定显示时不再有阴影（此时它已占据布局，不需要浮层阴影） */
.side-dock--pinned .side-dock__panel {
  box-shadow: none;
}

/* ===== 顶部图钉：固定 / 自动隐藏（只占一行，容器高度压到最小） ===== */
.side-dock__pin {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 100%;
  height: 24px;
  padding: 0;
  border: 0;
  border-radius: 5px;
  background: transparent;
  color: var(--jc-text-muted);
  cursor: pointer;
  transition: background 0.15s ease, color 0.15s ease;
}
.side-dock__pin:hover {
  background: var(--jc-bg-hover);
  color: var(--jc-text);
}
.side-dock__pin.is-on {
  color: var(--jc-accent);
}
.side-dock__pin-icon {
  /* 未固定：图钉斜插；固定：图钉竖直 */
  transform: rotate(45deg);
  transition: transform 0.18s ease;
}
.side-dock__pin.is-on .side-dock__pin-icon {
  transform: none;
}
.side-dock__divider {
  width: 100%;
  height: 1px;
  background: var(--jc-border);
}

/* ===== 功能按钮（纯图标，同时是页面 Tab） ===== */
/* 每个按钮的包裹层：选中态挂在它上面（普通元素 + v-for key，切换时可靠刷新） */
.side-dock__slot {
  width: 100%;
  display: flex;
}
.side-dock__item {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 100%;
  padding: 8px 0;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: var(--jc-text);
  cursor: pointer;
  transition: background 0.15s ease, color 0.15s ease;
}
.side-dock__item:hover {
  background: var(--jc-bg-hover);
  color: var(--jc-accent);
}
.side-dock__item:active {
  background: var(--jc-bg-soft);
}
/* 当前选中的页面：只用底色 + 强调色高亮（不画左侧竖线） */
.side-dock__slot.is-active .side-dock__item {
  background: var(--jc-bg-hover);
  color: var(--jc-accent);
}

/* ===== 底部用户头像 ===== */
/* margin-top:auto 把头像推到功能条最底部；头像默认 36px，缩到 30px 适配 40px 宽的 Dock */
.side-dock__user {
  margin-top: auto;
  display: flex;
  justify-content: center;
  padding: 4px 0 2px;
}
.side-dock__user :deep(.avatar-btn) {
  width: 30px;
  height: 30px;
}
.side-dock__user :deep(.avatar-btn svg) {
  width: 18px;
  height: 18px;
}
</style>
