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
  <HostListPanel
    v-if="panelId === 'hostList'"
    :dock="dock"
    :collapsed="collapsed"
    @open="(h: HostItem) => emit('open', h)"
    @collapse="emit('collapse')"
  />
  <keep-alive v-else-if="panelId === 'fileSystem'" :max="6">
    <FileSystemBrowser
      :key="sessionId || 'none'"
      :dock="dock"
      :collapsed="collapsed"
      :connected="connected"
      :hostname="hostname"
      :session-id="sessionId"
      :kind="kind"
      @collapse="emit('collapse')"
    />
  </keep-alive>
  <keep-alive v-else :max="6">
    <DeviceInfoPanel
      :key="sessionId || 'none'"
      :dock="dock"
      :collapsed="collapsed"
      :connected="connected"
      :hostname="hostname"
      :session-id="sessionId"
      :kind="kind"
      @collapse="emit('collapse')"
    />
  </keep-alive>
</template>

<script setup lang="ts">
/**
 * Dock 面板分发器
 * 作者：JC
 * 按 panelId 渲染对应面板组件，统一透传 dock/collapsed 与会话上下文。
 */
import HostListPanel from '@/components/host/HostListPanel.vue'
import FileSystemBrowser from '@/components/filesystem/FileSystemBrowser.vue'
import DeviceInfoPanel from '@/components/device/DeviceInfoPanel.vue'
import type { PanelId, Dock } from '@/stores/layout'
import type { HostItem } from '@/stores/host'

defineProps<{
  panelId: PanelId
  dock: Dock
  collapsed: boolean
  /** 当前激活 Tab 的会话上下文（fs/设备面板绑定） */
  connected?: boolean
  hostname?: string
  sessionId?: string
  /** 会话类型（local/ssh），决定 fs 数据读取链路 */
  kind?: 'local' | 'ssh'
}>()

const emit = defineEmits<{
  (e: 'open', host: HostItem): void
  (e: 'collapse'): void
}>()
</script>
