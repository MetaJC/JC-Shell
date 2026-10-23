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
  <!-- 注意：根元素前不要放注释/其它节点，必须保持单根元素，否则外层 v-show / class 透传会失效 -->
  <div class="flex flex-1 min-w-0 min-h-0 zen-no-drag">
    <!-- 左 Dock（精简/全屏模式隐藏；移动端不挂载，见 canDockPanels） -->
    <template v-if="!chromeHidden && canDockPanels">
      <aside
        v-if="layout.leftPanelId"
        :style="{ width: leftAsideWidth }"
        class="relative shrink-0 border-r border-[var(--jc-border)] overflow-hidden flex flex-col panel-aside is-left"
        :class="{ 'border-r-0': layout.leftCollapsed, 'is-collapsed': layout.leftCollapsed }"
      >
        <div class="dock-panels-wrap flex-1 min-h-0 overflow-hidden">
          <DockPanels
            :panel-id="layout.leftPanelId"
            dock="left"
            :collapsed="layout.leftCollapsed"
            :connected="activeConnected"
            :hostname="activeHostname"
            :session-id="activeTab?.sessionId"
            :kind="activeTab?.kind"
            @open="(h: HostItem) => emit('openHost', h)"
            @collapse="layout.toggleLeftCollapsed"
          />
        </div>
        <!-- 启动弹窗（小弹窗模式）：贴面板底部，宽度=面板宽度，高度向上延伸 -->
        <LaunchPop v-if="!layout.leftCollapsed" />
      </aside>
      <SplitBar v-if="layout.leftPanelId" orientation="vertical" :inactive="layout.leftCollapsed" @drag="onLeftResize" />
    </template>

    <!-- 中央：终端工作区（插槽）+ 底部文件系统面板 -->
    <div ref="centerRef" class="flex-1 flex flex-col min-w-0 min-h-0">
      <div
        :style="{ height: (bottomVisible && !layout.bottomCollapsed && !chromeHidden) ? layout.centerTopPercent + '%' : '100%' }"
        class="dock-center-top flex flex-col min-h-0"
      >
        <slot />
      </div>

      <template v-if="!chromeHidden && canDockPanels">
        <SplitBar v-if="bottomVisible" orientation="horizontal" :inactive="layout.bottomCollapsed" @drag="onCenterResize" />

        <!-- Bottom Dock：折叠态保留 PanelHeader + 图标行 -->
        <div
          v-if="bottomVisible"
          :style="{ height: bottomHeight }"
          class="dock-bottom min-h-0 flex flex-col overflow-hidden panel-aside"
          :class="{ 'bg-[var(--jc-bg-soft)]': layout.bottomCollapsed, 'is-collapsed': layout.bottomCollapsed }"
        >
          <div class="dock-panels-wrap flex-1 min-h-0 overflow-hidden">
            <DockPanels
              :panel-id="layout.bottomPanelId"
              dock="bottom"
              :collapsed="layout.bottomCollapsed"
              :connected="activeConnected"
              :hostname="activeHostname"
              :session-id="activeTab?.sessionId"
              :kind="activeTab?.kind"
              @open="(h: HostItem) => emit('openHost', h)"
              @collapse="layout.toggleBottomCollapsed"
            />
          </div>
        </div>
      </template>
    </div>

    <!-- 右 Dock（精简/全屏模式隐藏；移动端不挂载） -->
    <template v-if="!chromeHidden && canDockPanels">
      <SplitBar v-if="layout.rightPanelId" orientation="vertical" :inactive="layout.rightCollapsed" @drag="onRightResize" />
      <aside
        v-if="layout.rightPanelId"
        :style="{ width: rightAsideWidth }"
        class="shrink-0 border-l border-[var(--jc-border)] overflow-hidden flex flex-col panel-aside is-right"
        :class="{ 'bg-[var(--jc-bg-soft)] border-l-0': layout.rightCollapsed, 'is-collapsed': layout.rightCollapsed }"
      >
        <div class="dock-panels-wrap flex-1 min-h-0 overflow-hidden">
          <DockPanels
            :panel-id="layout.rightPanelId"
            dock="right"
            :collapsed="layout.rightCollapsed"
            :connected="activeConnected"
            :hostname="activeHostname"
            :session-id="activeTab?.sessionId"
            :kind="activeTab?.kind"
            @open="(h: HostItem) => emit('openHost', h)"
            @collapse="layout.toggleRightCollapsed"
          />
        </div>
      </aside>
    </template>
  </div>
</template>

<script setup lang="ts">
/**
 * 终端页
 * 作者：JC
 * 从 DockLayout 抽出的三面板布局，独立成「页面」以便复用：
 * 终端工作区由调用方通过默认插槽传入，其余面板按 layout store 的停靠配置渲染。
 */
import { computed, ref } from 'vue'
import { useLayoutStore } from '@/stores/layout'
import { useTerminalStore } from '@/stores/terminal'
import { useActiveSession } from '@/composables/useActiveSession'
import { CAP } from '@/utils/platform'
import SplitBar from '@/components/common/SplitBar.vue'
import DockPanels from '@/components/layout/DockPanels.vue'
import LaunchPop from '@/components/launch/LaunchPop.vue'
import type { HostItem } from '@/stores/host'

/** 精简/全屏模式：隐藏所有停靠面板 */
defineProps<{ chromeHidden?: boolean }>()
const emit = defineEmits<{ (e: 'openHost', host: HostItem): void }>()

const layout = useLayoutStore()
const termStore = useTerminalStore()

/**
 * 是否挂载三块停靠面板（会话列表 / 设备信息 / 文件系统）。
 * 移动端为 false：这三块改由底部导航的独立页面承载，这里**不挂载**而不是靠 CSS 隐藏 ——
 * 面板内有轮询（设备指标、SFTP 目录同步），隐藏后仍会持续跑，白耗电与流量。
 */
const canDockPanels = CAP.dockPanels()

const centerRef = ref<HTMLElement>()
const bottomPanelPresent = computed(() => !!layout.bottomPanelId)
/** 分屏时隐藏底部文件系统面板（每个分屏单元已内嵌独立 fs，避免重复） */
const bottomVisible = computed(() =>
  bottomPanelPresent.value &&
  !(termStore.splitMode !== 'none' && layout.bottomPanelId === 'fileSystem')
)

// ===== 分隔条拖拽（写入 store 并持久化）=====
function onLeftResize(delta: number) {
  const v = Math.max(140, Math.min(400, layout.leftWidth + delta))
  layout.setLeftWidth(v)
}
function onRightResize(delta: number) {
  const v = Math.max(200, Math.min(480, layout.rightWidth - delta))
  layout.setRightWidth(v)
}
function onCenterResize(delta: number) {
  const h = centerRef.value?.clientHeight ?? 600
  const pct = (delta / h) * 100
  layout.setCenterTopPercent(Math.max(20, Math.min(80, layout.centerTopPercent + pct)))
}

// ===== 当前激活 Tab 的会话上下文（面板据此加载数据）=====
// 判定逻辑抽在 composable 里：移动端的「SFTP / 仪表盘」页共用同一份实现，避免两边跑偏。
const { activeTab, connected: activeConnected, hostname: activeHostname } = useActiveSession()

// ===== 折叠态尺寸：与折叠按钮（24px）完全一致，无任何额外边距 =====
const leftAsideWidth = computed(() => layout.leftCollapsed ? '24px' : layout.leftWidth + 'px')
const rightAsideWidth = computed(() => layout.rightCollapsed ? '24px' : layout.rightWidth + 'px')
const bottomHeight = computed(() => layout.bottomCollapsed ? '24px' : (100 - layout.centerTopPercent) + '%')
</script>

<style scoped>
/* 折叠/展开过渡：宽度、高度平滑变化。
   注意：这组规则必须写在本组件（.panel-aside/.dock-bottom/.dock-center-top 都在这里的模板里）；
   之前写在 DockLayout.vue 的 scoped 里是不生效的（scope 属性到不了这些元素）。
   拖拽分隔条时的 transition:none 由全局规则 body.split-dragging 统一禁用。 */
.panel-aside { transition: width 0.2s ease; }
.dock-bottom,
.dock-center-top { transition: height 0.2s ease; }
</style>
