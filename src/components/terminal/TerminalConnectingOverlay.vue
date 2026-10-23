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
  <div class="term-connect">
    <div class="term-connect__card">
      <!-- 连接标识：旋转的渐变环 + 终端字形（带闪烁光标），而非普通转圈 loading -->
      <div class="term-connect__glyph">
        <span class="term-connect__ring" />
        <span class="term-connect__core">&gt;_<span class="term-connect__caret">▋</span></span>
      </div>
      <!-- 流光进度条 -->
      <div class="term-connect__bar" />
      <div class="term-connect__title">{{ t('terminal.connecting.label') }}</div>
      <div v-if="host" class="term-connect__host">{{ host }}</div>
      <div class="term-connect__tip">{{ t('terminal.connecting.desc') }}</div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 连接会话「连接中」过渡遮罩（仅移动端，由 CAP.connectingOverlay 控制渲染）
 * 作者：JC
 * 设计：旋转渐变环包裹终端字形（呼吸 + 光标闪烁）+ 流光进度条 + 文案，
 * 用主题色（--jc-accent）与背景（--jc-bg-soft）自适应明暗主题，避免单调的转圈图。
 */
import { useI18nStore } from '@/stores/i18n'

const props = defineProps<{ host?: string }>()
const { t } = useI18nStore()
void props
</script>

<style scoped>
.term-connect {
  position: absolute;
  inset: 0;
  z-index: 30;
  display: flex;
  align-items: center;
  justify-content: center;
  background: radial-gradient(
    130% 110% at 50% 35%,
    color-mix(in srgb, var(--jc-bg-soft) 70%, transparent),
    var(--jc-bg-soft) 72%
  );
  backdrop-filter: blur(3px);
  -webkit-backdrop-filter: blur(3px);
}
.term-connect__card {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 16px;
  padding: 28px 34px;
  border-radius: 20px;
  animation: jc-connect-breathe 2.6s ease-in-out infinite;
}
.term-connect__glyph {
  position: relative;
  width: 88px;
  height: 88px;
  display: grid;
  place-items: center;
}
.term-connect__ring {
  position: absolute;
  inset: 0;
  border-radius: 26px;
  background: conic-gradient(from 0deg, transparent 0 68%, var(--jc-accent) 100%);
  -webkit-mask: radial-gradient(farthest-side, transparent calc(100% - 4px), #000 calc(100% - 4px));
  mask: radial-gradient(farthest-side, transparent calc(100% - 4px), #000 calc(100% - 4px));
  animation: jc-connect-spin 1.15s linear infinite;
}
.term-connect__core {
  position: relative;
  z-index: 1;
  font-family: ui-monospace, "SF Mono", "Cascadia Code", Consolas, monospace;
  font-size: 26px;
  font-weight: 700;
  letter-spacing: 1px;
  color: var(--jc-accent);
  text-shadow: 0 0 18px color-mix(in srgb, var(--jc-accent) 45%, transparent);
}
.term-connect__caret {
  margin-left: 1px;
  animation: jc-connect-blink 1s steps(1) infinite;
}
.term-connect__bar {
  position: relative;
  width: 176px;
  height: 4px;
  border-radius: 999px;
  background: var(--jc-border);
  overflow: hidden;
}
.term-connect__bar::after {
  content: "";
  position: absolute;
  top: 0;
  left: 0;
  height: 100%;
  width: 42%;
  border-radius: 999px;
  background: linear-gradient(90deg, transparent, var(--jc-accent), transparent);
  animation: jc-connect-shimmer 1.5s ease-in-out infinite;
}
.term-connect__title {
  color: var(--jc-text);
  font-size: 15px;
  font-weight: 600;
  letter-spacing: 0.5px;
}
.term-connect__host {
  color: var(--jc-text);
  font-size: 13px;
  font-family: ui-monospace, monospace;
  opacity: 0.9;
  max-width: 220px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.term-connect__tip {
  color: var(--jc-text-muted);
  font-size: 12px;
}

@keyframes jc-connect-spin {
  to { transform: rotate(360deg); }
}
@keyframes jc-connect-breathe {
  0%, 100% { transform: scale(1); opacity: 0.92; }
  50% { transform: scale(1.04); opacity: 1; }
}
@keyframes jc-connect-shimmer {
  0% { transform: translateX(-120%); }
  100% { transform: translateX(320%); }
}
@keyframes jc-connect-blink {
  50% { opacity: 0; }
}

/* 尊重用户的「减少动态效果」偏好：不做旋转/闪烁，仅保留静态信息 */
@media (prefers-reduced-motion: reduce) {
  .term-connect__ring,
  .term-connect__bar::after,
  .term-connect__caret,
  .term-connect__card {
    animation: none;
  }
}
</style>
