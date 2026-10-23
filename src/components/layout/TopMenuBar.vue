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
    class="top-menu-bar flex items-center text-sm select-none"
    :class="{ 'is-drag-region': isMacOrLinux }"
    @click="closeAll"
  >
    <!-- 左上角 Logo -->
    <img class="top-logo" src="@/assets/logo.png" alt="JC Shell Logo" @click="onLogoClick" />
    <div
      v-for="(menu, mi) in menus"
      :key="menu.key"
      :ref="el => setMenuRef(mi, el as HTMLElement)"
      class="menu-item group relative px-3 py-1 rounded cursor-pointer transition-colors"
      :class="{ 'is-open': openKey === menu.key }"
      @click.stop="onTopMenuClick(menu)"
      @mouseenter="onItemHover(menu)"
    >
      {{ t(menu.label) }}
      <!--
        QQ 群菜单右侧："加群领福利" 提示徽章。
        - 仅首次启动（未主动关闭过）时显示
        - 用户点击右侧 × 后写入 localStorage，永不再示
        - 鼠标悬停徽章内容时展示 Tip 提示
      -->
      <span
        v-if="menu.key === 'qq' && showQqGroupTip"
        class="qq-tip ml-2 inline-flex items-center"
        @click.stop
      >
        <el-tooltip
          content="限时活动：新用户加群即可领取 AI Agent 积分，可用于 Agent 使用模式。"
          placement="bottom"
          :show-after="200"
          :hide-after="0"
          raw-content
        >
          <span class="qq-tip-text">加群领福利</span>
        </el-tooltip>
        <el-icon class="qq-tip-close" @click.stop="closeQqGroupTip" title="关闭提示">
          <Close />
        </el-icon>
      </span>
    </div>
  </div>
  <Teleport to="body">
    <div
      v-for="menu in menus"
      :key="'dd-' + menu.key"
      v-show="openKey === menu.key"
      class="top-menu-dropdown fixed z-[2000] border rounded shadow-lg py-1 min-w-[200px]"
      :style="dropdownStyle(menu.key)"
      @click.stop
    >
      <template v-for="item in itemsOf(menu)" :key="item.key">
        <div v-if="item.divider" class="h-px my-1 mx-2" style="background: var(--jc-border)" />
        <div
          v-else
          class="dropdown-item px-3 py-1.5 flex justify-between items-center"
          :class="{ 'is-disabled': item.disabled }"
          @click="onItemClick(item)"
        >
          <span class="flex items-center gap-2">
            <el-icon v-if="item.icon" :size="13"><component :is="item.icon" /></el-icon>
            {{ t(item.label) }}
          </span>
          <span v-if="item.shortcut" class="text-xs opacity-50">{{ item.shortcut }}</span>
        </div>
      </template>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * 顶部菜单栏（文件/编辑/视图/终端/工具/帮助）
 * 作者：JC
 * 点击展开下拉菜单，hover 切换；下拉菜单 Teleport 到 body 避免裁剪。
 */
import { ref, onMounted, onBeforeUnmount } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useI18nStore } from '@/stores/i18n'
import { useAppStore } from '@/stores/app'
import { useLayoutStore } from '@/stores/layout'
import { isTauriEnv } from '@/api'
import { CAP, type Capability } from '@/utils/platform'
import {
  Plus, Close, Switch,
  CopyDocument, Document, Delete,
  Fold, Files, Monitor, FullScreen, ZoomIn, ZoomOut, RefreshRight,
  Operation, Connection, Download,
  Setting, Menu as MenuIcon, Sort,
  InfoFilled, Key, Upload, Message,
} from '@element-plus/icons-vue'
import type { Component } from 'vue'

const emit = defineEmits<{
  (e: 'action', key: string): void
}>()

const t = useI18nStore().t
const app = useAppStore()
const layout = useLayoutStore()

interface MenuItem {
  key: string
  label: string
  shortcut?: string
  icon?: Component
  disabled?: boolean
  divider?: boolean
  /** 声明该菜单项依赖的能力；当前平台不具备时自动隐藏（见 itemsOf） */
  cap?: Capability
}
interface MenuGroup {
  key: string
  label: string
  items: MenuItem[]
  /** 无子菜单的一级菜单（如「Q群」）：点击直接触发该 action，而不是展开空下拉 */
  action?: string
}

const menus: MenuGroup[] = [
  {
    key: 'file', label: 'menu.file',
    items: [
      { key: 'file:newTab', label: 'menu.file.newTab', shortcut: 'Ctrl+T', icon: Plus, cap: 'localTerminal' },
      { key: 'file:closeTab', label: 'menu.file.closeTab', shortcut: 'Ctrl+W', icon: Close },
      { key: 'd1', label: '', divider: true },
      { key: 'file:exit', label: 'menu.file.exit', icon: Switch },
    ]
  },
  {
    key: 'edit', label: 'menu.edit',
    items: [
      { key: 'edit:copy', label: 'menu.edit.copy', shortcut: 'Ctrl+C', icon: CopyDocument },
      { key: 'edit:paste', label: 'menu.edit.paste', shortcut: 'Ctrl+V', icon: Document },
      { key: 'edit:selectAll', label: 'menu.edit.selectAll', shortcut: 'Ctrl+A', icon: Document },
      { key: 'd2', label: '', divider: true },
      { key: 'edit:clear', label: 'menu.edit.clear', shortcut: 'Ctrl+L', icon: Delete },
    ]
  },
  {
    key: 'view', label: 'menu.view',
    items: [
      { key: 'view:toggleSidebar', label: 'menu.view.toggleSidebar', shortcut: 'Ctrl+B', icon: Fold },
      { key: 'view:toggleFilePanel', label: 'menu.view.toggleFilePanel', icon: Files },
      { key: 'view:toggleDevicePanel', label: 'menu.view.toggleDevicePanel', icon: Monitor },
      { key: 'd3', label: '', divider: true },
      { key: 'view:splitNone', label: 'menu.view.splitNone', icon: RefreshRight },
      { key: 'view:splitH', label: 'menu.view.splitH', icon: Sort },
      { key: 'view:splitV', label: 'menu.view.splitV', icon: Sort },
      { key: 'd4', label: '', divider: true },
      { key: 'view:fullscreen', label: 'menu.view.fullscreen', shortcut: 'F11', icon: FullScreen },
      { key: 'view:zoomIn', label: 'menu.view.zoomIn', shortcut: 'Ctrl++', icon: ZoomIn },
      { key: 'view:zoomOut', label: 'menu.view.zoomOut', shortcut: 'Ctrl+-', icon: ZoomOut },
      { key: 'view:zoomReset', label: 'menu.view.zoomReset', icon: RefreshRight },
    ]
  },
  {
    key: 'terminal', label: 'menu.terminal',
    items: [
      { key: 'terminal:newLocal', label: 'menu.terminal.newLocal', shortcut: 'Ctrl+T', icon: Operation, cap: 'localTerminal' },
      { key: 'terminal:newSsh', label: 'menu.terminal.newSsh', icon: Connection },
      { key: 'terminal:reconnect', label: 'menu.terminal.reconnect', icon: RefreshRight },
      { key: 'd5', label: '', divider: true },
      { key: 'terminal:clear', label: 'menu.terminal.clear', shortcut: 'Ctrl+L', icon: Delete },
      { key: 'terminal:exportLog', label: 'menu.terminal.exportLog', icon: Download },
    ]
  },
  {
    key: 'tools', label: 'menu.tools',
    items: [
      { key: 'tools:settings', label: 'menu.tools.settings', shortcut: 'Ctrl+,', icon: Setting },
      { key: 'tools:hostManager', label: 'menu.tools.hostManager', icon: MenuIcon },
      { key: 'tools:transferQueue', label: 'menu.tools.transferQueue', icon: Sort },
      { key: 'd7', label: '', divider: true },
      // 图标取「数据相对本应用的流向」：导入 = 进来（↓），导出 = 送出（↑）
      { key: 'tools:importSessions', label: 'menu.tools.importSessions', icon: Download },
      { key: 'tools:exportSessions', label: 'menu.tools.exportSessions', icon: Upload },
    ]
  },
  {
    key: 'help', label: 'menu.help',
    items: [
      { key: 'help:shortcuts', label: 'menu.help.shortcuts', shortcut: 'Ctrl+/', icon: Key },
      { key: 'help:feedback', label: 'menu.help.feedback', icon: Message },
      { key: 'd6', label: '', divider: true },
      { key: 'help:checkUpdate', label: 'menu.help.checkUpdate', icon: Upload },
      { key: 'help:about', label: 'menu.help.about', icon: InfoFilled },
    ]
  },
  {
    // 无子菜单的一级菜单（置于最右）：点击直接打开 QQ 群弹窗。
    // action 沿用历史 id `help:qqGroups`，与 Rust 原生菜单（macOS）emit 的 id 保持一致。
    key: 'qq', label: 'menu.qq', action: 'help:qqGroups', items: []
  }
]

/**
 * 按能力过滤菜单项：声明了 cap 且当前平台不具备该能力的项直接隐藏。
 * 好处：平台差异收敛在数据里，模板不含任何平台条件判断（桌面端 cap 全为 true，行为不变）。
 */
function itemsOf(menu: MenuGroup): MenuItem[] {
  return menu.items.filter((i) => (i.cap ? CAP[i.cap]() : true))
}

const openKey = ref('')
const menuRefs: HTMLElement[] = []

/**
 * "加群领福利" 提示徽章：
 *  - 仅在 Tauri 桌面环境、且用户从未主动关闭过时显示
 *  - 用户点 × 后写入 localStorage（永不再示）
 */
const QQ_GROUP_TIP_KEY = 'jc_shell_qq_group_tip_dismissed_v1'
const showQqGroupTip = ref(false)

function closeQqGroupTip(): void {
  showQqGroupTip.value = false
  try { localStorage.setItem(QQ_GROUP_TIP_KEY, '1') } catch { /* ignore */ }
}

/**
 * 平台分流：Mac / Linux 启用菜单栏区域作为窗口拖拽区（系统级无边框窗口惯例）；
 * Windows 保持原状，由父级 .title-bar-drag-area 槽位提供拖拽。
 * 使用同步检测避免首帧闪烁；UA 判定在 Tauri WebView 中稳定可靠。
 */
const isMacOrLinux =
  typeof navigator !== 'undefined' &&
  /Mac|Linux/i.test(navigator.userAgent) &&
  !/Windows/i.test(navigator.userAgent)

function setMenuRef(idx: number, el: HTMLElement | null) {
  if (el) menuRefs[idx] = el
}

function toggle(key: string) {
  if (openKey.value === key) {
    openKey.value = ''
  } else {
    openKey.value = key
  }
}

/**
 * 一级菜单点击：无子菜单的（如「Q群」）直接触发动作打开对应面板，
 * 其余按原逻辑展开 / 收起下拉。
 */
function onTopMenuClick(menu: MenuGroup) {
  if (menu.action) {
    closeAll()
    emit('action', menu.action)
    return
  }
  toggle(menu.key)
}

function onItemHover(menu: MenuGroup) {
  // 无子菜单的一级菜单（如「Q群」）不响应 hover 展开，否则会弹出一个空下拉
  if (!menu.items.length) return
  if (openKey.value && openKey.value !== menu.key) {
    openKey.value = menu.key
  }
}

function closeAll() { openKey.value = '' }

/** 点击 Logo：显示主菜单（系统级 Alt+F4 替代：最小化到托盘） */
async function onLogoClick() {
  try {
    const win = getCurrentWindow()
    await win.minimize()
  } catch { /* ignore */ }
}

function dropdownStyle(key: string) {
  const idx = menus.findIndex(m => m.key === key)
  const el = menuRefs[idx]
  if (!el) return { left: '0px', top: '0px', background: 'var(--jc-bg-soft)', borderColor: 'var(--jc-border)' }
  const rect = el.getBoundingClientRect()
  return {
    left: rect.left + 'px',
    top: (rect.bottom + 2) + 'px',
    background: 'var(--jc-bg-soft)',
    borderColor: 'var(--jc-border)',
  }
}

function onItemClick(item: MenuItem) {
  if (item.disabled || item.divider) return
  openKey.value = ''
  switch (item.key) {
    case 'view:toggleSidebar':
      app.toggleSidebar()
      return
    case 'view:toggleFilePanel':
      layout.toggleBottomCollapsed()
      return
    case 'view:toggleDevicePanel':
      layout.toggleRightCollapsed()
      return
    case 'tools:settings':
      // 走 emit('action')，由 MainWindow 统一处理：主窗口内打开设置弹窗（保证 100% 生效）
      emit('action', 'tools:settings')
      return
    case 'view:fullscreen':
      toggleFullscreen()
      return
    case 'file:exit':
      getCurrentWindow().close()
      return
  }
  emit('action', item.key)
}

async function toggleFullscreen() {
  const win = getCurrentWindow()
  const isFull = await win.isFullscreen()
  await win.setFullscreen(!isFull)
}

function onGlobalDown(e: MouseEvent) {
  const dropdowns = document.querySelectorAll('.top-menu-dropdown')
  const items = document.querySelectorAll('.menu-item')
  const targets = [...dropdowns, ...items]
  const node = e.target as Node | null
  if (node && !targets.some(t => t.contains(node))) {
    openKey.value = ''
  }
}
function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') openKey.value = ''
}

onMounted(() => {
  window.addEventListener('mousedown', onGlobalDown, true)
  window.addEventListener('keydown', onKey, true)
  window.addEventListener('resize', () => { openKey.value = '' }, { passive: true })

  // 仅 Tauri 桌面环境、未关闭过该提示时才显示"加群领福利"徽章
  if (isTauriEnv()) {
    try {
      if (!localStorage.getItem(QQ_GROUP_TIP_KEY)) {
        showQqGroupTip.value = true
      }
    } catch { /* ignore (localStorage 不可用等) */ }
  }
})
onBeforeUnmount(() => {
  window.removeEventListener('mousedown', onGlobalDown, true)
  window.removeEventListener('keydown', onKey, true)
})
</script>

<style scoped>
.top-menu-bar {
  -webkit-app-region: no-drag;
  pointer-events: auto;
}
/* Mac / Linux：菜单栏区域作为窗口拖拽区（菜单项、Logo 仍由内部 no-drag 覆盖为可点击） */
.top-menu-bar.is-drag-region {
  -webkit-app-region: drag;
}
.top-logo {
  width: 24px;
  height: 24px;
  margin: 0 8px 0 4px;
  cursor: pointer;
  object-fit: contain;
  -webkit-app-region: no-drag;
  pointer-events: auto;
}
.menu-item {
  -webkit-app-region: no-drag;
  pointer-events: auto;
  color: var(--jc-text);
}
.menu-item:hover, .menu-item.is-open {
  background: var(--jc-bg-hover);
}

/* QQ 群菜单右侧"加群领福利"徽章（首次启动才显示，关闭后永不再示） */
.qq-tip {
  font-size: 11px;
  color: #ff7a45;
  background: rgba(255, 122, 69, 0.12);
  border: 1px solid rgba(255, 122, 69, 0.3);
  border-radius: 10px;
  padding: 1px 4px 1px 8px;
  line-height: 1.5;
  cursor: default;
  transition: background 0.15s;
  white-space: nowrap;
  -webkit-app-region: no-drag;
}
.qq-tip:hover { background: rgba(255, 122, 69, 0.20); }
.qq-tip-text { user-select: none; }
.qq-tip-close {
  margin-left: 4px;
  cursor: pointer;
  opacity: 0.55;
  transition: opacity 0.15s;
  font-size: 12px;
  display: inline-flex;
  align-items: center;
}
.qq-tip-close:hover { opacity: 1; }
</style>

<style>
.top-menu-dropdown {
  -webkit-app-region: no-drag;
  pointer-events: auto;
}
.top-menu-dropdown .dropdown-item {
  color: var(--jc-text);
  cursor: pointer;
  white-space: nowrap;
  transition: background 0.1s;
}
.top-menu-dropdown .dropdown-item:hover {
  background: var(--jc-bg-hover);
}
.top-menu-dropdown .dropdown-item.is-disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
.top-menu-dropdown .dropdown-item.is-disabled:hover {
  background: transparent;
}
</style>
