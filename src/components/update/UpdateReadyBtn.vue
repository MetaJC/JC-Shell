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
  <el-tooltip
    v-if="updateStore.updateReady"
    content="点击重启并更新到新版本"
    placement="bottom"
    :show-after="100"
    :hide-after="50"
  >
    <button
      class="update-ready-btn"
      data-testid="update-ready-btn"
      @click="onClick"
    >
      <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
        <path d="M8 2.5v7"/>
        <polyline points="4.5,7 8,10.5 11.5,7"/>
        <path d="M3.5 12.5h9"/>
      </svg>
      <span class="btn-text">新版本已就绪</span>
    </button>
  </el-tooltip>
</template>

<script setup lang="ts">
/**
 * 新版本就绪按钮（右上角显示）
 * 作者：JC
 * 闲时静默下载完成后常驻显示；点击即静默安装并重启，用户感知上就是"重启了一下软件"。
 * 用户不点则一直是当前版本（关掉应用再打开提示依然在）。
 */
import { useUpdateStore } from '@/stores/update'

const updateStore = useUpdateStore()

async function onClick(): Promise<void> {
  // 安装包已就绪，直接静默安装 + 自动重启，不再弹窗二次确认
  await updateStore.installReady()
}
</script>

<style scoped>
.update-ready-btn {
  height: 100%;
  padding: 0 12px;
  display: flex;
  align-items: center;
  gap: 6px;
  background: transparent;
  border: none;
  border-left: 1px solid var(--jc-border);
  color: var(--jc-accent);
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  -webkit-app-region: no-drag;
  transition: background 0.15s, color 0.15s;
  animation: update-ready-pulse 2.4s ease-in-out infinite;
}
.update-ready-btn:hover {
  background: var(--jc-accent);
  color: #fff;
  animation: none;
}
.btn-text {
  white-space: nowrap;
}
@keyframes update-ready-pulse {
  0%, 100% { background: transparent; }
  50% { background: rgba(64, 158, 255, 0.18); }
}
</style>
