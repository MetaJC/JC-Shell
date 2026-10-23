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
  <div class="process-view">
    <div class="proc-toolbar">
      <span class="text-[10px] text-[var(--jc-text-muted)]">共 {{ rows.length }} 进程 · 按 {{ sortKey === 'cpu' ? 'CPU' : '内存' }} 降序</span>
      <el-button size="small" text class="!h-5 !px-1" title="刷新" @click="$emit('refresh')">
        <el-icon size="11"><Refresh /></el-icon>
      </el-button>
    </div>
    <div class="proc-table">
      <div class="proc-thead" :style="{ gridTemplateColumns: gridCols }">
        <span class="col-pid">PID</span>
        <span class="col-name">进程名</span>
        <span v-if="!hideCpu" class="col-cpu">CPU%</span>
        <span v-if="!hideMem" class="col-mem">内存</span>
        <span v-if="showPorts" class="col-ports">端口</span>
      </div>
      <div class="proc-tbody">
        <div
          v-for="p in sortedRows"
          :key="p.pid"
          class="proc-row"
          :class="{ active: ctxMenu.pid === p.pid }"
          :style="{ gridTemplateColumns: gridCols }"
          @contextmenu.prevent="onCtx($event, p)"
          :title="p.path"
        >
          <span class="col-pid">{{ p.pid }}</span>
          <span class="col-name truncate">{{ p.name }}</span>
          <span v-if="!hideCpu" class="col-cpu" :class="p.cpu > 50 ? 'text-red-400' : 'text-gray-300'">{{ p.cpu.toFixed(1) }}</span>
          <span v-if="!hideMem" class="col-mem" :class="p.mem > 50 ? 'text-amber-400' : 'text-gray-300'">{{ formatMem(p) }}</span>
          <span v-if="showPorts" class="col-ports">{{ formatPorts(p.ports) }}</span>
        </div>
        <div v-if="!sortedRows.length" class="text-center text-gray-600 text-[11px] py-2" :style="{ gridColumn: '1 / -1' }">无进程数据</div>
      </div>
    </div>

    <!-- 右键菜单 -->
    <div
      v-if="ctxMenu.visible"
      class="ctx-menu"
      :style="{ left: ctxMenu.x + 'px', top: ctxMenu.y + 'px' }"
      @click.stop
    >
      <div class="ctx-item" @click="onKill">
        <el-icon size="12"><Delete /></el-icon>
        <span class="ml-1">杀掉进程 (kill -9 {{ ctxMenu.pid }})</span>
      </div>
    </div>

  </div>
</template>

<script setup lang="ts">
/**
 * 进程视图（CPU/内存卡共用；独立进程卡复用全字段）
 * 进程名列自适应宽度（max 35%），其他列固定宽度
 * 作者：JC
 */
import { computed, onBeforeUnmount, reactive } from 'vue'
import { Refresh, Delete } from '@element-plus/icons-vue'

export interface ProcessInfo {
  pid: number
  name: string
  path: string
  /** CPU 使用率百分比（0-100） */
  cpu: number
  /** 内存使用率百分比（0-100），仅排序用 */
  mem: number
  /** 实际内存占用（MB），显示用；缺省回退为 mem% */
  memBytes?: number
  disk?: number
  net?: number
  ports?: number[]
}

const props = withDefaults(defineProps<{
  rows: ProcessInfo[]
  sortKey: 'cpu' | 'mem'
  hideCpu?: boolean
  hideMem?: boolean
  showPorts?: boolean
}>(), { hideCpu: false, hideMem: false, showPorts: false })
const emit = defineEmits<{ (e: 'kill', pid: number): void; (e: 'refresh'): void }>()

const sortedRows = computed(() =>
  [...props.rows].sort((a, b) => b[props.sortKey] - a[props.sortKey]).slice(0, 30)
)

/**
 * 固定列宽 [PID, CPU%, 内存, 端口]（进程名列用 CSS 百分比自适应，不参与存储）
 */
const FIXED_WIDTHS = [42, 46, 62, 54]

/**
 * 生成 grid-template-columns：进程名列用 minmax(40px, 35%) 自适应（不超过卡片宽度 35%），
 * 其他列用固定 px。列之间间距保证均衡。
 */
const gridCols = computed(() => {
  const parts: string[] = [`${FIXED_WIDTHS[0]}px`]
  parts.push('minmax(40px, 35%)')
  if (!props.hideCpu) parts.push(`${FIXED_WIDTHS[1]}px`)
  if (!props.hideMem) parts.push(`${FIXED_WIDTHS[2]}px`)
  if (props.showPorts) parts.push(`${FIXED_WIDTHS[3]}px`)
  return parts.join(' ')
})

/** 内存显示：有 memBytes 显示 MB，否则回退百分比 */
function formatMem(p: ProcessInfo): string {
  if (p.memBytes != null) return p.memBytes.toFixed(0) + 'MB'
  return p.mem.toFixed(1) + '%'
}

/** 端口格式化 */
function formatPorts(ports?: number[]): string {
  if (!ports || ports.length === 0) return '-'
  if (ports.length <= 2) return ports.join(',')
  return `${ports.slice(0, 2).join(',')} +${ports.length - 2}`
}

// ========== 右键菜单 ==========
const ctxMenu = reactive({ visible: false, x: 0, y: 0, pid: 0 })

function onCtx(e: MouseEvent, p: ProcessInfo) {
  ctxMenu.visible = true
  ctxMenu.x = e.clientX
  ctxMenu.y = e.clientY
  ctxMenu.pid = p.pid
  window.addEventListener('click', close, { once: true })
}
function close() { ctxMenu.visible = false }

function onKill() {
  const pid = ctxMenu.pid
  ctxMenu.visible = false
  emit('kill', pid)
}

onBeforeUnmount(() => {
  window.removeEventListener('click', close)
})
</script>

<style scoped>
.process-view { width: 100%; }
.proc-toolbar {
  display: flex; align-items: center; justify-content: space-between;
  margin-bottom: 4px; padding: 0 2px;
}
.proc-table {
  border: 1px solid var(--jc-bg); border-radius: 4px;
  font-size: 11px; max-height: 380px;
  overflow: auto; /* 纵向滚动显示全部进程，横向滚动支持窄卡片5列布局 */
}
.proc-thead, .proc-row {
  display: grid;
  align-items: center;
}
.proc-thead {
  background-color: var(--jc-bg-soft); color: var(--jc-text-muted);
  border-bottom: 1px solid var(--jc-border);
  position: sticky; top: 0; z-index: 10;
  font-weight: 600;
}
.proc-row {
  cursor: context-menu; border-bottom: 1px solid var(--jc-bg);
  transition: background-color 0.1s;
}
.proc-row:hover { background-color: rgba(59, 130, 246, 0.08); }
.proc-row.active { background-color: rgba(59, 130, 246, 0.15); }

.col-pid, .col-cpu, .col-mem, .col-ports {
  display: block; min-width: 0;
  font-variant-numeric: tabular-nums;
  padding: 0 4px;
  text-align: center;
}
.col-name {
  display: block; min-width: 0;
  padding: 0 4px;
  text-align: center;
}

.ctx-menu {
  position: fixed; z-index: 9999;
  min-width: 180px; padding: 4px;
  background-color: var(--jc-bg-hover);
  border: 1px solid var(--jc-border);
  border-radius: 6px;
  box-shadow: 0 8px 24px rgba(0,0,0,0.5);
  color: var(--jc-text); font-size: 12px;
}
.ctx-item {
  display: flex; align-items: center; gap: 6px;
  padding: 6px 8px; border-radius: 4px; cursor: pointer;
}
.ctx-item:hover { background-color: rgba(239, 68, 68, 0.15); color: #f87171; }
</style>
