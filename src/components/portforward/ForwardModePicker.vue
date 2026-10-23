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
  <div class="mode-list">
    <button
      v-for="(m, i) in FORWARD_MODES"
      :key="m"
      type="button"
      class="mode-card"
      @click="emit('select', m)"
    >
      <div class="mode-card__head">
        <span class="mode-card__index">{{ i + 1 }}</span>
        <span class="mode-card__title">{{ t(`portForward.mode.${m}.title`) }}</span>
      </div>
      <p class="mode-card__desc">{{ t(`portForward.mode.${m}.desc`) }}</p>
      <div class="mode-card__diagram">
        <ForwardDiagram :mode="m" compact />
      </div>
    </button>
  </div>
</template>

<script setup lang="ts">
/**
 * 转发模式选择卡片（本地 / 远程 / 动态）
 * 作者：JC
 *
 * 两处复用，保证视觉与交互完全一致：
 *   1. 创建流程第一步的弹窗（ForwardModeDialog）；
 *   2. 端口转发页「还没有转发规则」空状态里直接嵌入。
 */
import { useI18nStore } from '@/stores/i18n'
import { FORWARD_MODES, type ForwardMode } from '@/stores/portForward'
import ForwardDiagram from './ForwardDiagram.vue'

const emit = defineEmits<{ (e: 'select', mode: ForwardMode): void }>()

const t = useI18nStore().t
</script>

<style scoped>
.mode-list {
  display: grid;
  /* 按可用宽度自适应列数：宽屏三列，窄了自动换行，避免卡片被压得太窄 */
  grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
  gap: 16px;
  /* 行高只按内容撑开，不被容器剩余高度拉伸（否则卡片会被纵向拉高） */
  align-content: start;
}
.mode-card {
  display: flex;
  flex-direction: column;
  gap: 8px;
  /* 撑满网格行高，图示区域吃掉多余高度并居中 */
  height: 100%;
  padding: 20px 18px 14px;
  border: 1px solid var(--jc-border);
  border-radius: 10px;
  background: var(--jc-bg-panel);
  color: var(--jc-text);
  text-align: left;
  cursor: pointer;
  transition: border-color 0.15s ease, box-shadow 0.15s ease, transform 0.15s ease;
}
.mode-card:hover {
  border-color: var(--jc-accent);
  box-shadow: 0 6px 18px -8px var(--jc-accent);
  transform: translateY(-2px);
}
.mode-card__head {
  display: flex;
  align-items: center;
  gap: 8px;
}
.mode-card__index {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border-radius: 50%;
  font-size: 14px;
  color: #fff;
  background: var(--jc-accent);
  flex-shrink: 0;
}
.mode-card__title {
  font-size: 17px;
  font-weight: 600;
}
.mode-card__desc {
  margin: 0;
  font-size: 14px;
  line-height: 1.65;
  color: var(--jc-text-muted);
  min-height: 46px;
}
/* 图示区：固定一个紧凑高度，图示在其中等比缩放居中。
   画布是 1400×760（≈1.842:1），若按宽度自适应，图高 = 卡片宽/1.842 会随窗口宽度变化
   （Mac 大窗口下卡片宽 ~400px → 图高 ~220px），把卡片纵向撑得很高、看着"被拉长"。
   固定高度后卡片高度稳定、紧凑，且与窗口宽度无关。 */
.mode-card__diagram {
  flex: 0 0 auto;
  height: 168px;
  min-height: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
}
.mode-card__diagram :deep(.fwd-canvas) {
  width: 100%;
  max-width: 100%;
  height: 100%;
}
.mode-card__diagram :deep(.fwd-svg) {
  width: 100%;
  height: 100%;
}
/* 卡片里的图示被固定高度限制后绘制比例更小，标签在画布坐标里要多放大一档才看得清 */
.mode-card__diagram :deep(.fwd--compact .fwd-cap),
.mode-card__diagram :deep(.fwd--compact .fwd-target__text),
.mode-card__diagram :deep(.fwd--compact .fwd-tgtsvc__text) {
  font-size: 58px;
}
</style>
