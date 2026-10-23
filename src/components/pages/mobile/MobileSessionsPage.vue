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
  <div class="m-page">
    <!-- 复用桌面端的会话列表面板组件：工具栏 / 分组树 / 右键菜单等能力完全一致 -->
    <HostListPanel class="m-page__fill" dock="left" :collapsed="false" @open="onOpenHost" />
  </div>
</template>

<script setup lang="ts">
/**
 * 移动端「会话」页（底部导航第 1 项）
 * 作者：JC
 *
 * 会话列表在桌面端是终端页的左停靠面板，移动端把整屏让给它（屏幕窄，面板并列不可用）。
 * 复用 HostListPanel 而不是另写一套列表，保证增删改查 / 分组 / 导入导出等能力不会两边分叉。
 */
import { inject } from 'vue'
import HostListPanel from '@/components/host/HostListPanel.vue'
import { useDockStore } from '@/stores/dock'
import { OPEN_HOST_KEY } from '@/composables/useOpenHost'
import type { HostItem } from '@/stores/host'

const dockStore = useDockStore()
/** 由 DockLayout 注入的连接入口（最终交给 MainWindow → TerminalWorkspace 开标签） */
const openHost = inject(OPEN_HOST_KEY, () => { /* 脱离 DockLayout 时忽略 */ })

/** 点会话：先真正连上，再切到「终端」页 —— 移动端主流程就是「选会话 → 看终端」 */
function onOpenHost(host: HostItem): void {
  openHost(host)
  dockStore.setActivePage('terminal')
}
</script>
