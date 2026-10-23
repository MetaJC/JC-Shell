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
    :title="t('menu.help.qqGroups')"
    width="640px"
    :close-on-click-modal="false"
    destroy-on-close
    class="jc-dialog qq-dialog"
  >
    <div v-if="qqGroup.loading" class="state">
      <el-icon class="is-loading"><Loading /></el-icon>
      <span>{{ t('common.loading') }}</span>
    </div>
    <div v-else-if="qqGroup.error" class="state error">{{ qqGroup.error }}</div>
    <div v-else-if="qqGroup.groups.length === 0" class="state">暂无QQ群数据</div>
    <div v-else class="qq-body">
      <!-- 左侧：群号列表 -->
      <div class="qq-left">
        <div class="left-header">群列表（{{ qqGroup.groups.length }}）</div>
        <div class="left-list">
          <div
            v-for="g in qqGroup.groups"
            :key="g.id"
            :class="['left-item', { 'is-active': selectedId === g.id, 'is-full': g.deleted === 1 }]"
            @click="onSelect(g)"
          >
            <div class="qq-text">
              <span class="qq-num">{{ g.qqNumber }}</span>
              <span v-if="g.groupName" class="qq-name">（{{ g.groupName }}）</span>
              <span v-if="g.deleted === 1" class="qq-tag-full">已满</span>
            </div>
            <el-icon v-if="g.deleted !== 1 && g.qrCodeUrl && selectedId === g.id" class="active-mark"><Check /></el-icon>
          </div>
        </div>
      </div>

      <!-- 右侧：当前选中群的二维码 -->
      <div class="qq-right">
        <div v-if="currentGroup" class="right-card">
          <div class="right-title">{{ currentGroup.groupName || `群号 ${currentGroup.qqNumber}` }}</div>
          <div v-if="currentGroup.qrCodeUrl && !qrFailed" class="right-qr">
            <img
              :src="currentGroup.qrCodeUrl"
              :alt="`QQ群${currentGroup.qqNumber}二维码`"
              @error="onQrError"
            />
          </div>
          <div v-else class="right-empty">
            {{ qrFailed ? '二维码加载失败，请检查网络或后端地址配置' : '该群暂未配置二维码' }}
          </div>
          <div class="right-num">
            <span class="num-label">群号</span>
            <span class="num-value">{{ currentGroup.qqNumber }}</span>
            <el-button size="small" text type="primary" @click="copyQq(currentGroup.qqNumber)">复制</el-button>
          </div>
        </div>
        <div v-else class="right-empty">暂无可用群二维码</div>
      </div>
    </div>
  </el-dialog>
</template>

<script setup lang="ts">
/**
 * 社区QQ群弹窗
 * 作者：JC
 * 左右两栏布局：左侧群号列表（已满的灰显 + 不可点），右侧显示选中群的二维码。
 * 默认选中最新一个未满（deleted !== 1）的群。
 */
import { ref, computed, watch } from 'vue'
import { ElMessage } from 'element-plus'
import { Loading, Check } from '@element-plus/icons-vue'
import { useI18nStore } from '@/stores/i18n'
import { useQqGroupStore, type QqGroupItem } from '@/stores/qqGroup'

const t = useI18nStore().t
const qqGroup = useQqGroupStore()

const visible = defineModel<boolean>({ default: false })

/** 当前选中的群 ID（默认有二维码的正常群） */
const selectedId = ref<number | null>(null)

const currentGroup = computed<QqGroupItem | undefined>(() =>
  qqGroup.groups.find(g => g.id === selectedId.value)
)

/** 弹窗打开 / 群列表刷新时初始化选中项：最新一个未满（deleted !== 1）的群 */
function pickDefaultSelectedId(): number | null {
  // store 已按 id 倒序（最新在前），find 第一个未满的即为"最新未满群"；全满则回退到第一个群
  const candidate =
    qqGroup.groups.find(g => g.deleted !== 1) ||
    qqGroup.groups[0]
  return candidate ? candidate.id : null
}

watch(visible, (open) => {
  if (open) selectedId.value = pickDefaultSelectedId()
})
// 异步 fetchGroups 返回后 groups 更新，若当前选中项已不在列表中则重置（首次打开时列表可能尚未返回）
watch(() => qqGroup.groups, (list) => {
  if (visible.value && list.length && !list.some(g => g.id === selectedId.value)) {
    selectedId.value = pickDefaultSelectedId()
  }
})

/**
 * 二维码加载失败标记。
 * 图片被 CSP 拦截、地址失效或网络不通时，浏览器只留下一个空白框，
 * 没有任何报错，极难排查。这里捕获 error 事件并给出可见提示。
 */
const qrFailed = ref(false)
function onQrError() {
  qrFailed.value = true
}
// 切换群时重置，给新选中的群一次重新加载的机会
watch(selectedId, () => { qrFailed.value = false })

function onSelect(g: QqGroupItem) {
  selectedId.value = g.id
}

async function copyQq(num: string) {
  try {
    await navigator.clipboard.writeText(num)
    ElMessage.success('已复制群号')
  } catch {
    ElMessage.error('复制失败')
  }
}
</script>

<style scoped>
.state {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 40px 0;
  color: var(--jc-text-soft);
}
.state.error { color: var(--el-color-danger); }

.qq-body {
  display: grid;
  grid-template-columns: 240px 1fr;
  gap: 16px;
  min-height: 320px;
}

/* 左侧列表 */
.qq-left {
  display: flex;
  flex-direction: column;
  background: var(--jc-bg-soft);
  border: 1px solid var(--jc-border);
  border-radius: 8px;
  overflow: hidden;
}
.left-header {
  padding: 8px 12px;
  font-size: 12px;
  color: var(--jc-text-soft);
  border-bottom: 1px solid var(--jc-border);
  background: var(--jc-bg-hover);
}
.left-list {
  flex: 1;
  overflow-y: auto;
  padding: 4px 0;
}
.left-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 12px;
  cursor: pointer;
  border-left: 3px solid transparent;
  transition: background .15s ease;
  user-select: none;
}
.left-item:hover { background: var(--jc-bg-hover); }
.left-item.is-active {
  background: var(--jc-bg-hover);
  border-left-color: var(--el-color-primary);
}
.qq-text {
  font-size: 13px;
  color: var(--jc-text);
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.qq-num {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  color: var(--jc-text);
}
.qq-name {
  color: var(--jc-text-soft);
  margin-left: 2px;
}
.qq-tag-off {
  margin-left: 6px;
  font-size: 11px;
  color: var(--el-color-danger);
  background: rgba(245, 108, 108, 0.1);
  padding: 1px 5px;
  border-radius: 3px;
}
.qq-tag-full {
  margin-left: 6px;
  font-size: 11px;
  color: var(--el-color-warning);
  background: rgba(230, 162, 60, 0.12);
  padding: 1px 5px;
  border-radius: 3px;
}
.active-mark {
  color: var(--el-color-primary);
  flex-shrink: 0;
}

/* 已满群：灰显 + 不划线 + 不可点击（业务语义：已满但仍存在） */
.left-item.is-full {
  cursor: not-allowed;
  opacity: 0.6;
}
.left-item.is-full .qq-num,
.left-item.is-full .qq-name {
  /* 不划线，仅灰显 */
  color: var(--jc-text-muted);
}
.left-item.is-full.is-active .qq-num { color: var(--jc-text-muted); }

/* 右侧二维码 */
.qq-right {
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--jc-bg-soft);
  border: 1px solid var(--jc-border);
  border-radius: 8px;
  padding: 16px;
}
.right-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  width: 100%;
}
.right-title {
  font-size: 14px;
  font-weight: 500;
  color: var(--jc-text);
  text-align: center;
}
.right-qr {
  width: 200px;
  height: 200px;
  background: #fff;
  border: 1px solid var(--jc-border);
  border-radius: 6px;
  padding: 8px;
  display: flex;
  align-items: center;
  justify-content: center;
}
.right-qr img {
  width: 100%;
  height: 100%;
  object-fit: contain;
}
.right-empty {
  color: var(--jc-text-muted);
  font-size: 13px;
  text-align: center;
  padding: 40px 20px;
}
.right-num {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
}
.num-label { color: var(--jc-text-soft); }
.num-value {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  color: var(--jc-text);
}
</style>
