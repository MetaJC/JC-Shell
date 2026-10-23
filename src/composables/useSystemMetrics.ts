// Copyright 2024-2026 OpenJC (https://www.openjc.cc)
// 
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// 
//     http://www.apache.org/licenses/LICENSE-2.0
// 
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

/**
 * 系统资源指标（CPU / 内存 / 磁盘占用率 + 网络实时速率）
 * 作者：JC
 * - 本地会话：走 Tauri sysinfo 命令（get_local_*），不依赖 SSH；
 * - SSH 会话：走 exec_remote_command 采样 /proc 与 free/df。
 * 与「设备信息」面板同源同口径（占用率按字节算百分比、速率按字节/秒），
 * 供终端左下角系统状态条等轻量展示复用。
 */
import { ref, computed, watch, onBeforeUnmount, type Ref, type ComputedRef } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { isUiFrozen } from '@/utils/uiFreeze'

export interface SystemMetricsSource {
  /** 是否已连接（未连接不采样，面板可据此隐藏） */
  connected: () => boolean
  /** 会话类型：local 读本机，ssh 读远端 */
  kind: () => 'local' | 'ssh'
  /** SSH 会话 ID（kind 为 ssh 时必填） */
  sessionId: () => string | undefined
}

export interface SystemMetrics {
  /** CPU 总使用率（%） */
  cpuUsage: Ref<number>
  memUsed: Ref<number>
  memTotal: Ref<number>
  /** 内存占用率（%） */
  memPercent: ComputedRef<number>
  diskUsed: Ref<number>
  diskTotal: Ref<number>
  /** 磁盘占用率（%） */
  diskPercent: ComputedRef<number>
  /** 网络下行速率（字节/秒） */
  netRxRate: Ref<number>
  /** 网络上行速率（字节/秒） */
  netTxRate: Ref<number>
  /** 首帧有效采样是否就绪 */
  ready: Ref<boolean>
  /** 立即刷新一次 */
  refresh: () => Promise<void>
}

interface LocalCpuInfo { usage: number; perCore: number[]; loadAvg: [number, number, number] }
interface LocalMemInfo { total: number; used: number; available: number; cached: number; swapTotal: number; swapUsed: number }
interface LocalDiskInfo { total: number; used: number; partitions: { mount: string; total: number; used: number }[] }
interface LocalNetInfo { rxRate: number; txRate: number }

/** /proc/stat 聚合行采样：idle = idle + iowait，total = 前 8 列之和 */
interface StatSample { idle: number; total: number }

function parseAggCpu(text: string): StatSample | null {
  for (const raw of text.split('\n')) {
    const m = raw.trim().match(/^cpu\s+(.*)$/)
    if (!m) continue
    const nums = m[1].split(/\s+/).map(Number).filter(n => !isNaN(n))
    if (nums.length < 5) continue
    return {
      idle: (nums[3] ?? 0) + (nums[4] ?? 0),
      total: nums.slice(0, 8).reduce((s, n) => s + n, 0)
    }
  }
  return null
}

/** 两次 /proc/stat 采样差值 → 使用率（%） */
function cpuUsageBetween(before: StatSample | null, after: StatSample | null): number {
  if (!before || !after) return 0
  const dt = after.total - before.total
  const di = after.idle - before.idle
  if (dt <= 0 || di < 0) return 0
  return Math.max(0, Math.min(100, Math.round((1 - di / dt) * 1000) / 10))
}

/** free -b 输出 → 内存总量/已用（字节） */
function parseRemoteMem(out: string): { total: number; used: number } {
  for (const l of out.split('\n')) {
    if (!l.startsWith('Mem:')) continue
    const p = l.trim().split(/\s+/)
    if (p.length >= 3) return { total: Number(p[1]) || 0, used: Number(p[2]) || 0 }
  }
  return { total: 0, used: 0 }
}

/** df -B1 输出 → 根分区（优先 "/"）容量，缺省时回退为各分区求和 */
function parseRemoteDisk(out: string): { total: number; used: number } {
  let rootTotal = 0
  let rootUsed = 0
  let sumTotal = 0
  let sumUsed = 0
  for (const l of out.split('\n')) {
    const line = l.trim()
    if (!line || line.startsWith('Filesystem')) continue
    const p = line.split(/\s+/)
    if (p.length < 6 || !/^\d+$/.test(p[1]) || !/^\d+$/.test(p[2])) continue
    const size = Number(p[1])
    if (size <= 0) continue
    const usedBytes = Number(p[2])
    sumTotal += size
    sumUsed += usedBytes
    if (p[5] === '/') { rootTotal = size; rootUsed = usedBytes }
  }
  if (rootTotal === 0 && sumTotal > 0) return { total: sumTotal, used: sumUsed }
  return { total: rootTotal, used: rootUsed }
}

/** 本地刷新间隔 / 远端刷新间隔（远端采样含 1s 双采样，放缓轮询） */
const LOCAL_INTERVAL = 2000
const REMOTE_INTERVAL = 5000

export function useSystemMetrics(source: SystemMetricsSource): SystemMetrics {
  const cpuUsage = ref(0)
  const memUsed = ref(0)
  const memTotal = ref(0)
  const diskUsed = ref(0)
  const diskTotal = ref(0)
  const netRxRate = ref(0)
  const netTxRate = ref(0)
  const ready = ref(false)

  const memPercent = computed(() => memTotal.value > 0 ? (memUsed.value / memTotal.value) * 100 : 0)
  const diskPercent = computed(() => diskTotal.value > 0 ? (diskUsed.value / diskTotal.value) * 100 : 0)

  let timer: ReturnType<typeof setInterval> | null = null
  // 网络速率需要相邻两次采样差值
  let netLastRx = 0
  let netLastTx = 0
  let netLastTime = 0

  async function loadLocal(): Promise<void> {
    const [cpuInfo, memInfo, diskInfo, netInfo] = await Promise.all([
      invoke<LocalCpuInfo>('get_local_cpu_info'),
      invoke<LocalMemInfo>('get_local_mem_info'),
      invoke<LocalDiskInfo>('get_local_disk_info'),
      invoke<LocalNetInfo>('get_local_net_rate')
    ])
    cpuUsage.value = cpuInfo.usage
    memTotal.value = memInfo.total
    memUsed.value = memInfo.used
    diskTotal.value = diskInfo.total
    diskUsed.value = diskInfo.used
    netRxRate.value = netInfo.rxRate
    netTxRate.value = netInfo.txRate
    ready.value = true
  }

  async function execRemote(command: string): Promise<string> {
    const sessionId = source.sessionId()
    if (!sessionId) return ''
    return await invoke<string>('exec_remote_command', { dto: { session_id: sessionId, command } })
  }

  async function loadRemote(): Promise<void> {
    const sessionId = source.sessionId()
    if (!sessionId) return
    const [cpuOut, memOut, diskOut, netOut] = await Promise.all([
      execRemote("grep '^cpu ' /proc/stat; echo '---SAMPLE---'; sleep 1; grep '^cpu ' /proc/stat"),
      execRemote('free -b'),
      execRemote('df -B1 -x tmpfs -x devtmpfs -x squashfs 2>/dev/null || df -B1'),
      execRemote('cat /proc/net/dev')
    ])

    const samples = cpuOut.replace(/\r/g, '').split('---SAMPLE---')
    cpuUsage.value = cpuUsageBetween(parseAggCpu(samples[0] ?? ''), parseAggCpu(samples[1] ?? ''))

    const mem = parseRemoteMem(memOut)
    memTotal.value = mem.total
    memUsed.value = mem.used

    const dk = parseRemoteDisk(diskOut)
    diskTotal.value = dk.total
    diskUsed.value = dk.used

    let rx = 0
    let tx = 0
    for (const l of netOut.split('\n')) {
      const p = l.trim().split(/\s+/)
      if (p.length >= 10) {
        rx += parseInt(p[1]) || 0
        tx += parseInt(p[9]) || 0
      }
    }
    const now = Date.now()
    if (netLastTime > 0) {
      const dt = (now - netLastTime) / 1000
      if (dt > 0) {
        netRxRate.value = Math.max(0, Math.round((rx - netLastRx) / dt))
        netTxRate.value = Math.max(0, Math.round((tx - netLastTx) / dt))
      }
    }
    netLastRx = rx
    netLastTx = tx
    netLastTime = now

    ready.value = true
  }

  async function refresh(): Promise<void> {
    if (!source.connected()) return
    try {
      if (source.kind() === 'local') await loadLocal()
      else await loadRemote()
    } catch (e) {
      console.error('[systemMetrics] load failed:', e)
    }
  }

  function stop(): void {
    if (timer) {
      clearInterval(timer)
      timer = null
    }
  }

  function start(): void {
    stop()
    // 会话切换后速率基线失效，重置避免出现巨大跳变
    netLastRx = 0
    netLastTx = 0
    netLastTime = 0
    ready.value = false
    if (!source.connected()) return
    const interval = source.kind() === 'local' ? LOCAL_INTERVAL : REMOTE_INTERVAL
    void refresh()
    timer = setInterval(() => {
      // 窗口拖拽中跳过：远端采样 + 图表重渲染会阻塞窗口移动
      if (!isUiFrozen()) void refresh()
    }, interval)
  }

  watch(
    () => [source.connected(), source.kind(), source.sessionId()] as const,
    start,
    { immediate: true }
  )
  onBeforeUnmount(stop)

  return {
    cpuUsage, memUsed, memTotal, memPercent,
    diskUsed, diskTotal, diskPercent,
    netRxRate, netTxRate, ready, refresh
  }
}
