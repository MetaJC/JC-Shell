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
  <div class="page-root h-full w-full flex flex-col min-h-0 bg-[var(--jc-bg)]">
    <header class="page-head shrink-0 flex items-center gap-2 px-3 h-9 border-b border-[var(--jc-border)]">
      <el-icon :size="16" color="var(--jc-text-muted)"><component :is="icon" /></el-icon>
      <span class="text-[13px] text-[var(--jc-text)]">{{ title }}</span>
    </header>
    <div class="flex-1 min-h-0 flex items-center justify-center">
      <div class="flex flex-col items-center gap-3 text-center px-6">
        <el-icon :size="44" color="var(--jc-text-muted)"><component :is="icon" /></el-icon>
        <div class="text-[15px] text-[var(--jc-text)]">{{ title }}</div>
        <div class="text-xs text-[var(--jc-text-muted)] max-w-[320px] leading-relaxed">{{ desc }}</div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 页面占位骨架
 * 作者：JC
 */
import type { Component } from 'vue'

defineProps<{
  /** 页面标题 */
  title: string
  /** 页面说明（一句话描述将提供的能力） */
  desc: string
  /** 顶部与空态使用的图标 */
  icon: Component
}>()
</script>
