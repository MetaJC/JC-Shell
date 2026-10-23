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
  <!-- 终端左下角系统状态条：CPU / 内存 / 磁盘 占用率（进度条 + 百分比）+ 网络上下行实时速率 -->
  <!-- 整条可点：点任意位置（含三项指标、标题、条间空隙）都在「进度条 / 百分比」之间切换 -->
  <div
    v-if="connected"
    class="term-sys-status"
    :class="{ 'is-dimmed': dimmed, 'is-percent': showPercent }"
    :title="t('device.card.cpu') + ' / ' + t('device.card.mem') + ' / ' + t('device.card.disk') + '：点击切换 进度条 / 百分比'"
    data-swipe-ignore
    @click="togglePercent"
  >
    <div class="sys-cell sys-cell--metric" :title="`${t('device.card.cpu')} ${cpuUsage.toFixed(1)}%`">
      <span class="sys-label">{{ t('device.card.cpu') }}</span>
      <span v-if="!showPercent" class="sys-bar"><i :style="{ width: barWidth(cpuUsage), background: cpuGradient }" /></span>
      <span v-else class="sys-num">{{ cpuUsage.toFixed(0) }}%</span>
    </div>

    <div
      class="sys-cell sys-cell--metric"
      :title="`${t('device.card.mem')} ${memPercent.toFixed(1)}% · ${formatSize(memUsed)} / ${formatSize(memTotal)}`"
    >
      <span class="sys-label">{{ t('device.card.mem') }}</span>
      <span v-if="!showPercent" class="sys-bar"><i :style="{ width: barWidth(memPercent), background: memGradient }" /></span>
      <span v-else class="sys-num">{{ memPercent.toFixed(0) }}%</span>
    </div>

    <div
      class="sys-cell sys-cell--metric"
      :title="`${t('device.card.disk')} ${diskPercent.toFixed(1)}% · ${formatSize(diskUsed)} / ${formatSize(diskTotal)}`"
    >
      <span class="sys-label">{{ t('device.card.disk') }}</span>
      <span v-if="!showPercent" class="sys-bar"><i :style="{ width: barWidth(diskPercent), background: diskGradient }" /></span>
      <span v-else class="sys-num">{{ diskPercent.toFixed(0) }}%</span>
    </div>

    <div class="sys-cell sys-cell--net" :title="`${t('device.net.down')} ${formatSize(netRxRate)}/s · ${t('device.net.up')} ${formatSize(netTxRate)}/s`">
      <span class="sys-label">{{ t('device.card.net') }}</span>
      <span class="sys-rate sys-rate--down">↓{{ formatSize(netRxRate) }}/s</span>
      <span class="sys-rate sys-rate--up">↑{{ formatSize(netTxRate) }}/s</span>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 终端左下角系统状态条
 * 作者：JC
 * 纯展示、不拦截鼠标事件（pointer-events: none），避免遮挡终端选区与点击；
 * 终端铺满整个面板，状态条悬浮在面板最底边，只可能压住「视口最底一行」——
 * 该行有文字时降到 25% 不透明度让内容透出来，底行空了自动恢复 100%
 * （判定见 TerminalView 的 checkBottomOccupied）。
 * 数据源见 composables/useSystemMetrics（本地走 sysinfo 命令，SSH 走远端采样）。
 */
import { computed, ref } from 'vue'
import { useI18nStore } from '@/stores/i18n'
import { formatSize } from '@/utils/format'
import { useSystemMetrics } from '@/composables/useSystemMetrics'

const props = withDefaults(defineProps<{
  /** 当前会话是否已连接 */
  connected?: boolean
  /** 会话类型：本地 / SSH（决定采样通道） */
  kind?: 'local' | 'ssh'
  /** SSH 会话 ID */
  sessionId?: string
  /** 视口最底行被终端文字占用时降到 25% 不透明度，让内容透出来 */
  dimmed?: boolean
}>(), { connected: false, kind: 'local', sessionId: '', dimmed: false })

const i18n = useI18nStore()
const t = (k: string) => i18n.t(k)

const { cpuUsage, memUsed, memTotal, memPercent, diskUsed, diskTotal, diskPercent, netRxRate, netTxRate } =
  useSystemMetrics({
    connected: () => props.connected,
    kind: () => props.kind,
    sessionId: () => props.sessionId
  })

/**
 * CPU / 内存 / 磁盘 的展示模式：默认只显示进度条（不显示百分比，避免底栏拥挤）；
 * 点击三项中任意一项 → 三项同时切换为百分比（互斥展示，宽度与进度条模式相近，不会溢出）。
 * 网络速率不参与切换，始终保持现状。
 */
const showPercent = ref(false)
function togglePercent(): void {
  showPercent.value = !showPercent.value
}

/** 进度条宽度百分比（夹在 0-100，避免异常值溢出） */
function barWidth(percent: number): string {
  return Math.max(0, Math.min(100, percent)) + '%'
}

/** 占用率配色：越高越警戒，与设备信息面板口径一致 */
const cpuGradient = computed(() => {
  const u = cpuUsage.value
  if (u > 80) return 'linear-gradient(90deg, #ef4444, #f87171)'
  if (u > 60) return 'linear-gradient(90deg, #f59e0b, #fbbf24)'
  return 'linear-gradient(90deg, #3b82f6, #60a5fa)'
})
const memGradient = 'linear-gradient(90deg, #10b981, #34d399)'
const diskGradient = computed(() => {
  const p = diskPercent.value
  if (p > 85) return 'linear-gradient(90deg, #ef4444, #f87171)'
  if (p > 70) return 'linear-gradient(90deg, #f59e0b, #fbbf24)'
  return 'linear-gradient(90deg, #8b5cf6, #a78bfa)'
})
</script>

<style scoped>
.term-sys-status {
  position: absolute;
  left: 12px;
  bottom: 4px;
  z-index: 30;
  display: flex;
  align-items: center;
  gap: 12px;
  max-width: calc(100% - 24px);
  /* 无背景框：直接叠在终端底部，仅展示文字与进度条 */
  padding: 2px 0;
  font-size: 11px;
  line-height: 1;
  white-space: nowrap;
  overflow: hidden;
  /* 可点击：在「进度条 / 百分比」两种展示间切换。
     仅这条 15px 高的贴底窄条拦截点击，不影响终端其余区域的选区与操作 */
  pointer-events: auto;
  cursor: pointer;
  -webkit-tap-highlight-color: transparent;
  user-select: none;
  /* 底行有终端文字时淡到 25%（不遮挡内容但仍隐约可见）；底行空了自动恢复 */
  transition: opacity 0.25s ease;
  opacity: 1;
}
.term-sys-status.is-dimmed {
  opacity: 0.25;
}
.sys-cell {
  display: flex;
  align-items: center;
  gap: 5px;
  min-width: 0;
}
/* CPU / 内存 / 磁盘三项：点击切换展示形态（网络项只读、不参与） */
.sys-cell--metric {
  border-radius: 3px;
}
.term-sys-status:active {
  background: var(--jc-bg-hover);
}
.sys-label {
  color: var(--jc-text-muted);
  flex-shrink: 0;
}
.sys-bar {
  width: 36px;
  height: 3px;
  border-radius: 999px;
  background: var(--jc-bg);
  overflow: hidden;
  flex-shrink: 0;
}
.sys-bar > i {
  display: block;
  height: 100%;
  border-radius: 999px;
  transition: width 0.5s ease, background 0.3s ease;
}
.sys-num {
  width: 28px;
  text-align: right;
  color: var(--jc-text-soft);
  font-variant-numeric: tabular-nums;
  flex-shrink: 0;
}
.sys-cell--net {
  gap: 7px;
}
.sys-rate {
  font-variant-numeric: tabular-nums;
  flex-shrink: 0;
}
.sys-rate--down { color: #34d399; }
.sys-rate--up { color: #fbbf24; }
</style>
