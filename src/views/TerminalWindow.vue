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
  <DockLayout @open-host="onOpenHost">
    <TerminalWorkspace ref="workspaceRef" :take-tab="takeTab" :zen="app.zenMode" />
  </DockLayout>
</template>

<script setup lang="ts">
/**
 * 独立终端窗口（主窗口完整克隆）
 * 作者：JC
 * 由 Tab 拖出触发 open_terminal_window 打开，路由 #/terminal-window?take=<tabId>...
 * take query 携带迁出 Tab 的元数据；工作区挂载后通过 take_term_buffer 还原缓冲区。
 * Tab 可继续拖出（再成新窗口）或拖回其他窗口（jc:tab-transfer）。
 */
import { computed, onMounted, ref } from 'vue'
import { useRoute } from 'vue-router'
import { useLayoutStore } from '@/stores/layout'
import { useHostStore } from '@/stores/host'
import { useAuthStore } from '@/stores/auth'
import { useAppStore } from '@/stores/app'
import DockLayout from '@/components/layout/DockLayout.vue'
import TerminalWorkspace from '@/components/terminal/TerminalWorkspace.vue'
import type { TransferredTab } from '@/composables/useWindowRegistry'
import type { HostItem } from '@/stores/host'

const route = useRoute()
const layout = useLayoutStore()
const hostStore = useHostStore()
const auth = useAuthStore()
const app = useAppStore()

const workspaceRef = ref<InstanceType<typeof TerminalWorkspace> | null>(null)

/** 从 URL query 还原迁出 Tab 的元数据 */
const takeTab = computed<TransferredTab | null>(() => {
  const q = route.query
  if (!q.take) return null
  return {
    id: String(q.take),
    title: q.title ? String(q.title) : '终端',
    host: q.host ? String(q.host) : undefined,
    username: q.username ? String(q.username) : undefined,
    hostname: q.hostname ? String(q.hostname) : undefined,
    sessionId: q.sid ? String(q.sid) : undefined,
    kind: q.kind === 'ssh' ? 'ssh' : 'local',
    connected: q.connected === '1'
  }
})

function onOpenHost(host: HostItem) {
  workspaceRef.value?.openHost(host)
}

onMounted(async () => {
  layout.load()
  // 会话列表按账号隔离: 独立终端窗口也要先拿到登录态(哪个账号)再加载列表
  auth.subscribeEvents()
  await auth.load()
  hostStore.bindAccountScope()
  if (!hostStore.loaded) await hostStore.load()
})
</script>
