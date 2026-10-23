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
  <div class="h-full w-full bg-[var(--jc-bg)] text-[var(--jc-text)] flex flex-col overflow-hidden">
    <AppSettingsDialog />
    <!-- 同步冲突裁决弹窗：设置窗口自己也会跑同步，冲突必须能在这里弹出 -->
    <SyncConflictDialog />
    <!-- 安全密码弹窗：设置窗口是独立 webview，需自带一份 SecurityDialog，
         否则在本窗口点「启用安全密码 / 验证」时弹窗会渲染到主窗口而看不到 -->
    <SecurityDialog />
    <!-- 生物识别「换一种验证方式」选择器：与 SecurityDialog 同理，独立窗口需自带一份 -->
    <BiometricMethodDialog />
  </div>
</template>

<script setup lang="ts">
/**
 * 独立设置窗口根组件
 * 作者：JC
 * 直接复用 AppSettingsDialog，去掉主窗口外壳。
 * 登录态与主窗口同步：启动时拉取一次 + 订阅后端 auth_state_changed
 * 全局广播（登录/登出在任一窗口发生，所有窗口同步刷新）。
 * 支持通过 URL `?tab=xxx` 或 Rust 事件 `settings:switch-tab` 切到指定 tab。
 */
import { onMounted, onBeforeUnmount } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import AppSettingsDialog from '@/components/settings/AppSettingsDialog.vue'
import SyncConflictDialog from '@/components/settings/SyncConflictDialog.vue'
import SecurityDialog from '@/components/common/SecurityDialog.vue'
import BiometricMethodDialog from '@/components/common/BiometricMethodDialog.vue'
import { useAuthStore } from '@/stores/auth'
import { useAppStore, type SettingsTab } from '@/stores/app'
import { useHostStore } from '@/stores/host'
import { useSyncStore } from '@/stores/sync'
import { isTauriEnv } from '@/api'

const auth = useAuthStore()
const app = useAppStore()
const hostStore = useHostStore()
const sync = useSyncStore()

/** 解析 URL 中的 ?tab= 并写入 store（启动时一次性） */
function applyTabFromUrl() {
  try {
    const sp = new URLSearchParams(window.location.search)
    const t = sp.get('tab')?.trim()
    if (t) app.settingsInitialTab = t as SettingsTab
  } catch { /* ignore */ }
}

let unlistenSwitch: UnlistenFn | null = null

onMounted(async () => {
  applyTabFromUrl()
  auth.subscribeEvents()
  // 顺序不可颠倒: 登录态决定会话列表的归属域, 会话数据又是同步的合并输入。
  // 先等 auth.load() 再 hostStore.load(), 最后才启动同步 —— 否则同步会拿"未加载的空列表"
  // 参与合并, 一旦云端为空就把空文档推上去, 造成本机会话列表被"同步没"的假象。
  await auth.load()
  if (isTauriEnv()) {
    if (!hostStore.loaded) await hostStore.load()
    // 独立设置窗口作为 follower：不跑同步引擎，只把本窗口的配置变更/裁决转发给主窗口(leader)
    void sync.init('follower')
  }
  // 已开窗口：主窗口通过 Rust emit `settings:switch-tab` 通知切换
  unlistenSwitch = await listen<string>('settings:switch-tab', (e) => {
    const t = (e.payload || '').trim()
    if (t) app.settingsInitialTab = t as SettingsTab
  })
})

onBeforeUnmount(() => {
  unlistenSwitch?.()
  unlistenSwitch = null
})
</script>
