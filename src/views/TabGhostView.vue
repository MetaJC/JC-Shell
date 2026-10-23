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
  <div class="tab-ghost" :class="{ 'is-snapped': snapped }">
    <el-icon :size="11" class="tab-ghost__dot"><CircleCheckFilled /></el-icon>
    <span class="tab-ghost__title">{{ title }}</span>
  </div>
</template>

<script setup lang="ts">
/**
 * Tab 拖拽虚影窗口（label = tab-ghost）
 * 作者：JC
 * DOM 虚影画不出 WebView 边界，故由这个原生小窗口承载拖拽中的标签虚影。
 * 样式 1:1 复刻 Tab 栏标签（TerminalTabBar 的 .tab-item--active），仅整体半透明（不透明度 30%）；
 * 吸附到 Tab 栏插入位置时切到 .is-snapped 高亮，提示"松手即并入此处"。
 * 位置/尺寸由 Rust 端跟随循环驱动（本页只渲染，不处理移动）。
 * 标题来源：URL ?title=（首次创建）；复用时经 tab-ghost:title 事件更新。
 */
import { ref, onMounted, onBeforeUnmount } from 'vue'
import { useRoute } from 'vue-router'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { CircleCheckFilled } from '@element-plus/icons-vue'

/** 主题色（源窗口实际生效的 CSS 变量，由 tab_ghost_begin 注入） */
interface GhostTheme { bg: [number, number, number]; text: [number, number, number]; accent: [number, number, number]; success: [number, number, number] }
const rgbCss = (c: [number, number, number]) => `rgb(${c[0]}, ${c[1]}, ${c[2]})`

const route = useRoute()
const title = ref(String(route.query.title || '终端'))
const snapped = ref(false)

let unlistenTitle: UnlistenFn | null = null
let unlistenSnapped: UnlistenFn | null = null
let unlistenTheme: UnlistenFn | null = null

/** 把源窗口的主题色写到根元素：本窗口是独立 webview，
 *  默认只会拿到 :root 的浅色变量（--jc-bg 白），必须显式同步才能与界面同风格 */
function applyTheme(t: GhostTheme) {
  const root = document.documentElement
  root.style.setProperty('--jc-bg', rgbCss(t.bg))
  root.style.setProperty('--jc-text', rgbCss(t.text))
  root.style.setProperty('--jc-accent', rgbCss(t.accent))
  root.style.setProperty('--jc-success', rgbCss(t.success))
}

onMounted(async () => {
  try {
    unlistenTitle = await listen<string>('tab-ghost:title', ev => { title.value = ev.payload })
    unlistenSnapped = await listen<boolean>('tab-ghost:snapped', ev => { snapped.value = ev.payload })
    unlistenTheme = await listen<GhostTheme>('tab-ghost:theme', ev => { applyTheme(ev.payload) })
  } catch { /* ignore */ }
})

onBeforeUnmount(() => {
  unlistenTitle?.()
  unlistenSnapped?.()
  unlistenTheme?.()
})
</script>

<style>
html, body, #app {
  margin: 0;
  padding: 0;
  /* 不透明背景：窗口底色已由 Rust 设为源窗口 --jc-bg，这里同源避免首帧白闪 */
  background-color: var(--jc-bg, #1e1e1e);
  color: var(--jc-text, #e5e7eb);
  overflow: hidden;
}
/* 与 Tab 栏标签完全一致：同高(28)、同内边距(12)、同字号(12)、同背景与下划线 */
.tab-ghost {
  box-sizing: border-box;
  width: 100vw;
  height: 100vh;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 0 12px;
  font-size: 12px;
  color: var(--jc-text, #d4d4d4);
  background-color: var(--jc-bg, #1f1f23);
  border-right: 1px solid var(--jc-border, #2b2b30);
  box-shadow: inset 0 -2px 0 0 var(--jc-accent, #3b82f6);
  /* 半透明虚影：不透明度 30% */
  opacity: 0.3;
  user-select: none;
  cursor: default;
}
/* 吸附态：略微提亮 + 下划线转绿，提示可并入 */
.tab-ghost.is-snapped {
  opacity: 0.55;
  box-shadow: inset 0 -2px 0 0 #22c55e;
}
.tab-ghost__dot {
  color: var(--jc-success, #22c55e);
  flex-shrink: 0;
}
.tab-ghost__title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
