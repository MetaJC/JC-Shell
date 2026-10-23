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

<script setup lang="ts">
/**
 * APP 内网页查看器（全屏覆盖层）
 * 作者：JC
 *
 * 用于在移动端 APP 内直接打开用户协议 / 隐私政策等外部网页，不再跳转系统浏览器。
 * 内容由 WebView 通过 iframe 直接加载，保留原页面的样式、图片与排版。
 * 仅移动端登录框使用；桌面端协议链接仍走系统浏览器，互不影响。
 */
import { onBeforeUnmount, watch } from 'vue'
import { ArrowLeft } from '@element-plus/icons-vue'

const props = defineProps<{
  visible: boolean
  url: string
  title?: string
}>()
const emit = defineEmits<{
  (e: 'update:visible', v: boolean): void
  (e: 'close'): void
}>()

function close() {
  emit('update:visible', false)
  emit('close')
}

/** 桌面键盘 / 平板外接键盘：Esc 关闭 */
function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') close()
}
watch(
  () => props.visible,
  (v) => {
    if (v) window.addEventListener('keydown', onKeydown)
    else window.removeEventListener('keydown', onKeydown)
  },
)
onBeforeUnmount(() => window.removeEventListener('keydown', onKeydown))
</script>

<template>
  <Teleport to="body">
    <div v-if="visible && url" class="jav">
      <div class="jav__bar">
        <button class="jav__back" type="button" @click="close">
          <el-icon><ArrowLeft /></el-icon>
        </button>
        <div class="jav__title">{{ title }}</div>
        <div class="jav__spacer" />
      </div>
      <iframe class="jav__frame" :src="url" referrerpolicy="no-referrer" />
    </div>
  </Teleport>
</template>

<style scoped>
.jav {
  position: fixed;
  inset: 0;
  z-index: 4000;
  display: flex;
  flex-direction: column;
  background: var(--jc-bg, #fff);
}
.jav__bar {
  flex: none;
  display: flex;
  align-items: center;
  gap: 8px;
  height: 48px;
  padding: 0 8px;
  border-bottom: 1px solid var(--jc-border, #e5e6eb);
  background: var(--jc-bg, #fff);
  /* 顶部安全区（刘海屏） */
  padding-top: env(safe-area-inset-top);
  height: calc(48px + env(safe-area-inset-top));
}
.jav__back {
  flex: none;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 36px;
  height: 36px;
  border: none;
  border-radius: 8px;
  background: transparent;
  color: var(--jc-text, #1f2329);
  font-size: 18px;
  cursor: pointer;
}
.jav__back:active {
  background: var(--jc-bg-soft, rgba(0, 0, 0, 0.05));
}
.jav__title {
  flex: 1;
  overflow: hidden;
  text-align: center;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 15px;
  font-weight: 600;
  color: var(--jc-text, #1f2329);
}
.jav__spacer {
  flex: none;
  width: 36px;
}
.jav__frame {
  flex: 1;
  width: 100%;
  border: none;
  background: #fff;
}
</style>
