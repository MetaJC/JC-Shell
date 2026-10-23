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
  <!-- 传输队列面板：从状态栏上方向上弹出；不透明底 + 亮边框 + 外发光阴影，边界清晰不突兀 -->
  <div class="transfer-queue absolute z-30 flex flex-col bg-[#252526] border border-[#5a5a64] rounded-lg overflow-hidden">
    <!-- 头部 -->
    <div class="shrink-0 h-8 px-3 flex items-center justify-between border-b border-[#3e3e42] bg-[#2d2d30] rounded-t-lg">
      <span class="text-xs text-gray-300">传输队列</span>
      <div class="flex items-center gap-1">
        <button
          class="text-[10px] px-1.5 py-0.5 rounded text-gray-400 hover:text-white hover:bg-white/10 transition-colors disabled:opacity-40"
          :disabled="!hasFinished"
          @click="store.clearFinished(sessionId)"
        >清除已完成</button>
        <button
          class="w-5 h-5 flex items-center justify-center rounded text-gray-400 hover:text-white hover:bg-white/10 transition-colors"
          title="关闭"
          @click="$emit('close')"
        ><el-icon size="12"><Close /></el-icon></button>
      </div>
    </div>

    <!-- 队列内容（高度随内容自适应，超出父级 maxHeight 时内部滚动） -->
    <div class="flex-1 min-h-0 overflow-y-auto p-2 space-y-3">
      <div v-if="uploads.length === 0 && downloads.length === 0" class="min-h-[84px] flex items-center justify-center text-gray-500 text-xs">
        暂无传输任务
      </div>
      <div v-for="group in groups" :key="group.title">
        <div v-if="group.items.length" class="space-y-1">
          <div class="text-[10px] text-gray-500 px-1">{{ group.title }}（{{ group.items.length }}）</div>
          <div
            v-for="it in group.items"
            :key="it.id"
            class="rounded bg-[#252526] border border-[#3e3e42] px-2 py-1.5"
          >
            <!-- 行1：左=图标+文件名+状态+按钮；右=百分比+剩余时间 -->
            <div class="flex items-center gap-2 h-6">
              <el-icon size="12" :class="statusClass(it)" class="shrink-0">
                <Upload v-if="it.direction === 'upload'" />
                <Download v-else />
              </el-icon>
              <span class="flex-1 min-w-0 truncate text-xs text-gray-100" :title="it.name">{{ it.name }}</span>
              <span class="text-[10px] text-gray-300 shrink-0 w-12 text-center">{{ statusText(it) }}</span>
              <button
                v-if="!it.done"
                class="w-5 h-5 flex items-center justify-center rounded text-gray-300 hover:text-red-400 hover:bg-white/10 transition-colors shrink-0"
                title="取消传输"
                @click="cancelTransfer(it)"
              ><el-icon size="12"><CircleClose /></el-icon></button>
              <button
                v-else
                class="w-5 h-5 flex items-center justify-center rounded text-gray-300 hover:text-white hover:bg-white/10 transition-colors shrink-0"
                title="打开所在目录"
                @click="$emit('openDir', it)"
              ><el-icon size="12"><FolderOpened /></el-icon></button>
              <!-- 右栏：百分比 + 剩余时间（不主动换行） -->
              <div class="shrink-0 w-28 flex items-center justify-end gap-1.5 text-[10px] text-gray-200 whitespace-nowrap">
                <span>{{ percentOf(it).toFixed(1) }}%</span>
                <span class="truncate min-w-0 text-right">{{ remainingText(it) }}</span>
              </div>
            </div>
            <!-- 行2：左=进度条；右=速度+总大小 -->
            <div class="flex items-center gap-2 h-5">
              <div class="flex-1 h-1 rounded bg-[#3e3e42] overflow-hidden">
                <div
                  class="h-full rounded transition-[width] duration-200"
                  :class="barClass(it)"
                  :style="{ width: percentOf(it) + '%' }"
                ></div>
              </div>
              <!-- 右栏：速度 + 总大小（不主动换行） -->
              <div class="shrink-0 w-28 flex items-center justify-end gap-1.5 text-[10px] text-gray-300 whitespace-nowrap">
                <span class="truncate min-w-0 text-right">{{ speedText(it) }}</span>
                <span class="shrink-0">{{ totalText(it) }}</span>
              </div>
            </div>
            <div v-if="it.error" class="mt-0.5 text-[10px] text-red-400 truncate" :title="friendlyText(it.error)">{{ friendlyText(it.error) }}</div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 文件传输队列面板（上传/下载分组，实时进度/速度/状态）
 * 作者：JC
 * 进行中任务可点击取消按钮，调用后端 cancel_transfer 命令置位取消标志
 */
import { computed } from 'vue'
import { Close, Upload, Download, FolderOpened, CircleClose } from '@element-plus/icons-vue'
import { ElMessage } from 'element-plus'
import { handleApiError, friendlyText } from '@/utils/errorHandler'
import { invoke } from '@/api'
import { useTransferStore, type TransferItem } from '@/stores/transfer'
import { formatSizeCompact } from '@/utils/format'

const props = defineProps<{ sessionId: string }>()
defineEmits<{
  (e: 'close'): void
  (e: 'openDir', item: TransferItem): void
}>()

const store = useTransferStore()
const list = store.bySession(props.sessionId)

const uploads = computed(() => list.value.filter(i => i.direction === 'upload'))
const downloads = computed(() => list.value.filter(i => i.direction === 'download'))
const groups = computed(() => [
  { title: '上传队列', items: uploads.value },
  { title: '下载队列', items: downloads.value }
])
const hasFinished = computed(() => list.value.some(i => i.done))

function percentOf(it: TransferItem): number {
  if (it.done && !it.error) return 100
  if (it.total > 0) return Math.min(100, Math.round((it.transferred / it.total) * 100))
  return 0
}
function statusText(it: TransferItem): string {
  if (it.error) return '失败'
  if (it.done) return '完成'
  return '传输中'
}
function statusClass(it: TransferItem): string {
  if (it.error) return 'text-red-400'
  if (it.done) return 'text-green-400'
  return 'text-blue-400'
}

/** 进度条颜色：active=蓝、done=绿、error=红 */
function barClass(it: TransferItem): string {
  if (it.error) return 'bg-red-500/70'
  if (it.done) return 'bg-green-500/70'
  return 'bg-blue-500/60'
}

/** 剩余时间：>=1h "1小时23分45秒"；>=1m "23分45秒"；>0 "45秒"；无效 "--" */
function remainingText(it: TransferItem): string {
  if (it.error || it.done) return ''
  if (it.total <= 0 || it.speed <= 0) return '剩余 --'
  const left = it.total - it.transferred
  if (left <= 0) return '剩余 0秒'
  const sec = Math.round(left / it.speed)
  if (sec >= 3600) {
    const h = Math.floor(sec / 3600)
    const m = Math.floor((sec % 3600) / 60)
    const s = sec % 60
    return `剩余 ${h}小时${m}分${s}秒`
  }
  if (sec >= 60) {
    const m = Math.floor(sec / 60)
    const s = sec % 60
    return `剩余 ${m}分${s}秒`
  }
  return `剩余 ${sec}秒`
}

/** 速度文本：紧凑无空格（"12.5MB/s"），无效 "--" */
function speedText(it: TransferItem): string {
  if (it.speed > 0) return formatSizeCompact(it.speed) + '/s'
  return '-'
}

/** 文件总大小文本：紧凑无空格（"5.2GB"），未知 "-" */
function totalText(it: TransferItem): string {
  if (it.total > 0) return formatSizeCompact(it.total)
  return '-'
}

/** 取消进行中的传输：调用后端 cancel_transfer 置位取消标志，传输循环在下一块边界响应 */
async function cancelTransfer(it: TransferItem): Promise<void> {
  if (it.done) return
  try {
    await invoke<boolean>('cancel_transfer', { transferId: it.id })
    ElMessage.info(`已请求取消：${it.name}`)
  } catch (e: any) {
    handleApiError(e, 'components/TransferQueuePanel.cancelTransfer', {
      prefix: '取消失败',
      url: `cancel_transfer/${it.id}`,
    })
  }
}
</script>

<style scoped>
.transfer-queue {
  /* 定位由父级 queuePanelStyle 提供（底部锚定）；双层阴影：贴近感 + 扩散投影，边界清晰 */
  box-shadow:
    0 0 0 1px rgba(0, 0, 0, 0.4),
    0 -4px 10px rgba(0, 0, 0, 0.3),
    0 8px 28px rgba(0, 0, 0, 0.45);
  animation: queue-slide-up 0.18s ease-out;
}
@keyframes queue-slide-up {
  from { opacity: 0; transform: translateY(12px); }
  to { opacity: 1; transform: translateY(0); }
}
</style>
