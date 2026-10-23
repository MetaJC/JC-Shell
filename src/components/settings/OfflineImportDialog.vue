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

<script setup lang="ts">
/**
 * 「导入本地离线配置」弹窗
 * 作者：JC
 *
 * 场景：用户在这台机器上未登录时建过会话列表（anon 归属域），登录后账号下却是空的。
 * 离线配置按设计不上云，只能由本弹窗显式并入当前账号 ——
 * 合并还是覆盖交给用户决定，同步流程绝不偷偷把两份数据搅在一起。
 */
import { computed, ref, watch } from 'vue'
import { ElMessage } from 'element-plus'
import { useI18nStore } from '@/stores/i18n'
import { useSyncStore } from '@/stores/sync'

const props = defineProps<{ modelValue: boolean }>()
const emit = defineEmits<{ (e: 'update:modelValue', v: boolean): void }>()

const i18n = useI18nStore()
const sync = useSyncStore()
const t = (k: string, p?: Record<string, string | number>) => i18n.t(k, p)

type Mode = 'merge' | 'overwrite'
/** 默认永远是更安全的合并：覆盖属于破坏性选项，必须用户主动选 */
const mode = ref<Mode>('merge')
const working = ref(false)

const visible = computed({
  get: () => props.modelValue,
  set: (v: boolean) => emit('update:modelValue', v),
})
const preview = computed(() => sync.offlinePreview)

watch(() => props.modelValue, (v) => { if (v) mode.value = 'merge' })

async function onConfirm(): Promise<void> {
  if (working.value) return
  working.value = true
  try {
    const res = await sync.runOfflineImport(mode.value)
    visible.value = false
    if (!res.ok) {
      ElMessage.info(t('settings.sync.import.none'))
      return
    }
    ElMessage.success(res.mode === 'overwrite'
      ? t('settings.sync.import.doneOverwrite')
      : t('settings.sync.import.doneMerge', { n: res.added + res.updated }))
  } finally {
    working.value = false
  }
}

/** 暂不导入：本机该账号以后不再自动提醒，设置里的入口仍然保留可反复执行 */
function onSkip(): void {
  sync.skipOfflineImport()
  visible.value = false
}
</script>

<template>
  <el-dialog
    v-model="visible"
    :title="t('settings.sync.import.title')"
    width="520px"
    class="jc-dialog"
    append-to-body
    :close-on-click-modal="false"
    :show-close="!working"
  >
    <div class="oi-body">
      <p class="oi-desc">{{ t('settings.sync.import.desc') }}</p>

      <div class="oi-stats">
        <div class="oi-stat">
          <div class="oi-stat-num">{{ preview?.offlineCount ?? 0 }}</div>
          <div class="oi-stat-label">{{ t('settings.sync.import.offline') }}</div>
        </div>
        <div class="oi-stat-arrow">→</div>
        <div class="oi-stat">
          <div class="oi-stat-num">{{ preview?.mergedCount ?? 0 }}</div>
          <div class="oi-stat-label">{{ t('settings.sync.import.merged') }}</div>
        </div>
        <div class="oi-stat oi-stat-muted">
          <div class="oi-stat-num">{{ preview?.accountCount ?? 0 }}</div>
          <div class="oi-stat-label">{{ t('settings.sync.import.account') }}</div>
        </div>
      </div>

      <div v-if="(preview?.conflictCount ?? 0) > 0" class="oi-warn">
        {{ t('settings.sync.import.conflict', { n: preview?.conflictCount ?? 0 }) }}
      </div>

      <div class="oi-mode-title">{{ t('settings.sync.import.mode') }}</div>
      <el-radio-group v-model="mode" class="oi-modes">
        <el-radio value="merge" class="oi-mode">
          <div class="oi-mode-name">{{ t('settings.sync.import.merge') }}</div>
          <div class="oi-mode-desc">{{ t('settings.sync.import.mergeDesc') }}</div>
        </el-radio>
        <el-radio value="overwrite" class="oi-mode">
          <div class="oi-mode-name">{{ t('settings.sync.import.overwrite') }}</div>
          <div class="oi-mode-desc">{{ t('settings.sync.import.overwriteDesc') }}</div>
        </el-radio>
      </el-radio-group>

      <p class="oi-note">{{ t('settings.sync.import.note') }}</p>
    </div>

    <template #footer>
      <div class="oi-footer">
        <el-button :disabled="working" @click="onSkip">{{ t('settings.sync.import.skip') }}</el-button>
        <div class="oi-footer-right">
          <el-button :disabled="working" @click="visible = false">{{ t('common.cancel') }}</el-button>
          <el-button type="primary" :loading="working" @click="onConfirm">{{ t('settings.sync.import.run') }}</el-button>
        </div>
      </div>
    </template>
  </el-dialog>
</template>

<style scoped>
.oi-body { font-size: 13px; color: var(--jc-text-soft); }
.oi-desc { margin: 0 0 14px; line-height: 1.6; }
.oi-stats {
  display: flex; align-items: center; gap: 14px;
  padding: 12px 14px;
  background: var(--jc-bg-soft);
  border: 1px solid var(--jc-border);
  border-radius: 8px;
}
.oi-stat { text-align: center; min-width: 62px; }
.oi-stat-num { font-size: 20px; font-weight: 700; color: var(--jc-text); line-height: 1.2; }
.oi-stat-label { margin-top: 2px; font-size: 12px; color: var(--jc-text-muted); }
.oi-stat-muted .oi-stat-num { color: var(--jc-text-muted); font-weight: 500; }
.oi-stat-arrow { color: var(--jc-text-muted); font-size: 16px; }
.oi-warn {
  margin-top: 10px; padding: 8px 10px;
  font-size: 12px; line-height: 1.5;
  color: var(--el-color-warning);
  background: var(--el-color-warning-light-9);
  border: 1px solid var(--el-color-warning-light-7);
  border-radius: 6px;
}
.oi-mode-title { margin: 16px 0 6px; font-size: 12px; font-weight: 600; color: var(--jc-text-muted); }
.oi-modes { display: flex; flex-direction: column; gap: 6px; width: 100%; }
.oi-mode {
  height: auto; padding: 8px 10px; margin: 0; width: 100%;
  border: 1px solid var(--jc-border); border-radius: 8px;
  align-items: flex-start;
}
.oi-mode :deep(.el-radio__label) { white-space: normal; text-align: left; line-height: 1.4; }
.oi-mode-name { font-size: 13px; color: var(--jc-text); font-weight: 600; }
.oi-mode-desc { font-size: 12px; color: var(--jc-text-muted); font-weight: 400; margin-top: 2px; }
.oi-note { margin: 12px 0 0; font-size: 12px; color: var(--jc-text-muted); line-height: 1.5; }
.oi-footer { display: flex; align-items: center; justify-content: space-between; }
.oi-footer-right { display: flex; gap: 8px; }
</style>
