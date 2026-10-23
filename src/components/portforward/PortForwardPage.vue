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
  <div class="pf-page h-full w-full flex flex-col min-h-0 bg-[var(--jc-bg)]">
    <!-- 工具栏 -->
    <header class="pf-bar shrink-0 flex items-center gap-2 px-3 h-10 border-b border-[var(--jc-border)]">
      <el-button size="small" type="primary" :icon="Plus" @click="openCreate">
        {{ t('portForward.btn.create') }}
      </el-button>
      <el-button
        size="small"
        :icon="VideoPlay"
        :disabled="!selected.length"
        @click="batchEnable(true)"
      >
        {{ t('portForward.btn.start') }}
      </el-button>
      <el-button
        size="small"
        :icon="VideoPause"
        :disabled="!selected.length"
        @click="batchEnable(false)"
      >
        {{ t('portForward.btn.stop') }}
      </el-button>
      <el-button
        size="small"
        :icon="Delete"
        :disabled="!selected.length"
        @click="batchRemove"
      >
        {{ t('portForward.btn.delete') }}
      </el-button>
      <span v-if="selected.length" class="pf-bar__count">
        {{ t('portForward.selected', { n: selected.length }) }}
      </span>
      <span class="flex-1" />
      <el-button size="small" :icon="RefreshRight" @click="reload">{{ t('portForward.btn.refresh') }}</el-button>
    </header>

    <!-- 规则列表 -->
    <div class="flex-1 min-h-0 p-3">
      <el-table
        v-if="store.rules.length"
        ref="tableRef"
        :data="store.rules"
        height="100%"
        size="small"
        row-key="id"
        :row-class-name="rowClassName"
        @selection-change="(rows: ForwardRule[]) => (selected = rows)"
      >
        <el-table-column type="selection" width="42" />
        <el-table-column :label="t('portForward.col.name')" min-width="220">
          <template #default="{ row }">
            <div class="pf-name">{{ row.name }}</div>
            <div class="pf-summary">{{ ruleSummary(row) }}</div>
          </template>
        </el-table-column>
        <el-table-column :label="t('portForward.col.mode')" width="110">
          <template #default="{ row }">
            <span class="pf-mode">{{ t(`portForward.mode.${row.mode}.title`) }}</span>
          </template>
        </el-table-column>
        <el-table-column :label="t('portForward.col.ssh')" min-width="180">
          <template #default="{ row }">
            <span class="pf-mono">{{ row.sshUser }}@{{ row.sshHost }}:{{ row.sshPort }}</span>
          </template>
        </el-table-column>
        <el-table-column :label="t('portForward.col.status')" width="92">
          <template #default="{ row }">
            <el-switch
              :model-value="row.enabled"
              size="small"
              @update:model-value="(v: boolean) => store.setEnabled(row.id, v)"
            />
          </template>
        </el-table-column>
        <el-table-column :label="t('portForward.col.action')" width="150" align="right">
          <template #default="{ row }">
            <div class="pf-acts">
              <el-button link :icon="Edit" @click="openEdit(row)" />
              <el-button link :icon="CopyDocument" @click="store.duplicate(row.id)" />
              <el-button link type="danger" :icon="Delete" @click="removeOne(row)" />
            </div>
          </template>
        </el-table-column>
      </el-table>

      <!-- 空状态：文字提示在上，选择模式的卡片区直接嵌在页面里（不再弹窗） -->
      <div v-else class="pf-empty">
        <div class="pf-empty__inner">
          <el-icon :size="42" color="var(--jc-text-muted)"><Switch /></el-icon>
          <div class="pf-empty__title">{{ t('portForward.empty.title') }}</div>
          <div class="pf-empty__desc">{{ t('portForward.empty.desc') }}</div>
          <ForwardModePicker class="pf-empty__picker" @select="onModeSelected" />
        </div>
      </div>
    </div>

    <!-- 第一步：选择模式 -->
    <ForwardModeDialog v-model="modeVisible" @select="onModeSelected" />

    <!-- 第二步：配置（页面式 / 交互式） -->
    <ForwardConfigDialog v-model="configVisible" :draft="draft" :is-new="isNew" @saved="onSaved" />
  </div>
</template>

<script setup lang="ts">
/**
 * 端口转发页（Port Forwarding）
 * 作者：JC
 *
 * 展示已创建的转发规则（隧道），支持启停、编辑、复制、删除与批量操作。
 * 创建流程：选择模式（本地/远程/动态，含架构示意图）→ 配置（页面式 / 交互式，见设置）。
 */
import { onMounted, ref } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import {
  CopyDocument, Delete, Edit, Plus, RefreshRight, Switch, VideoPause, VideoPlay,
} from '@element-plus/icons-vue'
import { useI18nStore } from '@/stores/i18n'
import {
  defaultRule, ruleSummary, usePortForwardStore,
  type ForwardMode, type ForwardRule,
} from '@/stores/portForward'
import ForwardModeDialog from './ForwardModeDialog.vue'
import ForwardModePicker from './ForwardModePicker.vue'
import ForwardConfigDialog from './ForwardConfigDialog.vue'

const store = usePortForwardStore()
const t = useI18nStore().t

const selected = ref<ForwardRule[]>([])
const modeVisible = ref(false)
const configVisible = ref(false)
const draft = ref<ForwardRule | null>(null)
const isNew = ref(true)

onMounted(() => { if (!store.loaded) void store.load() })

function reload(): void {
  void store.load()
}

/** 已开启（运行中）的转发规则行加高亮类 */
function rowClassName({ row }: { row: ForwardRule }): string {
  return row.enabled ? 'pf-row--on' : ''
}

function openCreate(): void {
  modeVisible.value = true
}

function onModeSelected(mode: ForwardMode): void {
  draft.value = defaultRule(mode)
  isNew.value = true
  configVisible.value = true
}

function openEdit(row: ForwardRule): void {
  draft.value = JSON.parse(JSON.stringify(row)) as ForwardRule
  isNew.value = false
  configVisible.value = true
}

function onSaved(rule: ForwardRule): void {
  // 「保存」按钮不关闭弹窗，同一条规则可能被连续保存多次：
  // 必须按 id 是否已存在来决定新增还是更新，只看 isNew 会在第二次保存时又插一条同 id 的规则
  if (store.rules.some(r => r.id === rule.id)) store.update(rule)
  else store.add(rule)
}

function batchEnable(enabled: boolean): void {
  store.setEnabledMany(selected.value.map(r => r.id), enabled)
}

function batchRemove(): void {
  const n = selected.value.length
  if (!n) return
  ElMessageBox.confirm(t('portForward.confirmDeleteMany', { n }), t('portForward.btn.delete'), { type: 'warning' })
    .then(() => {
      store.removeMany(selected.value.map(r => r.id))
      ElMessage.success(t('portForward.deleted'))
    })
    .catch(() => { /* 取消 */ })
}

function removeOne(row: ForwardRule): void {
  ElMessageBox.confirm(t('portForward.confirmDelete', { name: row.name }), t('portForward.btn.delete'), { type: 'warning' })
    .then(() => {
      store.remove(row.id)
      ElMessage.success(t('portForward.deleted'))
    })
    .catch(() => { /* 取消 */ })
}
</script>

<style scoped>
.pf-bar__count {
  font-size: 12px;
  color: var(--jc-accent);
}
.pf-name {
  font-size: 13px;
  color: var(--jc-text);
}
.pf-summary {
  font-size: 11px;
  color: var(--jc-text-muted);
}
.pf-mode {
  font-size: 12px;
  color: var(--jc-text-soft);
}
.pf-mono {
  font-family: Consolas, Menlo, monospace;
  font-size: 12px;
  color: var(--jc-text-soft);
}
/* 行尾操作按钮：默认 small 图标只有 14px 左右偏小，放大一档并留出可点击的空隙 */
.pf-acts {
  display: inline-flex;
  align-items: center;
}
.pf-acts :deep(.el-button.is-link) {
  font-size: 19px;
  padding: 3px;
  margin-left: 8px;
}
.pf-acts :deep(.el-button.is-link:first-child) {
  margin-left: 0;
}
.pf-acts :deep(.el-button.is-link .el-icon) {
  width: 19px;
  height: 19px;
}
.pf-empty {
  height: 100%;
  box-sizing: border-box;
  overflow: auto;
  padding: 20px;
}
/* 内容不足一屏时整体居中；超出时从顶部开始滚动 */
.pf-empty__inner {
  min-height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
}
.pf-empty__title {
  font-size: 16px;
  color: var(--jc-text);
}
.pf-empty__desc {
  font-size: 13px;
  color: var(--jc-text-muted);
  max-width: 520px;
  text-align: center;
  line-height: 1.6;
}
.pf-empty__picker {
  width: 100%;
  max-width: 1160px;
  margin-top: 18px;
}

/* ===== 已开启（运行中）的转发规则：整行高亮 ===== */
:deep(.el-table__row.pf-row--on > td.el-table__cell) {
  background-color: color-mix(in srgb, var(--jc-accent) 14%, transparent) !important;
}
:deep(.el-table__row.pf-row--on:hover > td.el-table__cell) {
  background-color: color-mix(in srgb, var(--jc-accent) 24%, transparent) !important;
}
/* 左侧强调条 */
:deep(.el-table__row.pf-row--on > td.el-table__cell:first-child) {
  box-shadow: inset 3px 0 0 0 var(--jc-accent);
}
/* 名称用强调色加粗，并在前面点一个"运行中"小圆点 */
.pf-row--on .pf-name {
  color: var(--jc-accent);
  font-weight: 600;
}
.pf-row--on .pf-name::before {
  content: '';
  display: inline-block;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--jc-accent);
  margin-right: 6px;
  vertical-align: middle;
  box-shadow: 0 0 0 3px color-mix(in srgb, var(--jc-accent) 25%, transparent);
}
.pf-row--on .pf-summary {
  color: var(--jc-text-soft);
}
</style>
