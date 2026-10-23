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
    v-model="visible"
    :title="t('settings.account.status.sync.conflict.title')"
    width="480"
    append-to-body
    :close-on-click-modal="false"
    :close-on-press-escape="false"
    :show-close="false"
    class="jc-dialog jc-sync-conflict-dialog"
  >
    <div class="conflict-body">
      <div class="conflict-alert">
        <el-icon class="conflict-warn-icon"><WarningFilled /></el-icon>
        <div>
          <div class="conflict-alert-title">{{ t('settings.account.status.sync.conflict.desc') }}</div>
          <div class="conflict-alert-sub">
            {{ t('settings.account.status.sync.conflict.detail', { scope: docText, localRev: info?.localRev ?? 0, remoteRev: info?.remoteRev ?? 0 }) }}
          </div>
        </div>
      </div>

      <div
        v-for="opt in options"
        :key="opt.key"
        class="conflict-option"
        :class="{ 'is-active': chosen === opt.key }"
        @click="chosen = opt.key"
      >
        <el-radio v-model="chosen" :value="opt.key" class="conflict-radio">
          <span class="conflict-option-title">{{ opt.title }}</span>
        </el-radio>
        <div class="conflict-option-desc">{{ opt.desc }}</div>
      </div>
    </div>

    <template #footer>
      <el-button size="small" :disabled="busy" @click="onResolve(chosen)">
        {{ t('common.confirm') }}
      </el-button>
    </template>
  </el-dialog>
</template>

<script setup lang="ts">
/**
 * 同步冲突裁决弹窗
 * 作者：JC
 *
 * 触发条件：两端相对上一次共同祖先都改过配置（sync store 判定为 ConflictInfo 时挂起同步）。
 * 此时"自动选一边"没有正确答案 —— 选本地会丢云端改动，选云端会丢本地改动，
 * 所以必须停下来让用户决定，三种策略各自对应一种明确的取舍。
 */
import { computed, ref, watch } from 'vue'
import { ElMessage } from 'element-plus'
import { handleApiError } from '@/utils/errorHandler'
import { WarningFilled } from '@element-plus/icons-vue'
import { useSyncStore, type ConflictStrategy } from '@/stores/sync'
import { useI18nStore } from '@/stores/i18n'

const sync = useSyncStore()
const i18n = useI18nStore()
const t = (k: string, p?: Record<string, string | number>) => i18n.t(k, p)

const visible = ref(false)
/** 默认停在"智能合并": 它是唯一不丢数据的选项, 误点确认也不会造成损失 */
const chosen = ref<ConflictStrategy>('merge')
const busy = computed(() => sync.busy)

const info = computed(() => sync.conflict)

/** 冲突涉及的文档，拼成"会话列表 / 偏好设置"给用户看 */
const docText = computed(() => {
  const docs = info.value?.docs ?? []
  const names = docs.map(d => (d === 'hosts' ? t('settings.account.status.sync.conflict.hosts') : t('settings.account.status.sync.conflict.prefs')))
  return names.length ? names.join(' / ') : '-'
})

const options = computed(() => [
  {
    key: 'merge' as ConflictStrategy,
    title: t('settings.account.status.sync.conflict.merge'),
    desc: t('settings.account.status.sync.conflict.mergeDesc'),
  },
  {
    key: 'cloud' as ConflictStrategy,
    title: t('settings.account.status.sync.conflict.cloud'),
    desc: t('settings.account.status.sync.conflict.cloudDesc'),
  },
  {
    key: 'local' as ConflictStrategy,
    title: t('settings.account.status.sync.conflict.local'),
    desc: t('settings.account.status.sync.conflict.localDesc'),
  },
])

// 冲突出现即弹窗；用户裁决后 sync.conflict 会被清空，这里跟着关
watch(() => sync.conflict, (v) => {
  if (v) {
    chosen.value = 'merge'
    visible.value = true
  } else {
    visible.value = false
  }
}, { immediate: true })

async function onResolve(strategy: ConflictStrategy) {
  if (busy.value) return
  visible.value = false
  try {
    await sync.resolveConflict(strategy)
    if (sync.lastError) ElMessage.warning(sync.lastError)
    else ElMessage.success(t('settings.account.status.sync.success'))
  } catch (e: any) {
    handleApiError(e, 'components/SyncConflictDialog.resolve', {
      prefix: t('common.operationFail'),
    })
  }
}
</script>

<style scoped>
.conflict-body { display: flex; flex-direction: column; gap: 12px; }

.conflict-alert {
  display: flex;
  gap: 10px;
  padding: 10px 12px;
  border-radius: 6px;
  background: color-mix(in srgb, var(--el-color-warning) 12%, transparent);
  border: 1px solid color-mix(in srgb, var(--el-color-warning) 40%, transparent);
}
.conflict-warn-icon { font-size: 20px; color: var(--el-color-warning); flex-shrink: 0; margin-top: 1px; }
.conflict-alert-title { font-size: 13px; font-weight: 600; color: var(--jc-text); }
.conflict-alert-sub { margin-top: 3px; font-size: 12px; color: var(--jc-text-muted); line-height: 1.5; }

.conflict-option {
  padding: 10px 12px;
  border: 1px solid var(--jc-border);
  border-radius: 6px;
  cursor: pointer;
  transition: border-color 0.12s, background-color 0.12s;
}
.conflict-option:hover { background: var(--jc-bg-hover); }
.conflict-option.is-active { border-color: var(--jc-accent); background: var(--jc-bg-hover); }
.conflict-radio { width: 100%; }
.conflict-option-title { font-size: 13px; font-weight: 600; color: var(--jc-text); }
.conflict-option-desc { margin-top: 4px; padding-left: 22px; font-size: 12px; color: var(--jc-text-muted); line-height: 1.5; }
</style>
