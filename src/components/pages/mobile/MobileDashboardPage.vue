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
  <div class="m-page">
    <!--
      设备信息看板的移动端承载：与桌面端右停靠面板同一个组件、同一份会话上下文，
      所以「打开的是哪个终端，显示的就是哪台主机」这条语义在两端天然一致。
    -->
    <DeviceInfoPanel
      v-if="connected && sessionId"
      :key="sessionId"
      class="m-page__fill"
      dock="right"
      :connected="connected"
      :hostname="hostname"
      :session-id="sessionId"
      :kind="kind"
    />
    <div v-else class="m-empty">
      <el-icon :size="40" class="m-empty__icon"><Odometer /></el-icon>
      <div class="m-empty__title">{{ t('mobile.noSession.title') }}</div>
      <div class="m-empty__desc">{{ t('mobile.noSession.device') }}</div>
      <el-button type="primary" @click="goSessions">{{ t('mobile.goSessions') }}</el-button>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 移动端「仪表盘」页（底部导航第 4 项）
 * 作者：JC
 *
 * 即桌面端的设备信息看板（CPU / 内存 / 磁盘 / 网络 / 进程 等卡片），
 * 数据源同样来自「当前激活终端标签」，切换终端即切换主机。
 */
import DeviceInfoPanel from '@/components/device/DeviceInfoPanel.vue'
import { Odometer } from '@element-plus/icons-vue'
import { useDockStore } from '@/stores/dock'
import { useI18nStore } from '@/stores/i18n'
import { useActiveSession } from '@/composables/useActiveSession'

const t = useI18nStore().t
const dockStore = useDockStore()
const { connected, hostname, sessionId, kind } = useActiveSession()

function goSessions(): void {
  dockStore.setActivePage('sessions')
}
</script>
