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
  <el-dialog
    :model-value="modelValue"
    :title="t('portForward.mode.title')"
    width="96%"
    append-to-body
    class="jc-dialog fwd-dialog fwd-mode-dialog"
    @update:model-value="(v: boolean) => emit('update:modelValue', v)"
  >
    <p class="mode-tip">{{ t('portForward.mode.tip') }}</p>
    <ForwardModePicker @select="pick" />
  </el-dialog>
</template>

<script setup lang="ts">
/**
 * 创建端口转发：第一步选择模式
 * 作者：JC
 * 卡片内容复用 ForwardModePicker，与端口转发页空状态里的选择区保持一致。
 */
import { useI18nStore } from '@/stores/i18n'
import type { ForwardMode } from '@/stores/portForward'
import ForwardModePicker from './ForwardModePicker.vue'

defineProps<{ modelValue: boolean }>()
const emit = defineEmits<{
  (e: 'update:modelValue', v: boolean): void
  (e: 'select', mode: ForwardMode): void
}>()

const t = useI18nStore().t

function pick(mode: ForwardMode): void {
  emit('update:modelValue', false)
  emit('select', mode)
}
</script>

<style scoped>
.mode-tip {
  margin: 0 0 14px;
  font-size: 14px;
  line-height: 1.6;
  color: var(--jc-text-muted);
}
</style>
