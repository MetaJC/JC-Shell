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
  <!-- 必须保持 h-full：#app 高度链 html/body → #app → 本节点 → 各窗口 h-full，缺一即整体塌陷 -->
  <div class="h-full w-full">
    <router-view />
    <!-- 全局文件编辑器窗口（文件系统右键"打开"/双击文件触发） -->
    <FileEditorOverlay />
  </div>
</template>

<script setup lang="ts">
/**
 * 应用根节点（路由出口）
 * 作者：JC
 */
import { defineAsyncComponent, onMounted } from 'vue'
import { useConsentStore } from '@/stores/consent'

/** 文件编辑器按需异步加载（CodeMirror 体积较大，不阻塞首屏） */
const FileEditorOverlay = defineAsyncComponent(() => import('@/components/editor/FileEditorOverlay.vue'))

const consent = useConsentStore()
// 启动即拉取法律协议配置（供登录/注册表单判断、登录时上送协议版本）
onMounted(() => {
  void consent.fetchConfig()
})
</script>
