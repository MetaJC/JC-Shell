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
  <div class="m-page m-session-edit">
    <!-- 顶部返回条：左返回箭头 + 标题；新建与编辑共用这个页 -->
    <header class="m-se-hed">
      <button class="m-se-hed__back" @click="onBack" :aria-label="t('common.back')">
        <el-icon :size="22"><ArrowLeft /></el-icon>
      </button>
      <div class="m-se-hed__title">{{ isEdit ? t('session.edit') : t('session.create') }}</div>
      <span class="m-se-hed__spacer" />
    </header>

    <!-- 可滚动的表单区 -->
    <div class="m-se-body">
      <SessionForm
        ref="formRef"
        :mode="isEdit ? 'edit' : 'create'"
        :host="editHost"
        :default-parent-id="page.defaultParentId ?? null"
        @saved="onSaved"
        @save-and-connect="onSaveAndConnect"
      />
    </div>

    <!-- 底部操作栏：左「测试连接」；右「取消 / 保存 / 保存并连接」 -->
    <footer class="m-se-foot">
      <el-button class="m-se-foot__test" @click="formRef?.testConnection()">
        {{ t('session.testConnection') }}
      </el-button>
      <span class="m-se-foot__main">
        <el-button @click="onBack">{{ t('common.cancel') }}</el-button>
        <el-button type="primary" @click="formRef?.save()">{{ t('common.save') }}</el-button>
        <el-button type="primary" plain @click="formRef?.saveAndConnect()">{{ t('session.saveAndConnect') }}</el-button>
      </span>
    </footer>
  </div>
</template>

<script setup lang="ts">
/**
 * 移动端「新建 / 编辑会话」整页
 * 作者：JC
 *
 * 移动端把会话表单从弹窗改为整页：点「新建会话」进入此页，输入完毕——
 *   · 保存：落库后返回会话列表（pop 子页栈）；
 *   · 保存并连接：落库后打开该会话并切到「终端」页。
 * 编辑模式由 dock 子页栈传入的 hostId 决定；表单内容由 SessionForm 承载（与桌面弹窗同一份）。
 */
import { computed, inject, ref } from 'vue'
import { ArrowLeft } from '@element-plus/icons-vue'
import { useDockStore, type MobileSubPage } from '@/stores/dock'
import { useHostStore, type HostItem } from '@/stores/host'
import { useI18nStore } from '@/stores/i18n'
import { OPEN_HOST_KEY } from '@/composables/useOpenHost'
import SessionForm from '@/components/host/SessionForm.vue'

const props = defineProps<{ page: MobileSubPage }>()

const dockStore = useDockStore()
const hostStore = useHostStore()
const t = useI18nStore().t

/** 由 DockLayout 注入的连接入口（→ MainWindow → TerminalWorkspace 开标签） */
const openHost = inject(OPEN_HOST_KEY, () => { /* 脱离 DockLayout 时忽略 */ })

const formRef = ref<InstanceType<typeof SessionForm> | null>(null)

const isEdit = computed(() => props.page.type === 'session-edit' && !!props.page.hostId)
/** 编辑模式：从 store 取出要编辑的会话对象（深拷贝其数据为表单初始值） */
const editHost = computed<HostItem | null>(() =>
  props.page.hostId ? (hostStore.hosts.find(h => h.id === props.page.hostId) ?? null) : null
)

/** 返回：出栈子页（回到会话列表） */
function onBack(): void {
  dockStore.popMobileSubPage()
}

/** 仅保存：落库后返回列表 */
function onSaved(): void {
  dockStore.popMobileSubPage()
}

/** 保存并连接：落库后打开会话并切到终端页 */
function onSaveAndConnect(host: HostItem): void {
  openHost(host)
  dockStore.setActivePage('terminal')
  dockStore.popMobileSubPage()
}
</script>
