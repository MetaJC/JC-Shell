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
  <div class="panel-header" :class="[`panel-header--${dock}`, { 'panel-header--collapsed': collapsed }]">
    <!-- 拖拽手柄（六点）：left/bottom dock 位于最左；right dock 位于最右；折叠态隐藏 -->
    <PanelGrip v-if="panelId && !collapsed" :panel-id="panelId" class="panel-header__grip shrink-0" />
    <!-- 中间工具栏区：窗口变窄时仅此处图标收入溢出菜单，两侧按钮位置恒定 -->
    <div class="panel-header__center flex-1 min-w-0 flex items-center overflow-hidden">
      <slot />
    </div>
    <!-- 折叠/展开按钮：left/bottom dock 位于最右；right dock 位于最左。折叠态始终显示；
         hideCollapse 时隐藏（如 FTP 分屏内嵌面板，折叠无意义） -->
    <el-button
      v-if="!hideCollapse"
      size="small"
      text
      class="panel-header__collapse !h-6 !w-6 !p-0 shrink-0"
      :title="collapsed ? '展开面板' : '折叠面板'"
      @click="$emit('collapse')"
    >
      <el-icon size="13">
        <Expand v-if="collapsed" />
        <Fold v-else />
      </el-icon>
    </el-button>
  </div>
</template>

<script setup lang="ts">
/**
 * 面板统一头部（工具栏行）
 * 作者：JC
 * 按钮位置规则：
 * - 折叠按钮：面板位于最右侧（right dock）→ 最左；其余（left/bottom）→ 最右
 * - 拖拽按钮（grip）：与折叠按钮相反；折叠态隐藏
 * - 折叠按钮固定不隐藏，窄窗口时仅中间工具栏图标收纳
 */
import { Fold, Expand } from '@element-plus/icons-vue'
import PanelGrip from '@/components/common/PanelGrip.vue'
import type { PanelId, Dock } from '@/stores/layout'

defineProps<{
  /** 面板 ID（提供则显示拖拽手柄） */
  panelId?: PanelId
  /** 所在 dock 方向，决定按钮排布 */
  dock: Dock
  /** 当前是否折叠（决定按钮图标 Fold/Expand） */
  collapsed: boolean
  /** 隐藏折叠/展开按钮（内嵌分屏等折叠无意义的场景） */
  hideCollapse?: boolean
}>()

defineEmits<{ (e: 'collapse'): void }>()
</script>

<style scoped>
.panel-header {
  display: flex;
  align-items: center;
  gap: 2px;
  height: 32px;
  padding: 0 6px;
  flex-shrink: 0;
  background-color: var(--jc-bg-soft);
  border-bottom: 1px solid var(--jc-border);
  /* 折叠/展开时头部 32px ↔ 24px 平滑变化，不再硬跳 */
  transition: height 0.2s ease, padding 0.2s ease, gap 0.2s ease;
}
/* 折叠态：高度与内边距均归零到按钮尺寸，头部与折叠按钮（24px）完全一致 */
.panel-header--collapsed {
  height: 24px;
  padding: 0;
  gap: 0;
  border-bottom: none;
}
/* left/bottom：grip 最左，折叠按钮最右 */
.panel-header--left .panel-header__grip,
.panel-header--bottom .panel-header__grip {
  order: -1;
}
.panel-header--left .panel-header__collapse,
.panel-header--bottom .panel-header__collapse {
  order: 99;
}
/* right：折叠按钮最左，grip 最右 */
.panel-header--right .panel-header__collapse {
  order: -1;
}
.panel-header--right .panel-header__grip {
  order: 99;
}
.panel-header__collapse {
  color: var(--jc-text-muted) !important;
}
.panel-header__collapse:hover {
  color: var(--jc-text) !important;
  background-color: var(--jc-bg-hover) !important;
}
</style>
