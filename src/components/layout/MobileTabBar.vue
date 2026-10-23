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
  <!--
    移动端底部导航（手机 App 惯用形态）
    仅移动端渲染，由 DockLayout 按平台切换（桌面端仍是左侧 SideDock 竖条，互不影响）。
    activePage 与桌面 Dock 共用同一个 dock store，所以「哪一页在前台」只有一份真相。
  -->
  <nav class="m-tabbar">
    <button
      v-for="item in items"
      :key="item.key"
      class="m-tabbar__item"
      :class="{ 'is-active': activePage === item.key }"
      :aria-current="activePage === item.key ? 'page' : undefined"
      @click="onSelect(item)"
    >
      <el-icon :size="22" class="m-tabbar__icon"><component :is="item.icon" /></el-icon>
      <span class="m-tabbar__label">{{ t(item.labelKey) }}</span>
    </button>
  </nav>
</template>

<script setup lang="ts">
/**
 * 移动端底部导航栏
 * 作者：JC
 *
 * 5 个入口（顺序与产品定义一致）：
 *   会话  —— 会话列表，连接入口；点会话即连上并切到终端
 *   终端  —— 终端页，当前前台终端的完整视图
 *   SFTP  —— 该主机的文件管理，默认打开「当前选中终端」对应主机的目录，并与终端目录同步
 *   仪表盘 —— 设备信息看板，同样跟随当前选中终端
 *   我的  —— 账号 / 设置 / 关于等入口聚合
 *
 * 实现要点：与桌面端 SideDock 共用同一个 dock store（`activePage` 是「哪一页在前台」的唯一真相），
 * 因此两个平台下页面切换语义完全一致，只是导航形态不同（底部横排 vs 左侧竖排）。
 */
import { computed, markRaw, type Component } from 'vue'
import { ChatLineSquare, FolderOpened, Monitor, Odometer, User } from '@element-plus/icons-vue'
import { useDockStore } from '@/stores/dock'
import { useI18nStore } from '@/stores/i18n'

interface TabItem {
  key: string
  /** i18n key */
  labelKey: string
  icon: Component
}

const dockStore = useDockStore()
const t = useI18nStore().t

const activePage = computed(() => dockStore.activePage)

/** 图标用 markRaw：图标组件是常量，不需要参与响应式代理。
 *  注意：顺序必须与 dock store 的 MOBILE_PAGE_ORDER 一致 —— 左右滑动切页按那份顺序取相邻页 */

const items: TabItem[] = [
  { key: 'sessions', labelKey: 'mnav.sessions', icon: markRaw(ChatLineSquare) },
  { key: 'terminal', labelKey: 'mnav.terminal', icon: markRaw(Monitor) },
  { key: 'sftp', labelKey: 'mnav.sftp', icon: markRaw(FolderOpened) },
  { key: 'dashboard', labelKey: 'mnav.dashboard', icon: markRaw(Odometer) },
  { key: 'profile', labelKey: 'mnav.profile', icon: markRaw(User) },
]

function onSelect(item: TabItem): void {
  // 切换底部导航页时收起可能存在的子页（如正在「新建会话」整页中直接点「终端」）
  dockStore.clearMobileSubPages()
  dockStore.setActivePage(item.key)
}
</script>
