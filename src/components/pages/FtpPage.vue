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
  <div class="ftp-page">
    <!-- 顶栏：标题 + 添加会话窗口（+） + 分屏布局按钮（与终端模式一致） -->
    <div class="ftp-head">
      <el-icon size="14" class="ftp-head-ico"><UploadFilled /></el-icon>
      <span class="ftp-title">{{ t('dock.ftp') }}</span>
      <el-tooltip :content="t('ftp.addSession')" placement="bottom" :show-after="400">
        <button
          class="ftp-add"
          :class="{ 'is-max': panes.length >= MAX_PANES }"
          :disabled="panes.length >= MAX_PANES"
          :title="panes.length >= MAX_PANES ? t('ftp.maxPanes') : t('ftp.addSession')"
          @click="openPicker(null)"
        >
          <el-icon size="14"><Plus /></el-icon>
        </button>
      </el-tooltip>
      <!-- 分屏布局按钮（右侧固定）：左右 / 上下 / 网格；点击已激活的按钮恢复自动布局 -->
      <div class="ftp-split">
        <button
          v-for="btn in splitButtons"
          :key="btn.mode"
          class="ftp-split-btn"
          :class="{ 'is-active': layoutMode === btn.mode }"
          :title="btn.tooltip"
          @click="onSplitClick(btn.mode)"
        >
          <el-icon size="14"><component :is="btn.icon" /></el-icon>
        </button>
      </div>
    </div>

    <!-- 窗格区：自动（≤3 左右 / ≥4 网格）或手动指定 左右 / 上下 / 网格；分界线可拖拽调整尺寸 -->
    <div
      ref="layoutRef"
      class="ftp-panes"
      :class="containerClass"
      :style="layoutStyle"
    >
      <template v-for="(pane, i) in panes" :key="pane.id">
        <section class="ftp-pane" :style="paneStyle(i)">
          <!-- 窗格头：左上角会话按钮（点击呼出会话列表重连；悬停时圆点变 ×，点 × 关闭窗格） -->
          <header class="pane-head">
            <button
              class="pane-session"
              :title="t('ftp.pane.pickSession')"
              @click="openPicker(pane.id)"
            >
              <!-- 状态点 / 关闭按钮：悬停会话按钮时圆点被红色 × 替代，点击 × 关闭所属窗格 -->
              <span class="ps-swap">
                <span class="ps-dot" :class="pane.connecting ? 'busy' : (pane.connected ? 'ok' : 'off')" />
                <span
                  class="ps-close"
                  :title="t('ftp.pane.close')"
                  @click.stop="closePane(pane.id)"
                >
                  <el-icon :size="11"><Close /></el-icon>
                </span>
              </span>
              <span class="ps-label" :class="{ 'is-muted': !pane.connected && !pane.connecting }">
                {{ pane.connecting ? t('ftp.pane.connecting') : pane.label }}
              </span>
            </button>
            <!-- 文件系统工具条容器：FileSystemBrowser 通过 Teleport 把工具条渲染到这里（与选择会话按钮同行、靠右） -->
            <div :id="'fs-tb-' + pane.id" class="pane-fs-toolbar" />
          </header>
          <!-- 文件系统面板：本地 / 远程 SFTP 共用同一组件 -->
          <div class="pane-body">
            <FileSystemBrowser
              :session-id="pane.sessionId"
              :connected="pane.connected"
              :kind="pane.kind"
              :hostname="pane.label"
              :grip="false"
              :hide-sync="true"
              :hide-collapse="true"
              :toolbar-teleport="'#fs-tb-' + pane.id"
            />
          </div>
          <!-- 本窗格专属的会话选择抽屉：从「所属分屏」右侧滑出，仅覆盖当前窗格 -->
          <transition name="ftp-drawer">
            <div
              v-if="pickerVisible && pickerTargetId === pane.id"
              class="pane-picker-layer"
              @click.self="closePicker"
            >
              <aside class="pane-picker-panel">
                <SessionPicker @pick="onPickSession" @close="closePicker" />
              </aside>
            </div>
          </transition>
        </section>
        <!-- 左右 / 上下分屏：相邻窗格间可拖拽分界线 -->
        <SplitBar
          v-if="effMode !== 'grid' && i < panes.length - 1"
          :orientation="effMode === 'row' ? 'vertical' : 'horizontal'"
          @drag="delta => onFlexDrag(i, delta)"
        />
      </template>
      <!-- 网格分屏：列 / 行可拖拽分界线（列线按行生成，末行不满时由最后一个窗格横跨补齐） -->
      <template v-if="effMode === 'grid'">
        <SplitBar
          v-for="d in gridColSlots"
          :key="d.key"
          orientation="vertical"
          :style="{ gridColumn: String(d.gap * 2), gridRow: String(d.row * 2 + 1) }"
          @drag="delta => onGridColDrag(d.gap, delta)"
        />
        <SplitBar
          v-for="r in gridRowGapCount"
          :key="'grid-row-' + r"
          orientation="horizontal"
          :style="{ gridRow: String(r * 2), gridColumn: `1 / ${gridCols * 2}` }"
          @drag="delta => onGridRowDrag(r - 1, delta)"
        />
      </template>
    </div>

    <!-- 顶栏「+」的会话选择抽屉：从整个页面右侧滑出（与端口转发同一套 SessionPicker 交互） -->
    <transition name="ftp-drawer">
      <div
        v-if="pickerVisible && pickerTargetId === null"
        class="picker-layer"
        @click.self="pickerVisible = false"
      >
        <aside class="picker-panel">
          <SessionPicker @pick="onPickSession" @close="pickerVisible = false" />
        </aside>
      </div>
    </transition>
  </div>
</template>

<script setup lang="ts">
/**
 * FTP 模式页
 * 作者：JC
 *
 * 多窗格文件管理器：
 *  - 初始仅一个「本地」窗格（默认打开本地家目录）
 *  - 顶栏「+」/ 窗格头会话按钮 → 右侧滑出会话列表（复用端口转发的 SessionPicker）
 *  - 选中会话后为窗格建立独立 SSH 会话（SFTP 能力挂在 SSH 会话上），FileSystemBrowser 展示
 *  - ≤3 窗格左右分屏、≥4 自动网格平铺，分界线均可拖拽；上限 9 个窗口
 */
import { ref, computed, watch, onBeforeUnmount, h, defineComponent } from 'vue'
import { ElMessage } from 'element-plus'
import { UploadFilled, Plus, Close } from '@element-plus/icons-vue'
import FileSystemBrowser from '@/components/filesystem/FileSystemBrowser.vue'
import SessionPicker from '@/components/portforward/SessionPicker.vue'
import SplitBar from '@/components/common/SplitBar.vue'
import { useI18nStore } from '@/stores/i18n'
import { useHostStore, type HostItem } from '@/stores/host'
import { useSettingsStore } from '@/stores/settings'
import { invoke } from '@/api'
import { resolveSshProxy } from '@/utils/sshProxy'

const t = useI18nStore().t
const hostStore = useHostStore()
const prefs = useSettingsStore()

/** 平铺类页面（FTP / 中断模式）统一窗口上限 */
const MAX_PANES = 9

// ===== 窗格模型 =====
interface FtpPane {
  id: string
  kind: 'local' | 'ssh'
  /** 后端会话 ID；local 窗格用固定占位 id（FileSystemBrowser 只要求非空） */
  sessionId: string
  connected: boolean
  connecting: boolean
  /** 展示文案：本地 / user@host */
  label: string
  hostId?: string
}

let paneSeq = 0
const newPaneId = () => `ftp-pane-${++paneSeq}`

/** 默认窗格：本地文件系统（FileSystemBrowser 会自动定位到家目录）。
 *  本地窗格用唯一 id 作为 sessionId（不依赖 'local' 占位），保证多本地窗格互拖时
 *  被当作跨会话传输（复制保留源），且各自独立显示传输进度。 */
function makeLocalPane(): FtpPane {
  return { id: newPaneId(), kind: 'local', sessionId: newPaneId(), connected: true, connecting: false, label: t('ftp.pane.local') }
}

const panes = ref<FtpPane[]>([makeLocalPane()])

// ===== 会话选择抽屉 =====
const pickerVisible = ref(false)
/** 目标窗格 id；null 表示通过顶栏「+」新增窗格 */
const pickerTargetId = ref<string | null>(null)

function openPicker(paneId: string | null): void {
  if (paneId === null && panes.value.length >= MAX_PANES) {
    ElMessage.warning(t('ftp.maxPanes'))
    return
  }
  pickerTargetId.value = paneId
  pickerVisible.value = true
  // 会话列表数据可能尚未加载（首次进入本页）
  hostStore.load().catch(() => { /* 列表加载失败时面板内会显示空态 */ })
}

function closePicker(): void {
  pickerVisible.value = false
}

function onPickSession(host: HostItem): void {
  pickerVisible.value = false
  const targetId = pickerTargetId.value
  if (targetId === null) {
    // 新增窗格
    if (panes.value.length >= MAX_PANES) { ElMessage.warning(t('ftp.maxPanes')); return }
    const pane: FtpPane = {
      id: newPaneId(), kind: 'ssh', sessionId: '', connected: false, connecting: true,
      label: hostLabel(host), hostId: host.id,
    }
    panes.value.push(pane)
    // 用 paneId 从响应式数组取回代理再操作；直接改写本地变量不会触发视图更新
    void connectPane(pane.id, host, true)
    return
  }
  // 重连已有窗格：先回收旧会话再建新连接
  const pane = panes.value.find(p => p.id === targetId)
  if (!pane) return
  if (pane.kind === 'ssh' && pane.sessionId) {
    invoke('close_session', { sessionId: pane.sessionId }).catch(() => {})
  }
  pane.sessionId = ''
  pane.connected = false
  pane.connecting = true
  pane.hostId = host.id
  pane.label = hostLabel(host)
  void connectPane(pane.id, host, false)
}

/** 窗格头展示：user@host */
function hostLabel(host: HostItem): string {
  const at = host.addr.indexOf('@')
  const hostPart = at >= 0 ? host.addr.slice(at + 1) : host.addr
  const hostname = hostPart.split(':')[0]
  const cfg = host.config || {}
  const username = cfg.username?.trim() || (at >= 0 ? host.addr.slice(0, at) : '') || 'root'
  return `${username}@${hostname}`
}

/** 为窗格建立独立 SSH 会话（SFTP 挂在 SSH 会话上），链路与「测试连接」一致；
 *  paneId + removeOnFail：通过 paneId 从响应式数组取回代理再改写，确保视图实时刷新；
 *  removeOnFail：新增窗格连接失败时直接移除该窗格，避免留下空窗格 */
async function connectPane(paneId: string, host: HostItem, removeOnFail: boolean): Promise<void> {
  const pane = panes.value.find(p => p.id === paneId)
  if (!pane) return
  const cfg = host.config || {}
  // addr 形如 user@host:port；config 里的 username 优先
  const at = host.addr.indexOf('@')
  const hostPart = at >= 0 ? host.addr.slice(at + 1) : host.addr
  const addrHost = hostPart.split(':')[0]
  const addrPort = Number(hostPart.split(':')[1]) || cfg.port || 22
  const username = cfg.username?.trim() || (at >= 0 ? host.addr.slice(0, at) : '') || 'root'
  try {
    const proxy = await resolveSshProxy(prefs.settings)
    // 密钥托管内容先落盘换取可认证的绝对路径
    let keyPath = cfg.keyPath || null
    if (cfg.authType === 'key' && cfg.keyContent) {
      keyPath = await invoke<string>('materialize_key', { content: cfg.keyContent })
    }
    const timeoutSecs = Math.max(5, Math.min(prefs.settings.sshConnectTimeout || 15, 15))
    const session = await invoke<{ id: string }>('connect_ssh', {
      dto: {
        host: addrHost,
        port: addrPort,
        username,
        auth_type: cfg.authType === 'key' ? 'private_key' : 'password',
        password: cfg.authType === 'key' ? null : (cfg.password ?? null),
        private_key_path: cfg.authType === 'key' ? keyPath : null,
        passphrase: cfg.authType === 'key' ? (cfg.passphrase || null) : null,
        // 不挂 PTY 终端，尺寸仅占位；SFTP 只依赖会话本身
        cols: 80,
        rows: 24,
        connect_timeout_secs: timeoutSecs,
        keep_alive_secs: null,
        proxy_addr: proxy.addr,
        proxy_kind: proxy.kind,
      },
    }, { timeoutMs: (timeoutSecs + 5) * 1000 })
    pane.sessionId = session.id
    pane.connected = true
  } catch (e: any) {
    // 新增的窗格连接失败直接移除；重连的窗格保留并标记未连接（可再次点击重试）
    if (removeOnFail) {
      const idx = panes.value.findIndex(p => p.id === paneId)
      if (idx >= 0) panes.value.splice(idx, 1)
    }
    ElMessage.error(`${t('ftp.connectFail')}：${hostLabel(host)} — ${e?.message || e}`)
  } finally {
    pane.connecting = false
  }
}

/** 关闭窗格：回收远端会话；全部关完则回到默认本地窗格 */
function closePane(paneId: string): void {
  const idx = panes.value.findIndex(p => p.id === paneId)
  if (idx < 0) return
  const [pane] = panes.value.splice(idx, 1)
  if (pane.kind === 'ssh' && pane.sessionId) {
    invoke('close_session', { sessionId: pane.sessionId }).catch(() => {})
  }
  if (panes.value.length === 0) panes.value.push(makeLocalPane())
  // 分屏下关闭到仅剩一个窗格时，自动退出分屏模式（收起为单窗格）
  if (panes.value.length <= 1 && layoutMode.value !== 'auto') layoutMode.value = 'auto'
}

// 组件销毁（keep-alive 被逐出 / 应用关闭）时回收所有远端会话
onBeforeUnmount(() => {
  for (const p of panes.value) {
    if (p.kind === 'ssh' && p.sessionId) {
      invoke('close_session', { sessionId: p.sessionId }).catch(() => {})
    }
  }
})

// ===== 布局：自动（≤3 左右 / ≥4 网格）或顶栏手动指定 左右 / 上下 / 网格 =====
const layoutRef = ref<HTMLElement | null>(null)

/** 布局模式：auto 跟随窗格数量，h 左右 / v 上下 / grid 网格（与终端分屏按钮一致） */
type LayoutMode = 'auto' | 'h' | 'v' | 'grid'
const layoutMode = ref<LayoutMode>('auto')

/** 实际生效的铺排方式 */
const effMode = computed<'row' | 'col' | 'grid'>(() => {
  if (layoutMode.value === 'h') return 'row'
  if (layoutMode.value === 'v') return 'col'
  if (layoutMode.value === 'grid') return 'grid'
  return panes.value.length <= 3 ? 'row' : 'grid'
})

/** 容器类名（flex 行 / flex 列 / grid） */
const containerClass = computed(() => ({
  'is-row': effMode.value === 'row',
  'is-col': effMode.value === 'col',
  'is-grid': effMode.value === 'grid',
}))

// ===== 分屏按钮（与终端 TabBar 同一套线性图标） =====
const svgAttrs = {
  viewBox: '0 0 14 14', width: 14, height: 14,
  fill: 'none', stroke: 'currentColor',
  'stroke-width': '1.2', 'stroke-linecap': 'round', 'stroke-linejoin': 'round'
} as const
/** 左右分屏 */
const IconSplitH = defineComponent({ render: () => h('svg', svgAttrs, [
  h('rect', { x: 1, y: 2, width: 12, height: 10, rx: 1 }),
  h('line', { x1: 7, y1: 2, x2: 7, y2: 12 })
]) })
/** 上下分屏 */
const IconSplitV = defineComponent({ render: () => h('svg', svgAttrs, [
  h('rect', { x: 1, y: 2, width: 12, height: 10, rx: 1 }),
  h('line', { x1: 1, y1: 7, x2: 13, y2: 7 })
]) })
/** 网格分屏 */
const IconSplitGrid = defineComponent({ render: () => h('svg', svgAttrs, [
  h('rect', { x: 1, y: 2, width: 12, height: 10, rx: 1 }),
  h('line', { x1: 7, y1: 2, x2: 7, y2: 12 }),
  h('line', { x1: 1, y1: 7, x2: 13, y2: 7 })
]) })
const splitButtons = [
  { mode: 'h' as const, icon: IconSplitH, tooltip: '左右分屏（再次点击恢复自动）' },
  { mode: 'v' as const, icon: IconSplitV, tooltip: '上下分屏（再次点击恢复自动）' },
  { mode: 'grid' as const, icon: IconSplitGrid, tooltip: '网格分屏（再次点击恢复自动）' },
]
function onSplitClick(mode: 'h' | 'v' | 'grid'): void {
  // 再次点击已激活的按钮 → 关闭分屏模式，收起为单个窗格
  if (layoutMode.value === mode) { collapseToSingle(); return }
  // 分屏要求的最小窗格数：左右/上下 ≥2，网格 ≥4；不足时用本地窗格补齐（与终端模式一致）
  const required = mode === 'grid' ? 4 : 2
  while (panes.value.length < required && panes.value.length < MAX_PANES) {
    panes.value.push(makeLocalPane())
  }
  layoutMode.value = mode
}

/** 关闭分屏模式：保留第一个窗格（回收其余窗格的远端会话），回到单窗格视图 */
function collapseToSingle(): void {
  const firstId = panes.value[0]?.id
  for (const p of [...panes.value]) {
    if (p.id !== firstId) closePane(p.id)
  }
  layoutMode.value = 'auto'
}

/** 行模式：每窗格一个 flex-grow 权重 */
const flexWeights = ref<number[]>([])
/** 网格模式：列 / 行权重 */
const colWeights = ref<number[]>([])
const rowWeights = ref<number[]>([])

const gridCols = computed(() => (panes.value.length <= 4 ? 2 : 3))
const gridRows = computed(() => Math.ceil(Math.max(panes.value.length, 1) / gridCols.value))
const gridRowGapCount = computed(() => Math.max(gridRows.value - 1, 0))

/** 网格竖向分界线槽位（按行生成；末行不满时由最后一个窗格横跨补齐，不留空格） */
const gridColSlots = computed(() => {
  const cols = gridCols.value
  const n = panes.value.length
  const slots: { key: string; row: number; gap: number }[] = []
  for (let r = 0; r < gridRows.value; r++) {
    const items = Math.min(cols, n - r * cols)
    for (let g = 0; g < items - 1; g++) slots.push({ key: `gc-${r}-${g}`, row: r, gap: g })
  }
  return slots
})

/** 窗格数量变化（进入/退出网格）时重置为等分 */
watch(() => panes.value.length, () => {
  flexWeights.value = Array.from({ length: panes.value.length }, () => 1)
  colWeights.value = Array.from({ length: gridCols.value }, () => 1)
  rowWeights.value = Array.from({ length: gridRows.value }, () => 1)
}, { immediate: true })

/** 窗格内联尺寸：行/列模式用 flex-grow；网格用显式行列定位（奇数轨放窗格，偶数轨留给分界线） */
function paneStyle(i: number) {
  // 行/列模式：basis 设为 0%，让尺寸完全按 flexGrow 权重比例分配，
  // 避免内部文件系统内容的最小宽度把自由空间占满、导致拖拽权重变化看不出效果
  if (effMode.value !== 'grid') return { flexGrow: String(flexWeights.value[i] ?? 1), flexBasis: '0%' }
  const cols = gridCols.value
  const n = panes.value.length
  const c = i % cols
  const r = Math.floor(i / cols)
  const colStart = c * 2 + 1
  // 末行不满时，最后一个窗格横跨剩余列，铺满腾出的空间
  const itemsInRow = Math.min(cols, n - r * cols)
  const colEnd = i === n - 1 && itemsInRow < cols ? cols * 2 : colStart + 1
  return { gridColumn: `${colStart} / ${colEnd}`, gridRow: String(r * 2 + 1) }
}

/** 把权重数组拼成 CSS 轨道串：fr 轨道与 5px 分界线交替 */
function buildTracks(w: number[], n: number): string {
  const parts: string[] = []
  for (let i = 0; i < n; i++) {
    if (i > 0) parts.push('5px')
    // minmax(0, Nfr)：允许轨道缩到内容以下，拖拽调权重时尺寸才真正变化
    parts.push(`minmax(0, ${w[i] ?? 1}fr)`)
  }
  return parts.join(' ')
}

const layoutStyle = computed(() => {
  if (effMode.value !== 'grid') return undefined
  return {
    gridTemplateColumns: buildTracks(colWeights.value, gridCols.value),
    gridTemplateRows: buildTracks(rowWeights.value, gridRows.value),
  }
})

/** 单个窗格的最小尺寸，避免被拖到不可用 */
const MIN_PANE_PX = 120

/**
 * 在相邻两项之间转移权重：delta 为鼠标位移（px），totalPx 为容器该方向总尺寸。
 * 按比例换算后夹紧，保证两侧都不小于 MIN_PANE_PX。
 */
function shiftWeights(arr: number[], i: number, delta: number, totalPx: number): void {
  const a = arr[i]
  const b = arr[i + 1]
  if (a === undefined || b === undefined || totalPx <= 0) return
  const sum = a + b
  const wTotal = arr.reduce((s, v) => s + v, 0)
  if (wTotal <= 0) return
  const minWeight = (MIN_PANE_PX / totalPx) * wTotal
  if (sum <= minWeight * 2) return
  const na = Math.min(Math.max(a + (delta / totalPx) * wTotal, minWeight), sum - minWeight)
  if (na === a) return
  arr[i] = na
  arr[i + 1] = sum - na
}

function onFlexDrag(i: number, delta: number): void {
  const el = layoutRef.value
  if (!el) return
  shiftWeights(flexWeights.value, i, delta, el.clientWidth)
}
function onGridColDrag(gapIndex: number, delta: number): void {
  const el = layoutRef.value
  if (!el) return
  shiftWeights(colWeights.value, gapIndex, delta, el.clientWidth)
}
function onGridRowDrag(gapIndex: number, delta: number): void {
  const el = layoutRef.value
  if (!el) return
  shiftWeights(rowWeights.value, gapIndex, delta, el.clientHeight)
}
</script>

<style scoped>
.ftp-page {
  position: relative;
  height: 100%;
  display: flex;
  flex-direction: column;
  background: var(--jc-bg);
  overflow: hidden;
}

/* ---- 顶栏 ---- */
.ftp-head {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 34px;
  padding: 0 12px;
  border-bottom: 1px solid var(--jc-border);
  background: var(--jc-bg-soft);
  flex-shrink: 0;
}
.ftp-head-ico { color: var(--jc-text-soft); }
.ftp-title { font-size: 13px; font-weight: 600; color: var(--jc-text); }
.ftp-add {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  margin-left: 2px;
  border: 1px solid var(--jc-border);
  border-radius: 5px;
  background: transparent;
  color: var(--jc-text-soft);
  cursor: pointer;
  transition: background-color 0.12s ease, color 0.12s ease, border-color 0.12s ease;
}
.ftp-add:hover:not(:disabled) {
  color: var(--jc-accent);
  border-color: var(--jc-accent);
  background: rgba(59, 130, 246, 0.08);
}
.ftp-add:disabled, .ftp-add.is-max { color: var(--jc-text-muted); opacity: 0.45; cursor: not-allowed; }

/* 分屏布局按钮（与终端 TabBar 同款交互）：点击已激活的按钮恢复自动布局 */
.ftp-split {
  margin-left: auto;
  display: flex;
  align-items: center;
  gap: 2px;
}
.ftp-split-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 22px;
  padding: 0;
  border: 0;
  border-radius: 5px;
  background: transparent;
  color: var(--jc-text-soft);
  cursor: pointer;
  transition: background-color 0.12s ease, color 0.12s ease;
}
.ftp-split-btn:hover { color: var(--jc-text); background: var(--jc-bg-hover); }
.ftp-split-btn.is-active { color: var(--jc-accent); background: rgba(59, 130, 246, 0.15); }

/* ---- 窗格区 ---- */
.ftp-panes {
  flex: 1;
  min-height: 0;
  background: var(--jc-split-line);
}
.ftp-panes.is-row { display: flex; flex-direction: row; }
.ftp-panes.is-col { display: flex; flex-direction: column; }
.ftp-panes.is-grid { display: grid; }

.ftp-pane {
  /* 作为窗格内抽屉（.pane-picker-layer）的定位参照，保证抽屉从本分屏右侧滑出 */
  position: relative;
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  background: var(--jc-bg);
  overflow: hidden;
}

/* 窗格头：会话按钮（悬停时圆点变 ×，点 × 关闭窗格） */
.pane-head {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 30px;
  padding: 0 6px 0 8px;
  border-bottom: 1px solid var(--jc-border);
  background: var(--jc-bg-soft);
  flex-shrink: 0;
}
/* 文件系统工具条容器：占据剩余宽度并右对齐（AdaptiveToolbar 按父容器宽度判定收纳，
   父容器必须有真实宽度，不能随内容收缩，否则会全部收进溢出箭头）。
   注意不能加 overflow:hidden：会把溢出箭头的悬浮工具栏一起裁掉 */
.pane-fs-toolbar {
  flex: 1 1 0;
  min-width: 0;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: flex-end;
}
.pane-session {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  height: 22px;
  padding: 0 8px 0 6px;
  border: 1px solid var(--jc-border);
  border-radius: 6px;
  background: var(--jc-bg);
  color: var(--jc-text);
  font-size: 12px;
  cursor: pointer;
  transition: border-color 0.12s ease, color 0.12s ease, background-color 0.12s ease;
}
.pane-session:hover {
  color: var(--jc-accent);
  border-color: var(--jc-accent);
  background: rgba(59, 130, 246, 0.08);
}
/* 状态点 / 关闭按钮切换容器：固定尺寸，悬停会话按钮时圆点淡出、红色 × 淡入 */
.ps-swap {
  position: relative;
  width: 16px;
  height: 16px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}
/* 关闭按钮：红色圆形背景，醒目，点击关闭所属窗格（默认隐藏，悬停时替代状态点） */
.ps-close {
  position: absolute;
  inset: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 50%;
  background: #ef4444;
  color: #fff;
  opacity: 0;
  pointer-events: none;
  transition: background-color 0.12s ease, opacity 0.12s ease;
}
.ps-close:hover { background: #dc2626; }
.pane-session:hover .ps-close { opacity: 1; pointer-events: auto; }
.ps-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  transition: opacity 0.12s ease;
}
.ps-dot.ok { background: #34d399; }
.ps-dot.off { background: var(--jc-text-muted); }
.ps-dot.busy { background: #fbbf24; animation: ps-pulse 1s ease-in-out infinite; }
.pane-session:hover .ps-dot { opacity: 0; }
@keyframes ps-pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.35; }
}
.ps-label {
  max-width: 240px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.ps-label.is-muted { color: var(--jc-text-muted); }

.pane-body {
  position: relative;
  flex: 1;
  min-height: 0;
}

/* ---- 会话选择抽屉（右侧滑出） ---- */
.picker-layer {
  position: absolute;
  inset: 0;
  background: rgba(0, 0, 0, 0.35);
  z-index: 40;
}
.picker-panel {
  position: absolute;
  top: 0;
  right: 0;
  bottom: 0;
  width: 280px;
  background: var(--jc-bg-soft);
  border-left: 1px solid var(--jc-border);
  box-shadow: -8px 0 24px rgba(0, 0, 0, 0.35);
}
.ftp-drawer-enter-active, .ftp-drawer-leave-active { transition: opacity 0.15s ease; }
.ftp-drawer-enter-active .picker-panel, .ftp-drawer-leave-active .picker-panel {
  transition: transform 0.2s ease;
}
.ftp-drawer-enter-from, .ftp-drawer-leave-to { opacity: 0; }
.ftp-drawer-enter-from .picker-panel, .ftp-drawer-leave-to .picker-panel {
  transform: translateX(100%);
}

/* ---- 窗格专属抽屉：锚定到所属分屏，从分屏右侧滑出（仅覆盖该窗格） ---- */
.pane-picker-layer {
  position: absolute;
  inset: 0;
  z-index: 30;
  background: rgba(0, 0, 0, 0.3);
}
.pane-picker-panel {
  position: absolute;
  top: 0;
  right: 0;
  bottom: 0;
  width: min(280px, 92%);
  background: var(--jc-bg-soft);
  border-left: 1px solid var(--jc-border);
  box-shadow: -8px 0 24px rgba(0, 0, 0, 0.35);
}
/* 复用同一套滑入动画（.pane-picker-panel 与 .picker-panel 视觉一致） */
.ftp-drawer-enter-active .pane-picker-panel, .ftp-drawer-leave-active .pane-picker-panel {
  transition: transform 0.2s ease;
}
.ftp-drawer-enter-from .pane-picker-panel, .ftp-drawer-leave-to .pane-picker-panel {
  transform: translateX(100%);
}
</style>
