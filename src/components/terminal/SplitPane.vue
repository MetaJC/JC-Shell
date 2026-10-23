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
  <div
    class="split-pane flex min-w-0 min-h-0 bg-[var(--jc-bg)]"
    :class="[layoutClass, { 'split-pane--active': active && splitMode !== 'none' }]"
    @mousedown.capture="emit('activate')"
  >
    <!-- 终端区（应用内文件拖入 → 会话当前目录） -->
    <div
      ref="termAreaRef"
      class="relative flex-1 min-w-0 min-h-0"
      :class="{ 'ring-1 ring-inset ring-blue-500/60': dropHover }"
      @dragover="onTermDragOver"
      @dragleave="dropHover = false"
      @drop="onTermDrop"
    >
      <TerminalView
        :ref="el => termRef = (el as InstanceType<typeof TerminalView>) || null"
        :tab-id="tab.id"
        :session-id="tab.sessionId || ''"
        :kind="tab.kind"
        :connected="tab.connected"
        :agent-mode="agentStore.isEnabled(tab.id)"
        class="absolute inset-0"
        @reconnect="emit('reconnect')"
        @agent-submit="(prompt: string) => emit('agent-submit', prompt)"
        @agent-action="(action: AgentAction) => emit('agent-action', action)"
        @shell-command="emit('shell-command')"
        @term-context-menu="onTermContextMenu"
      />

      <!-- 分屏模式底部操作提示：进入分屏时弹出，10 秒后自动消失；可单个永久关闭 -->
      <div v-if="visibleTips.length" class="split-tips">
        <div v-for="tip in visibleTips" :key="tip.key" class="split-tip">
          <span>{{ tip.text }}</span>
          <button class="split-tip-dismiss" @click.stop="dismissSplitTip(tip.key)">不再显示</button>
        </div>
      </div>
    </div>

    <!-- 终端与文件系统之间的可拖拽分隔条（调整 fs 高度/宽度） -->
    <SplitBar
      v-if="splitMode !== 'none' && !fsCollapsed"
      :orientation="splitMode === 'v' ? 'vertical' : 'horizontal'"
      @drag="onFsResize"
    />

    <!-- 独立文件系统面板（绑定本单元会话；h/grid 在下方，v 在右侧） -->
    <div ref="fsRef" v-if="splitMode !== 'none'" class="shrink-0 overflow-hidden" :style="fsStyle">
      <!-- 折叠态：展开条（保留展开按钮可随时恢复；h/grid 横向、v 竖向） -->
      <button
        v-if="fsCollapsed"
        class="w-full h-full flex items-center justify-center bg-[var(--jc-bg-soft)] hover:bg-[var(--jc-bg-hover)] text-[var(--jc-text-muted)]"
        :class="splitMode === 'v' ? 'flex-col' : 'flex-row'"
        title="展开文件系统"
        @click="onFsCollapse(false)"
      >
        <el-icon size="12" :class="splitMode === 'v' ? 'rotate-90' : ''"><Expand /></el-icon>
      </button>
      <template v-else>
        <div class="w-full h-full flex flex-col" title="Alt 键切换工具条显示">
          <!-- 内容区：分屏空间有限，哪个方向展示不全就在哪个方向滚动 -->
          <div v-show="!fsCollapsed" class="flex-1 min-h-0 overflow-auto">
            <FileSystemBrowser
              ref="fsBrowserRef"
              class="min-w-[320px] min-h-[140px]"
              :connected="fsConnected"
              :hostname="''"
              :session-id="tab.sessionId"
              :kind="tab.kind"
              dock="bottom"
              :grip="false"
              :hide-header="!splitFsToolbar.visible"
              @collapse="onFsCollapse(true)"
              @fs-scroll="onFsScroll"
              @fs-toolbar="onFsToolbar"
            />
          </div>
        </div>
      </template>
    </div>
  </div>
</template>

<script lang="ts">
/**
 * 分屏内文件系统工具条显隐状态（窗口内所有分屏共享）：默认隐藏，按 Alt 切换。
 * 作者：JC
 * 多个 SplitPane 实例共用一个 Alt 监听，用引用计数避免重复/丢失。
 */
import { reactive } from 'vue'
import { useSettingsStore } from '@/stores/settings'

export const splitFsToolbar = reactive({ visible: true })

// ===== 分屏底部操作提示的「不再显示」持久化 =====
/** localStorage 键名 */
const SPLIT_TIP_KEY = 'jc_shell_split_tip_dismissed'

/** 读取当前版本中已被用户永久关闭的提示 key */
function loadSplitTipDismissed(): string[] {
  try {
    const raw = localStorage.getItem(SPLIT_TIP_KEY)
    if (!raw) return []
    const parsed = JSON.parse(raw)
    // 记录里带版本号：升级到新版本后视为全新记录，重新提示
    if (parsed?.version !== __APP_VERSION__) return []
    return Array.isArray(parsed.keys) ? parsed.keys.filter((k: unknown) => typeof k === 'string') : []
  } catch { return [] }
}

/** 窗口内所有分屏实例共享：当前版本中不再显示的提示 key */
export const splitTipDismissed = reactive<{ keys: string[] }>({ keys: loadSplitTipDismissed() })

/** 写入「不再显示」记录（带当前版本号） */
export function persistSplitTipDismissed(): void {
  try {
    localStorage.setItem(SPLIT_TIP_KEY, JSON.stringify({ version: __APP_VERSION__, keys: splitTipDismissed.keys }))
  } catch { /* 忽略写入失败 */ }
}

/** 连按两次 Alt 的判定窗口（与人手双击节奏匹配，放宽到 700ms） */
const DOUBLE_ALT_MS = 700
let altToggleRefs = 0
let lastAltDownAt = 0
function onAltToggleKey(e: KeyboardEvent) {
  // 按住 Alt 会持续触发 repeat，不忽略会让工具条高频闪烁
  if (e.key !== 'Alt' || e.repeat) return
  e.preventDefault()
  if (useSettingsStore().settings.fsToolbarToggle === 'doubleAlt') {
    const now = Date.now()
    if (lastAltDownAt > 0 && now - lastAltDownAt < DOUBLE_ALT_MS) {
      splitFsToolbar.visible = !splitFsToolbar.visible
      lastAltDownAt = 0
    } else {
      lastAltDownAt = now
    }
    return
  }
  splitFsToolbar.visible = !splitFsToolbar.visible
}
export function attachFsToolbarAltToggle() {
  if (altToggleRefs++ === 0) window.addEventListener('keydown', onAltToggleKey)
}
export function detachFsToolbarAltToggle() {
  if (--altToggleRefs === 0) window.removeEventListener('keydown', onAltToggleKey)
}
</script>

<script setup lang="ts">
/**
 * 分屏单元
 * 作者：JC
 * 独立终端 + 独立文件系统（绑定该单元会话）：
 * - h/grid 模式（左右分屏）：单元内终端上、fs 下（垂直布局）
 * - v 模式（上下分屏）：单元内终端左、fs 右（水平布局）
 * - fs 工具条默认隐藏（Alt 切换）；折叠按钮在工具条行最右（与非分屏一致）
 * - 内容区双向滚动：哪个方向展示不全就在哪个方向滚
 * - 终端区为文件放置目标：放入 → 会话当前目录
 */
import { ref, computed, watch, onMounted, onBeforeUnmount, nextTick } from 'vue'
import { Expand } from '@element-plus/icons-vue'
import { ElMessage } from 'element-plus'
import { handleApiError } from '@/utils/errorHandler'
import TerminalView from './TerminalView.vue'
import SplitBar from '@/components/common/SplitBar.vue'
import FileSystemBrowser from '@/components/filesystem/FileSystemBrowser.vue'
import type { TerminalTab, SplitMode } from '@/stores/terminal'
import { useAgentStore, type AgentAction } from '@/stores/agent'
import {
  isRemoteFileDrag, getRemoteFilePayload, transferRemoteFile, getShellCwd,
  registerOsDropZone, unregisterOsDropZone
} from '@/composables/useFileDrag'
import { registerTermArea, unregisterTermArea, attachTermAreaSync, emitSyncFs, type FsSyncPayload, type FsToolbarSync } from '@/composables/useTermSync'

const props = defineProps<{
  tab: TerminalTab
  splitMode: SplitMode
  /** 是否为当前激活 Tab（单 Tab 模式下控制显隐由父级 v-show 处理） */
  active: boolean
  /** 是否显示分屏底部操作提示；由父级只给第一个分屏窗口开启，避免多个窗口重复弹同样的提示 */
  tipEnabled?: boolean
}>()

const emit = defineEmits<{
  (e: 'activate'): void
  (e: 'reconnect'): void
  /** Agent 模式下回车提交的自然语言指令 */
  (e: 'agent-submit', prompt: string): void
  /** Agent 终端内确认/控制动作 */
  (e: 'agent-action', action: AgentAction): void
  /** Agent 输入行被当成命令直接执行了（工作区需刷新仿真 prompt） */
  (e: 'shell-command'): void
  /** 终端空白区右键（无选区）：把坐标 + 本单元 tabId 抛给工作区弹菜单 */
  (e: 'termContextMenu', payload: { x: number; y: number; tabId: string }): void
}>()

/**
 * 终端空白区右键：补上本单元 tabId 后抛给工作区。
 * 写成函数而不是模板内联箭头，是因为 vue-tsc 解析模板内联的「对象类型标注」会报 TS1005。
 */
function onTermContextMenu(p: { x: number; y: number }): void {
  emit('termContextMenu', { ...p, tabId: props.tab.id })
}

const agentStore = useAgentStore()

const termRef = ref<InstanceType<typeof TerminalView> | null>(null)
const termAreaRef = ref<HTMLElement | null>(null)
const fsRef = ref<HTMLElement | null>(null)
const fsBrowserRef = ref<InstanceType<typeof FileSystemBrowser> | null>(null)
/** 文件系统折叠/尺寸：本地状态（各分屏独立）；Ctrl 批量模式开启时经 emitSyncFs 广播给其他分屏 */
const fsCollapsed = ref(false)
const fsHeightRatio = ref(0.38)
const fsWidthRatio = ref(0.42)

/** 批量模式生效时把当前 fs 状态广播到其它分屏 */
function syncFsState() {
  if (!props.tab.sessionId) return
  emitSyncFs({
    sessionId: props.tab.sessionId,
    collapsed: fsCollapsed.value,
    heightRatio: fsHeightRatio.value,
    widthRatio: fsWidthRatio.value,
  })
}

/** 折叠/展开：更新本地并广播 */
function onFsCollapse(v: boolean) {
  fsCollapsed.value = v
  syncFsState()
}

/** 接收其它分屏在批量模式下同步过来的 fs 状态 */
function applyFsSync(data: FsSyncPayload) {
  fsCollapsed.value = data.collapsed
  fsHeightRatio.value = data.heightRatio
  fsWidthRatio.value = data.widthRatio
  fsBrowserRef.value?.applyRemoteFs(data)
}
/** 源分屏滚动：节流后广播滚动比例（仅批量模式生效） */
let lastScrollEmit = 0
function onFsScroll(sx: number, sy: number) {
  if (!props.tab.sessionId) return
  const now = Date.now()
  if (now - lastScrollEmit < 60) return
  lastScrollEmit = now
  emitSyncFs({
    sessionId: props.tab.sessionId,
    collapsed: fsCollapsed.value,
    heightRatio: fsHeightRatio.value,
    widthRatio: fsWidthRatio.value,
    scrollX: sx,
    scrollY: sy,
  })
}

/** 源分屏工具栏按钮：广播按钮 key 与相关状态快照（仅批量模式生效） */
function onFsToolbar(p: FsToolbarSync) {
  if (!props.tab.sessionId) return
  emitSyncFs({
    sessionId: props.tab.sessionId,
    collapsed: fsCollapsed.value,
    heightRatio: fsHeightRatio.value,
    widthRatio: fsWidthRatio.value,
    viewMode: p.viewMode,
    syncEnabled: p.syncEnabled,
    autoRefreshEnabled: p.autoRefreshEnabled,
    toolbarKey: p.key,
  })
}

const dropHover = ref(false)

// ===== 分屏底部操作提示 =====
/** 提示停留时长（毫秒） */
const SPLIT_TIP_DURATION = 10_000
/** 进入分屏后向用户说明的操作捷径 */
const SPLIT_TIPS = [
  { key: 'ctrlBatch', text: '连按两次 Ctrl 进入批量操作模式' },
  { key: 'altToolbar', text: '按 Alt 切换显示/隐藏工具栏' }
] as const

/** 当前正在展示的提示 key（每个分屏实例各自计时） */
const shownTips = ref<string[]>([])
let splitTipTimer: ReturnType<typeof setTimeout> | null = null

function stopSplitTipTimer(): void {
  if (splitTipTimer) {
    clearTimeout(splitTipTimer)
    splitTipTimer = null
  }
}

/** 进入分屏模式：弹出尚未被永久关闭的提示，10 秒后自动收起（只在第一个分屏窗口显示） */
function showSplitTips(): void {
  stopSplitTipTimer()
  // 只有第一个分屏窗口弹提示，其余窗口保持干净
  if (!props.tipEnabled) {
    shownTips.value = []
    return
  }
  shownTips.value = SPLIT_TIPS.filter(t => !splitTipDismissed.keys.includes(t.key)).map(t => t.key)
  if (!shownTips.value.length) return
  splitTipTimer = setTimeout(() => {
    splitTipTimer = null
    shownTips.value = []
  }, SPLIT_TIP_DURATION)
}

/** 点击「不再显示」：立即收起该条，当前版本内所有分屏都不再弹出 */
function dismissSplitTip(key: string): void {
  shownTips.value = shownTips.value.filter(k => k !== key)
  if (!splitTipDismissed.keys.includes(key)) splitTipDismissed.keys.push(key)
  persistSplitTipDismissed()
}

const visibleTips = computed(() => SPLIT_TIPS.filter(t => shownTips.value.includes(t.key)))

// 每次进入分屏模式都提示一次；退出分屏立即收起
watch(() => props.splitMode, (mode) => {
  if (mode === 'none') {
    stopSplitTipTimer()
    shownTips.value = []
  } else {
    showSplitTips()
  }
  // 分屏布局切换会改变本单元终端区尺寸：等 DOM 更新后统一 re-fit，
  // 避免容器在重排中间态为 0 尺寸时终端不刷新（表现为提示符丢失 / 无法输入）
  nextTick(() => termRef.value?.resize?.())
}, { immediate: true })

/** 分屏布局方向：v 模式单元内终端与 fs 左右排布，其余上下 */
const layoutClass = computed(() => props.splitMode === 'v' ? 'flex-row' : 'flex-col')
/** fs 容器尺寸：折叠时仅保留头部/展开条；展开时按可拖拽比例 */
const fsStyle = computed(() => {
  if (props.splitMode === 'v') {
    return {
      width: fsCollapsed.value ? '24px' : (fsWidthRatio.value * 100) + '%',
      height: '100%',
      borderLeft: '1px solid var(--jc-border)'
    }
  }
  return {
    height: fsCollapsed.value ? '24px' : (fsHeightRatio.value * 100) + '%',
    width: '100%',
    borderTop: '1px solid var(--jc-border)'
  }
})

/** 拖拽分隔条：fs 在分隔条下方/右侧，向终端方向拖（上/左）fs 变大，故取 -delta */
function onFsResize(delta: number) {
  const paneEl = fsRef.value?.parentElement
  const fsEl = fsRef.value
  if (!paneEl || !fsEl) return
  if (props.splitMode === 'v') {
    const total = paneEl.clientWidth
    const size = Math.min(Math.max(fsEl.offsetWidth - delta, 80), total - 120)
    fsWidthRatio.value = size / total
  } else {
    const total = paneEl.clientHeight
    const size = Math.min(Math.max(fsEl.offsetHeight - delta, 60), total - 100)
    fsHeightRatio.value = size / total
  }
  syncFsState()
}

/** fs 数据可用判定：本地会话走 list_local_dir，SSH 会话需已连接 */
const fsConnected = computed(() => props.tab.connected && !!props.tab.sessionId)

/** Ctrl 批量模式的终端区事件复制注销函数 */
let detachTermAreaSync: (() => void) | null = null

// ===== 终端区：应用内文件拖入 → 会话当前目录 =====
function onTermDragOver(e: DragEvent) {
  if (!isRemoteFileDrag(e)) return
  e.preventDefault()
  if (e.dataTransfer) e.dataTransfer.dropEffect = 'move'
  dropHover.value = true
}

async function onTermDrop(e: DragEvent) {
  dropHover.value = false
  const payload = getRemoteFilePayload(e)
  if (!payload) return
  e.preventDefault()
  if (!props.tab.sessionId) {
    ElMessage.warning('该终端尚未建立会话，无法接收文件')
    return
  }
  try {
    const cwd = await getShellCwd(props.tab.sessionId, props.tab.kind === 'local')
    const dst = await transferRemoteFile(payload, props.tab.sessionId, cwd, props.tab.kind)
    ElMessage.success(`已移动到 ${dst}`)
  } catch (err: any) {
    handleApiError(err, 'components/SplitPane.onTermDrop', { prefix: '移动失败' })
  }
}

// ===== OS 文件拖入放置区注册（drag-drop 事件按坐标命中） =====
watch(
  () => [props.tab.sessionId, props.tab.kind] as const,
  () => syncZone(),
  { immediate: false }
)

// Tab 激活（v-show 从隐藏切回可见）时：重新 fit 并整屏重绘。
// 此前仅调 resize()，但隐藏期间 resize() 被「0 尺寸跳过」挡掉、WebGL 渲染层停留在旧尺寸黑屏，
// 空闲会话要等下一条输出才刷新 —— 表现为「关掉其它 Tab 后剩余 Tab 一片黑」。onShow 内含 refresh 整屏。
watch(() => props.active, (active) => {
  if (active) {
    nextTick(() => termRef.value?.onShow?.())
  }
})

function syncZone() {
  unregisterOsDropZone(props.tab.id)
  if (!termAreaRef.value || !props.tab.sessionId) return
  registerOsDropZone({
    id: props.tab.id,
    el: termAreaRef.value,
    sessionId: props.tab.sessionId,
    local: props.tab.kind === 'local'
  })
}

onMounted(() => {
  syncZone()
  attachFsToolbarAltToggle()
  // Ctrl 批量模式：终端区鼠标事件复制到其他分屏
  if (termAreaRef.value) {
    registerTermArea(props.tab.id, termAreaRef.value)
    detachTermAreaSync = attachTermAreaSync(props.tab.id, termAreaRef.value)
  }
})
onBeforeUnmount(() => {
  unregisterOsDropZone(props.tab.id)
  unregisterTermArea(props.tab.id)
  detachTermAreaSync?.()
  detachFsToolbarAltToggle()
  stopSplitTipTimer()
})

defineExpose({
  /** 终端视图实例（供工作区路由输出/序列化） */
  term: termRef,
  /** Ctrl 批量模式下接收其它分屏同步过来的 fs 折叠/尺寸状态 */
  applyFsSync
})
</script>

<style scoped>
.split-pane--active {
  outline: 1px solid rgba(59, 130, 246, 0.35);
  outline-offset: -1px;
}

/* ===== 分屏底部操作提示 ===== */
.split-tips {
  position: absolute;
  left: 12px;
  bottom: 12px;
  z-index: 20;
  display: flex;
  flex-direction: column;
  gap: 6px;
  /* 容器不拦截终端交互，仅提示条本身可点击 */
  pointer-events: none;
}
.split-tip {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 6px 10px;
  border-radius: 6px;
  border: 1px solid var(--jc-border);
  background: color-mix(in srgb, var(--jc-bg-panel) 94%, transparent);
  color: var(--jc-text);
  font-size: 12px;
  line-height: 1.4;
  white-space: nowrap;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.25);
  pointer-events: auto;
  animation: split-tip-in 0.2s ease;
}
.split-tip-dismiss {
  flex-shrink: 0;
  padding: 2px 6px;
  border: 1px solid var(--jc-border);
  border-radius: 4px;
  background: transparent;
  color: var(--jc-text-soft);
  font-size: 11px;
  cursor: pointer;
  transition: color 0.15s, border-color 0.15s;
}
.split-tip-dismiss:hover {
  color: var(--jc-accent);
  border-color: var(--jc-accent);
}
@keyframes split-tip-in {
  from { opacity: 0; transform: translateY(6px); }
  to { opacity: 1; transform: translateY(0); }
}
</style>
