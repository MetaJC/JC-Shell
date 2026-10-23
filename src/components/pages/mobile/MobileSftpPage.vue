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
      绑定「当前激活终端标签」的会话：切哪个终端，这里就是哪台主机的 SFTP。
      key 用 sessionId：换会话必须重建浏览器，否则会残留上一台主机的目录栈。
      FileSystemBrowser 自带终端目录双向联动（连接时取 shell cwd 定位，之后轮询同步），
      因此无需额外传参即可满足「目录与终端同步」。
    -->
    <FileSystemBrowser
      v-if="connected && sessionId"
      :key="sessionId"
      class="m-page__fill"
      dock="right"
      :connected="connected"
      :hostname="hostname"
      :session-id="sessionId"
      :kind="kind"
      :grip="false"
      :hide-collapse="true"
    />
    <div v-else class="m-empty">
      <el-icon :size="40" class="m-empty__icon"><UploadFilled /></el-icon>
      <div class="m-empty__title">{{ t('mobile.noSession.title') }}</div>
      <div class="m-empty__desc">{{ t('mobile.noSession.sftp') }}</div>
      <el-button type="primary" @click="goSessions">{{ t('mobile.goSessions') }}</el-button>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 移动端「SFTP」页（底部导航第 3 项）
 * 作者：JC
 *
 * 默认打开「当前选中终端」对应主机的文件目录，并与该终端的工作目录保持同步。
 * 没有已连接的会话时给出空态与跳转入口，而不是展示一个无法操作的空白文件树。
 */
import FileSystemBrowser from '@/components/filesystem/FileSystemBrowser.vue'
import { UploadFilled } from '@element-plus/icons-vue'
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
