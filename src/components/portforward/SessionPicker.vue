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
  <div class="session-picker">
    <div class="sp-head">
      <span class="sp-title">{{ t('portForward.sessionPicker.title') }}</span>
      <el-icon class="sp-close" @click="emit('close')"><Close /></el-icon>
    </div>

    <div v-if="rows.length === 0" class="sp-empty">
      {{ t('portForward.sessionPicker.empty') }}
    </div>

    <div v-else class="sp-list">
      <div
        v-for="row in rows"
        :key="row.node.id"
        class="sp-row"
        :class="row.node.type === 'folder' ? 'is-folder' : 'is-host'"
        :style="{ paddingLeft: row.depth * 14 + 8 + 'px' }"
        @click="onRowClick(row.node)"
      >
        <template v-if="row.node.type === 'folder'">
          <el-icon size="11" class="sp-caret" :class="{ 'is-open': expanded.has(row.node.id) }">
            <ArrowRight />
          </el-icon>
          <el-icon size="14" color="#fbbf24" class="sp-ico shrink-0">
            <FolderOpened v-if="expanded.has(row.node.id)" />
            <Folder v-else />
          </el-icon>
          <span class="sp-name">{{ row.node.name }}</span>
        </template>
        <template v-else>
          <span class="sp-caret sp-caret--blank"></span>
          <el-icon size="14" color="var(--jc-text-muted)" class="sp-ico shrink-0">
            <Monitor />
          </el-icon>
          <span class="sp-name">{{ row.node.name }}</span>
          <span class="sp-addr">{{ formatAddr(row.node.addr) }}</span>
        </template>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 会话选择面板（端口转发「选择 SSH 会话」用）
 * 作者：JC
 *
 * 视觉样式照搬终端页左侧「会话」窗口（使用相同的图标/缩进/悬停色），
 * 但只支持「鼠标单击选择会话」：不提供右键菜单、不响应双击进入终端、不拖拽。
 * 单击某个会话即把它的连接信息回填到表单（由父组件处理）。
 */
import { computed, reactive } from 'vue'
import { ArrowRight, FolderOpened, Folder, Monitor, Close } from '@element-plus/icons-vue'
import { useI18nStore } from '@/stores/i18n'
import { useHostStore, type HostItem } from '@/stores/host'

const emit = defineEmits<{ (e: 'pick', host: HostItem): void; (e: 'close'): void }>()
const t = useI18nStore().t
const hostStore = useHostStore()

type Row = { node: HostItem & { children?: HostItem[] }; depth: number }

/** 默认展开所有文件夹（会话数量不大，一次性展开便于挑选） */
const expanded = reactive(new Set<string>(hostStore.treeData.filter(n => n.type === 'folder').map(n => n.id)))

/** 把树拍平成可见行（文件夹按展开态决定是否展开子级） */
const rows = computed<Row[]>(() => {
  const out: Row[] = []
  const walk = (list: (HostItem & { children?: HostItem[] })[], depth: number) => {
    for (const node of list) {
      out.push({ node, depth })
      if (node.type === 'folder' && node.children?.length && expanded.has(node.id)) {
        walk(node.children, depth + 1)
      }
    }
  }
  walk(hostStore.treeData as Row['node'][], 0)
  return out
})

function onRowClick(node: HostItem): void {
  if (node.type === 'folder') {
    if (expanded.has(node.id)) expanded.delete(node.id)
    else expanded.add(node.id)
    return
  }
  emit('pick', node)
}

function formatAddr(addr: string): string {
  const parts = addr.split('@')
  if (parts.length >= 2) return parts[1].split(':')[0]
  return addr
}
</script>

<style scoped>
.session-picker {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--jc-bg-soft);
  border-left: 1px solid var(--jc-border);
}
.sp-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 12px;
  border-bottom: 1px solid var(--jc-border);
  flex-shrink: 0;
}
.sp-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--jc-text);
}
.sp-close {
  cursor: pointer;
  color: var(--jc-text-muted);
  font-size: 15px;
}
.sp-close:hover { color: var(--jc-text); }
.sp-empty {
  padding: 24px 12px;
  text-align: center;
  font-size: 12px;
  color: var(--jc-text-muted);
}
.sp-list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 4px 0;
}
/* 行样式与终端页 HostNode 对齐：缩进、悬停、字号、图标间距 */
.sp-row {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 5px 8px;
  margin: 0 4px;
  border-radius: 6px;
  font-size: 13px;
  color: var(--jc-text);
  cursor: pointer;
  transition: background-color 0.12s ease;
}
.sp-row:hover { background: var(--jc-bg-hover); }
.sp-row.is-host:hover { background: rgba(59, 130, 246, 0.12); }
.sp-caret {
  width: 12px;
  flex-shrink: 0;
  transition: transform 0.18s ease;
}
.sp-caret.is-open { transform: rotate(90deg); }
.sp-caret--blank { visibility: hidden; }
.sp-ico { margin-left: 2px; }
.sp-name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.sp-addr {
  flex-shrink: 0;
  font-size: 10px;
  color: var(--jc-text-muted);
  max-width: 40%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
