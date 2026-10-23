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
  <!-- isolate：给终端工作区建独立层叠上下文，把内部所有浮层（状态条/快捷按钮 30、
       agent ghost 100、悬停指示环 999 等）都关在这一层里，保证它们永远盖不到
       左侧 Dock 的弹出菜单（见 SideDock.vue 的 z-index 说明） -->
  <div class="terminal-workspace relative isolate h-full flex flex-col min-h-0">
    <!-- Ctrl 批量模式指示：按住临时生效 / 双击 Ctrl 常驻 -->
    <div
      v-if="termSync.active"
      class="absolute top-10 right-3 z-20 pointer-events-none px-2.5 py-1 rounded bg-blue-500/20 border border-blue-500/50 text-blue-300 text-xs"
    >
      {{ syncBadgeText }}：操作同步到所有分屏
    </div>
    <TerminalTabBar
      v-if="!zen"
      :tabs="termStore.tabs"
      :active-tab-id="termStore.activeTabId"
      :split-mode="termStore.splitMode"
      @select="termStore.setActive"
      @close="onTabRemove"
      @add="openLocalTerminal"
      @reorder="onReorder"
      @detach="onTabDetach"
      @split-mode="onSplitMode"
      @tab-menu-action="onTabMenuAction"
    />

    <!-- 终端区：单 Tab 视图 / 左右 / 上下 / 网格 分屏铺排（分屏时容器底色作为分界线透出） -->
    <div ref="layoutRef" class="flex-1 min-h-0" :class="containerClass" :style="containerStyle">
      <template v-for="(tab, i) in termStore.tabs" :key="tab.id">
        <SplitPane
          v-show="termStore.splitMode !== 'none' || termStore.activeTabId === tab.id"
          :ref="el => setPaneRef(tab.id, el as InstanceType<typeof SplitPane> | null)"
          :tab="tab"
          :split-mode="termStore.splitMode"
          :active="termStore.activeTabId === tab.id"
          :tip-enabled="tab.id === termStore.tabs[0]?.id"
          :class="paneClass"
          :style="paneStyle(i)"
          @activate="termStore.setActive(tab.id)"
          @reconnect="reconnectTab(tab.id)"
          @agent-submit="(prompt: string) => onAgentSubmit(tab.id, prompt)"
          @agent-action="(action: AgentAction) => onAgentAction(tab.id, action)"
          @shell-command="refreshAgentPrompt(tab.id)"
          @term-context-menu="onTermContextMenu"
        />
        <!-- 左右/上下分屏：相邻面板之间的可拖拽分界线 -->
        <SplitBar
          v-if="(termStore.splitMode === 'h' || termStore.splitMode === 'v') && i < termStore.tabs.length - 1"
          :orientation="termStore.splitMode === 'h' ? 'vertical' : 'horizontal'"
          @drag="delta => onDividerDrag(i, delta)"
        />
      </template>
      <!-- 网格分屏：列 / 行之间的可拖拽分界线 -->
      <template v-if="termStore.splitMode === 'grid'">
        <!-- 竖向分界线按「行」生成：末行面板少一颗时由最后一个面板横跨补齐，不再留空 -->
        <SplitBar
          v-for="d in gridColDividerSlots"
          :key="d.key"
          orientation="vertical"
          :style="{ gridColumn: String(d.gap * 2), gridRow: String(d.row * 2 + 1) }"
          @drag="delta => onGridColDrag(d.gap, delta)"
        />
        <!-- 横向分界线贯通整行 -->
        <SplitBar
          v-for="r in gridRowGapCount"
          :key="'grid-row-' + r"
          orientation="horizontal"
          :style="{ gridRow: String(r * 2), gridColumn: `1 / ${gridCols * 2}` }"
          @drag="delta => onGridRowDrag(r - 1, delta)"
        />
      </template>
      <div v-if="termStore.tabs.length === 0" class="term-empty h-full flex items-center justify-center text-[var(--jc-text-muted)]">
        <div class="text-center">
          <el-icon class="term-empty__icon" size="48"><Monitor /></el-icon>
          <div class="term-empty__text mt-2 text-sm">{{ CAP.localTerminal() ? '点击「+」新建本地终端，或从会话列表连接主机' : '从会话列表连接主机，开始你的第一个 SSH 终端' }}</div>
          <!-- 移动端没有本地终端（后端无 PTY），空态给一个直达会话列表的入口，
               否则用户停在空页面上不知道下一步该去哪 -->
          <el-button
            v-if="!CAP.localTerminal()"
            class="term-empty__btn mt-3"
            size="small"
            type="primary"
            @click="dockStore.setActivePage('sessions')"
          >
            {{ t('mobile.goSessions') }}
          </el-button>
        </div>
      </div>
    </div>

    <!-- 右下角悬浮快捷操作：AI Agent / 精简模式 / 全屏（F11）/ 重连 -->
    <TerminalQuickActions
      :agent="agentEnabled"
      :zen="!!zen"
      :fullscreen="isFullscreen"
      :disconnected="activeDisconnected"
      @toggle-agent="toggleAgent"
      @toggle-zen="toggleZen"
      @toggle-fullscreen="toggleFullscreen"
      @reconnect="reconnectActive"
      @clear="clearActiveTerminal"
    />

    <!-- 左下角系统状态条：CPU / 内存 / 磁盘 占用率 + 网络上下行实时速率
         （终端铺满全高；底行有文字时状态条降到 25% 透明让内容透出来，底行空了自动恢复） -->
    <TerminalSystemStatus
      :connected="sysConnected"
      :kind="sysKind"
      :session-id="sysSessionId"
      :dimmed="sysStatusDimmed"
    />

    <!-- 连接会话时的「连接中」过渡遮罩：仅移动端（CAP.connectingOverlay），连接成功/失败即消失。
         以「正在连接的激活 Tab id」为 key：上一个会话还在连接中时再点开下一个，遮罩元素不会卸载，
         没有 key 就不会重播进入动画；加 key 后每次新建/切换会话都强制重新挂载→重播动画 -->
    <Transition name="jc-connect-fade">
      <TerminalConnectingOverlay v-if="showConnectingOverlay" :key="connectingKey" :host="connectingHost" />
    </Transition>

    <!-- Agent 未登录授权提示浮层 -->
    <div
      v-if="agentAuthHint.visible && agentAuthHint.tabId === termStore.activeTabId"
      class="absolute left-1/2 bottom-16 z-40 -translate-x-1/2 pointer-events-auto flex items-center gap-3 px-4 py-2.5 rounded-lg border border-[var(--jc-border)] bg-[var(--jc-bg-panel)] shadow-lg"
    >
      <span class="text-sm text-[var(--jc-text)] whitespace-nowrap">开启 Agent 模式需要登录授权</span>
      <el-button type="primary" size="small" :loading="authStore.loading" @click="onAgentLoginClick">
        立即登录
      </el-button>
      <button class="text-[var(--jc-text-muted)] hover:text-[var(--jc-text)]" @click="agentAuthHint.visible = false">
        <el-icon size="14"><Close /></el-icon>
      </button>
    </div>

    <!-- 拖拽悬停点击进度环 -->
    <div
      v-if="hoverClick.active"
      class="fixed z-[999] pointer-events-none"
      :style="{ left: hoverClick.x + 'px', top: hoverClick.y + 'px' }"
    >
      <svg width="28" height="28" viewBox="0 0 28 28" style="transform: translate(-50%, -50%)">
        <circle cx="14" cy="14" r="11" fill="var(--jc-bg-hover)" stroke="var(--jc-border)" stroke-width="2" />
        <circle
          cx="14" cy="14" r="11" fill="none" stroke="var(--jc-accent)" stroke-width="2"
          stroke-linecap="round"
          :stroke-dasharray="CIRC"
          :stroke-dashoffset="CIRC * (1 - hoverClick.progress)"
          transform="rotate(-90 14 14)"
        />
      </svg>
    </div>

    <!-- 终端空白区右键菜单（复用通用两级菜单组件） -->
    <FsContextMenu
      v-if="ctxMenu.visible"
      :x="ctxMenu.x"
      :y="ctxMenu.y"
      :items="sessionMenuItems"
      @select="onSessionMenuSelect"
      @close="ctxMenu.visible = false"
    />
  </div>
</template>

<script setup lang="ts">
/**
 * 终端工作区（主窗口与独立终端窗口共用）
 * 作者：JC
 * - TabBar + 分屏铺排（单 Tab / 左右 / 上下 / 网格，不足补本地终端）
 * - 真实会话接线：本地 PTY（create_local_shell）、SSH（connect_ssh），
 *   输出统一经 ssh-output 事件路由到对应终端
 * - Tab 拖拽排序 / 拖出：跨窗口迁移（jc:tab-transfer）或新开独立窗口（take 接管模式），
 *   缓冲区经 serialize + stash/take_term_buffer 延续
 * - 本地 OS 文件拖入（tauri://drag-drop 按坐标命中放置区）与拖拽悬停点击
 */
import { nextTick, onMounted, onBeforeUnmount, computed, watch, ref, reactive, h } from 'vue'
import { ElMessage, ElMessageBox, ElInput, ElCheckbox } from 'element-plus'
import { handleApiError, friendlyText } from '@/utils/errorHandler'
import { useI18nStore } from '@/stores/i18n'
import { useDockStore } from '@/stores/dock'
import { Monitor, Close, Plus, CopyDocument, RefreshRight, Link, Delete, DocumentCopy, ZoomIn, ZoomOut, Operation } from '@element-plus/icons-vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { PhysicalSize, PhysicalPosition } from '@tauri-apps/api/dpi'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { useTerminalStore, type TerminalTab, type SplitMode } from '@/stores/terminal'
import { useHostStore } from '@/stores/host'
import { useSettingsStore } from '@/stores/settings'
import { useAppStore } from '@/stores/app'
import { useAuthStore } from '@/stores/auth'
import { invoke, isTauriEnv } from '@/api'
import { copyText } from '@/utils/clipboard'
import { CAP, isMobileWebview } from '@/utils/platform'
import TerminalConnectingOverlay from '@/components/terminal/TerminalConnectingOverlay.vue'
import TerminalTabBar from './TerminalTabBar.vue'
import TerminalQuickActions from './TerminalQuickActions.vue'
import TerminalSystemStatus from './TerminalSystemStatus.vue'
import { termBottomOccupied } from './termBottomStatus'
import SplitPane from './SplitPane.vue'
import SplitBar from '@/components/common/SplitBar.vue'
import FsContextMenu from '@/components/filesystem/FsContextMenu.vue'
import type { FsMenuItem } from '@/components/filesystem/FsContextMenu.vue'
import { useAgentStore, AGENT_GHOST_HINT, type AgentAction, type AgentPending } from '@/stores/agent'
import { getShellCwd } from '@/composables/useFileDrag'
import type { HostItem } from '@/stores/host'
import {
  initWindowRegistry, requestTabTransfer, windowLabel, closeCurrentWindow,
  type TransferredTab
} from '@/composables/useWindowRegistry'
import { findOsDropZoneAt, handleOsFilesToZone } from '@/composables/useFileDrag'
import { termSync, onSyncInput, onSyncFs, type FsSyncPayload } from '@/composables/useTermSync'
import { comboOfEvent } from '@/composables/useShortcuts'
import { isUiFrozen, onUiThaw } from '@/utils/uiFreeze'
import { initDragHoverClick, dragHoverClickState as hoverClick, osDragHoverAt, osDragHoverClear } from '@/composables/useDragHoverClick'
import { addKnownHost } from '@/utils/knownHosts'
import { resolveSshProxy } from '@/utils/sshProxy'

const props = defineProps<{
  /** 独立窗口接管模式：从 URL query 还原的 Tab 元数据 */
  takeTab?: TransferredTab | null
  /** 精简模式：隐藏 Tab 标签栏，只留终端本体（标题栏/停靠面板由 DockLayout 隐藏） */
  zen?: boolean
}>()

const termStore = useTerminalStore()
const hostStore = useHostStore()
const settingsStore = useSettingsStore()
const app = useAppStore()
const agentStore = useAgentStore()
const authStore = useAuthStore()
const dockStore = useDockStore()
const t = useI18nStore().t

// ===== AI Agent 模式 =====
/** 当前激活 Tab 的 Agent 模式状态（驱动按钮高亮） */
const agentEnabled = computed(() => agentStore.isEnabled(termStore.activeTabId))

/** Agent 模式下未登录/未授权时，在终端区域显示的登录引导浮层 */
const agentAuthHint = ref<{ visible: boolean; tabId: string }>({ visible: false, tabId: '' })

/** 生成仿真 prompt（Agent 提交后的新行前缀；尽量贴近真实 shell 提示符） */
function buildPrompt(tab: TerminalTab, cwd: string): string {
  const c = cwd || '~'
  if (tab.kind === 'ssh') {
    // 仿 bash 默认 PS1：user@host:cwd$（root 为 #）；家目录折叠为 ~
    const user = tab.username || 'root'
    const host = tab.hostname || tab.title || 'remote'
    const home = user === 'root' ? '/root' : `/home/${user}`
    const shown = c === home ? '~' : (c.startsWith(home + '/') ? '~' + c.slice(home.length) : c)
    return `${user}@${host}:${shown}${user === 'root' ? '#' : '$'} `
  }
  const st = (settingsStore.settings.defaultShell || 'powershell').toLowerCase()
  if (st.includes('cmd')) return `${c}> `
  return `PS ${c}> `
}

/** 开关 Agent 模式（Tab 级）：保留真实 shell 提示符，输入位置显示浅灰幽灵提示 */
async function toggleAgent(): Promise<void> {
  const tabId = termStore.activeTabId
  if (!tabId) { ElMessage.warning('请先打开一个终端'); return }
  const tab = termStore.tabs.find(t => t.id === tabId)
  if (!tab?.connected || !tab.sessionId) { ElMessage.warning('终端未连接，无法开启 AI Agent'); return }
  const on = agentStore.toggle(tabId)
  const term = paneRefs.get(tabId)?.term as any
  if (!on) {
    // 退出 Agent 模式：若有进行中的思考/待确认方案，一并收尾（浮层、动画行、预留空行）
    const p = agentStore.pending
    if (p && p.tabId === tabId) { closeAgentConfirm(tabId); agentStore.clearPending() }
    if (agentAuthHint.value.tabId === tabId) agentAuthHint.value.visible = false
  } else {
    // 仿真 prompt 优先取屏幕上 shell 的真实提示符（与真实 prompt 完全一致，
    // 避免出现 root@IP 与 root@host 两种写法）；读不到时再用 cwd 估算兜底。
    const real = term?.currentPromptText?.() ?? ''
    if (real) {
      agentStore.setPrompt(tabId, real)
    } else {
      try {
        const cwd = await getShellCwd(tab.sessionId, tab.kind === 'local')
        agentStore.setPrompt(tabId, buildPrompt(tab, cwd))
      } catch { /* prompt 获取失败时使用默认 '> ' */ }
    }
  }
  // 移动端不主动 focus：focus 会唤起软键盘，把终端内容顶掉大半屏。
  // 用户要输入时自己点一下终端即可，切换开关不该带出键盘。
  if (!isMobileWebview()) term?.focus?.()
}

/** 点击 Agent 未登录浮层的「立即登录」：拉起 OAuth 登录流程并隐藏浮层 */
function onAgentLoginClick() {
  agentAuthHint.value.visible = false
  authStore.startLogin().catch(() => {})
}

/** 剥离终端序列化内容中的 ANSI 转义序列（供 AI 阅读纯文本） */
function stripAnsi(s: string): string {
  // eslint-disable-next-line no-control-regex
  return s.replace(/\x1b(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~]|\][^\x07]*(?:\x07|\x1b\\))/g, '')
}

// ===== Agent 终端内交互（思考动画 / 方案渲染 / 风险分级执行） =====
/** 思考动画计时器（每 Tab 一个） */
const spinnerTimers = new Map<string, ReturnType<typeof setInterval>>()
const SPINNER_FRAMES = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏']

function agentTerm(tabId: string): any {
  return paneRefs.get(tabId)?.term as any
}

/** 启动「AI 正在思考」动画（单行覆盖刷新；pending 消失时自愈停止） */
function startSpinner(tabId: string): void {
  stopSpinner(tabId)
  let i = 0
  const timer = setInterval(() => {
    const p = agentStore.pending
    if (!p || p.tabId !== tabId || !p.loading) { stopSpinner(tabId); return }
    const frame = SPINNER_FRAMES[i++ % SPINNER_FRAMES.length]
    agentTerm(tabId)?.write(`\r\x1b[0K\x1b[36m${frame} AI 正在思考...\x1b[0m  \x1b[90m(Ctrl+C 终止)\x1b[0m`)
  }, 120)
  spinnerTimers.set(tabId, timer)
  // 首帧立即输出，避免 120ms 空白
  agentTerm(tabId)?.write(`\x1b[36m${SPINNER_FRAMES[0]} AI 正在思考...\x1b[0m  \x1b[90m(Ctrl+C 终止)\x1b[0m`)
}

function stopSpinner(tabId: string): void {
  const t = spinnerTimers.get(tabId)
  if (t) { clearInterval(t); spinnerTimers.delete(tabId) }
}

/** 擦除思考动画所在行（无换行，光标仍在该行） */
function eraseSpinnerLine(tabId: string): void {
  stopSpinner(tabId)
  agentTerm(tabId)?.write('\r\x1b[0K')
}

/** 确认等待动画：写入仿真 prompt + 动态转圈，表示正在等待用户确认 */
function startConfirmSpinner(tabId: string): void {
  stopSpinner(tabId)
  const term = agentTerm(tabId)
  if (!term) return
  // 写入仿真 prompt
  term.write(agentStore.getPrompt(tabId) || '> ')
  // 启动转圈动画
  let i = 0
  const timer = setInterval(() => {
    const p = agentStore.pending
    if (!p || p.tabId !== tabId || p.result) { stopSpinner(tabId); return }
    const frame = SPINNER_FRAMES[i++ % SPINNER_FRAMES.length]
    term.write(`\r\x1b[0K${agentStore.getPrompt(tabId) || '> '}\x1b[36m${frame} 等待确认...\x1b[0m`)
  }, 120)
  spinnerTimers.set(tabId, timer)
  // 首帧立即输出
  term.write(`\x1b[36m${SPINNER_FRAMES[0]} 等待确认...\x1b[0m`)
}

/** 流程结束后给出新的输入行（仿真 prompt + 幽灵提示） */
function newAgentInputLine(tabId: string): void {
  agentTerm(tabId)?.agentNewLine?.()
  agentTerm(tabId)?.focus?.()
}

// ===== Agent 确认浮层：代码块 + 按钮锚定在终端行上（鼠标可点，键盘快捷键并存） =====
/** 每 Tab 的确认浮层释放函数 */
const overlayDisposers = new Map<string, () => void>()
/** 每 Tab 浮层占据的空行数（用于移除后擦除） */
const overlayLines = new Map<string, number>()

function dismissAgentOverlay(tabId: string): void {
  const d = overlayDisposers.get(tabId)
  if (d) { overlayDisposers.delete(tabId); try { d() } catch { /* 终端已销毁时忽略 */ } }
  overlayLines.delete(tabId)
}

/** 结束一次确认交互：停动画 → 移浮层 DOM → 擦当前行 → 真删浮层预留的空行。
 *  处理完光标停在原预留区首行，后续输出直接接在方案说明之后，不留空行残留 */
function closeAgentConfirm(tabId: string): void {
  const reserved = overlayLines.get(tabId) ?? 0
  const term = agentTerm(tabId)
  stopSpinner(tabId)
  dismissAgentOverlay(tabId)
  term?.write('\r\x1b[0K')
  if (reserved > 0) term?.eraseReservedLines?.(reserved)
}

/** 构建确认浮层 DOM：风险标题 + 代码块（命令预览，可复制）+ 操作按钮。
 *  refocus 在任意点击交互后恢复终端焦点，保证键盘快捷键持续可用 */
function buildConfirmOverlay(danger: boolean, cmds: string[], onAction: (a: AgentAction) => void, refocus: () => void): { el: HTMLElement; heightPx: number } {
  const accent = danger ? '#ff6b6b' : '#e8b33f'
  const el = document.createElement('div')
  // 获取终端实际背景色（从终端容器计算样式），确保浮层背景与终端一致
  const termContainer = document.querySelector('.terminal-container') as HTMLElement | undefined
  const bgColor = termContainer ? getComputedStyle(termContainer).backgroundColor : app.terminalBgColor
  el.style.cssText = 'display:flex;flex-direction:column;gap:8px;padding:10px 12px;max-width:680px;' +
    `background:${bgColor};border:1px solid ${accent}59;border-radius:8px;` +
    'box-shadow:0 6px 20px rgba(0,0,0,0.35);font-size:13px;'
  // 标题行
  const head = document.createElement('div')
  head.style.cssText = `display:flex;align-items:center;gap:6px;color:${accent};font-weight:600;`
  head.textContent = danger ? '⚠ 高危操作，请确认后执行' : '⚠ 该操作存在风险，请确认后执行'
  el.appendChild(head)
  // 代码块：头部（标签 + 复制 + 操作按钮）+ 命令列表
  const block = document.createElement('div')
  block.style.cssText = 'border:1px solid var(--jc-border);border-radius:6px;overflow:hidden;'
  const blockHead = document.createElement('div')
  blockHead.style.cssText = 'display:flex;align-items:center;justify-content:space-between;padding:4px 10px;' +
    'background:rgba(127,127,127,0.10);color:var(--jc-text-muted);font-size:12px;'
  blockHead.appendChild(document.createTextNode('待执行命令'))
  
  // 右侧按钮组：复制 + 执行/修改/拒绝
  const rightGroup = document.createElement('div')
  rightGroup.style.cssText = 'display:flex;align-items:center;gap:8px;'
  
  // 复制按钮（带反馈：点击后变为对勾 3 秒）
  const copyBtn = document.createElement('span')
  copyBtn.textContent = '⧉ 复制'
  copyBtn.style.cssText = 'cursor:pointer;user-select:none;'
  copyBtn.onmouseenter = () => { if (copyBtn.textContent !== '✓ 已复制') copyBtn.style.color = 'var(--jc-accent)' }
  copyBtn.onmouseleave = () => { if (copyBtn.textContent !== '✓ 已复制') copyBtn.style.color = '' }
  copyBtn.onclick = () => {
    copyText(cmds.join('\n')).catch(() => { /* 忽略 */ })
    // 反馈：变为对勾，3 秒后恢复
    copyBtn.textContent = '✓ 已复制'
    copyBtn.style.color = '#4ade80'
    setTimeout(() => {
      copyBtn.textContent = '⧉ 复制'
      copyBtn.style.color = ''
    }, 3000)
    refocus()
  }
  rightGroup.appendChild(copyBtn)
  
  // 操作按钮：执行 / 修改 / 拒绝
  const mkBtn = (label: string, color: string, bg: string, hoverBg: string, action: AgentAction): HTMLButtonElement => {
    const b = document.createElement('button')
    b.textContent = label
    b.style.cssText = `padding:3px 10px;border-radius:4px;border:1px solid ${color}80;` +
      `background:${bg};color:${color};font-size:12px;cursor:pointer;`
    b.onmouseenter = () => { b.style.background = hoverBg }
    b.onmouseleave = () => { b.style.background = bg }
    b.onclick = () => { onAction(action); refocus() }
    return b
  }
  rightGroup.appendChild(mkBtn('执行', '#16a34a', '#f0fdf4', '#dcfce7', 'execute'))
  rightGroup.appendChild(mkBtn('修改', '#52525b', '#f4f4f5', '#e4e4e7', 'modify'))
  rightGroup.appendChild(mkBtn('拒绝', '#dc2626', '#fef2f2', '#fee2e2', 'reject'))
  
  blockHead.appendChild(rightGroup)
  block.appendChild(blockHead)
  const codeBody = document.createElement('div')
  codeBody.style.cssText = 'padding:8px 10px;display:flex;flex-direction:column;gap:4px;' +
    'font-family:Consolas,"Cascadia Code",Menlo,monospace;background:rgba(127,127,127,0.06);'
  for (const c of cmds) {
    const line = document.createElement('div')
    line.style.cssText = 'white-space:pre-wrap;word-break:break-all;'
    const pfx = document.createElement('span')
    pfx.textContent = '❯ '
    pfx.style.color = 'var(--jc-accent)'
    line.appendChild(pfx)
    line.appendChild(document.createTextNode(c))
    codeBody.appendChild(line)
  }
  block.appendChild(codeBody)
  el.appendChild(block)
  // 高度估算：标题 20 + 代码块（头 26 + 每行 20 + 内边距 16）+ 间距/外边距 24
  return { el, heightPx: 86 + cmds.length * 20 }
}

/** Agent 提交：采集上下文 → 请求服务端网关 → 终端内渲染结果（无弹层） */
async function onAgentSubmit(tabId: string, prompt: string): Promise<void> {
  const tab = termStore.tabs.find(t => t.id === tabId)
  if (!tab?.sessionId) return
  const term = agentTerm(tabId)
  let cwd = ''
  try { cwd = await getShellCwd(tab.sessionId, tab.kind === 'local') } catch { /* 忽略 */ }
  // 终端最近输出（纯文本，末尾 1500 字符；剔除幽灵提示文案避免污染上下文）
  let recentOutput = ''
  try {
    const raw: string = term?.serialize?.() ?? ''
    recentOutput = stripAnsi(raw).split('\n').map(l => l.trimEnd()).filter(Boolean).slice(-30).join('\n')
    recentOutput = recentOutput.split(AGENT_GHOST_HINT).join('')
    if (recentOutput.length > 1500) recentOutput = recentOutput.slice(-1500)
  } catch { /* 忽略 */ }
  startSpinner(tabId)
  await agentStore.submit(tabId, prompt, {
    shellType: tab.kind === 'local' ? (settingsStore.settings.defaultShell || 'auto') : 'sh/bash',
    os: tab.kind === 'local' ? 'windows' : 'linux(远端,假定)',
    cwd,
    target: tab.kind === 'local' ? 'local' : `ssh:${tab.title}`,
    recentOutput: recentOutput || undefined
  })
  const p = agentStore.pending
  if (!p || p.tabId !== tabId) { stopSpinner(tabId); return } // 已被中止/关闭
  // 同步最新积分余额（供 AI 助手卡片摘要展示）
  const bal = p.result?.balanceRemaining
  if (typeof bal === 'number') agentStore.setBalance(bal)
  await renderAgentResult(tabId, p)
}

/** 终端内渲染 AI 方案：safe 直接执行；caution/danger 终端内确认；blocked 拒绝 */
async function renderAgentResult(tabId: string, p: AgentPending): Promise<void> {
  // 竞态防护：结果到达时用户已中止/切换（pending 已被清除或替换），丢弃迟到结果
  if (agentStore.pending !== p) { stopSpinner(tabId); return }
  const term = agentTerm(tabId)
  eraseSpinnerLine(tabId)
  // 请求失败
  if (p.error) {
    // 未登录 / 未授权等鉴权错误，给出友好提示而非暴露内部错误文案
    // 鉴权判断必须使用原始错误（rawError），展示用已脱敏的 error
    const raw = p.rawError ?? p.error
    const isAuthError = /未授权|未登录|登录态|Unauthorized|401|授权/.test(raw)
    const msg = isAuthError ? '开启 Agent 模式需要登录授权' : p.error
    term?.write(`\x1b[31m✗ ${msg}\x1b[0m\r\n`)
    agentStore.clearPending()
    if (isAuthError && termStore.activeTabId === tabId) {
      agentAuthHint.value = { visible: true, tabId }
    }
    newAgentInputLine(tabId)
    return
  }
  const r = p.result
  if (!r) { agentStore.clearPending(); newAgentInputLine(tabId); return }
  // AI 说明（暗色，可多行）
  if (r.explanation) {
    for (const line of r.explanation.split('\n')) {
      term?.write(`\x1b[90m${line.trimEnd()}\x1b[0m\r\n`)
    }
  }
  const cmds = (r.commands ?? []).map(c => c.cmd).filter(Boolean)
  // 需要补充信息 / 未给出命令：打印追问后回到输入行
  if (r.needMoreInfo || cmds.length === 0) {
    if (r.question) term?.write(`\x1b[36m${r.question}\x1b[0m\r\n`)
    agentStore.clearPending()
    newAgentInputLine(tabId)
    return
  }
  // 本地/服务端合并后的最终风险等级
  const level = r.finalRiskLevel
  if (level === 'blocked') {
    term?.write(`\x1b[31m⛔ 已拦截：该请求命中高危规则，不予执行${r.guardReason ? `（${r.guardReason}）` : ''}\x1b[0m\r\n`)
    agentStore.clearPending()
    newAgentInputLine(tabId)
    return
  }
  if (level === 'safe') {
    term?.write(`\x1b[32m✓ 低风险，直接执行\x1b[0m\r\n`)
    agentStore.clearPending()
    void executeAgentCommands(tabId, cmds)
    return
  }
  // caution/danger：必须用户确认。
  // 策略：先写空行供浮层锚定，浮层成功则不写纯文本（避免移除后残留）；
  // 浮层失败则回退写纯文本 + 键盘确认。
  const danger = level === 'danger'
  dismissAgentOverlay(tabId)
  const { el, heightPx } = buildConfirmOverlay(danger, cmds,
    (a) => void onAgentAction(tabId, a),
    () => agentTerm(tabId)?.focus?.()
  )
  // 预留行数：优先用浮层真实渲染高度测量（+1 行作为与卡片下沿的间距），测不到时回退估算值
  const totalLines = (term?.measureOverlayLines?.(el) as number | undefined) || Math.ceil(heightPx / 18) + 1
  // 写入空行供浮层定位
  await term?.writeDone?.('\r\n'.repeat(totalLines))
  await new Promise<void>(resolve => setTimeout(resolve, 50))
  if (agentStore.pending !== p) { el.remove(); return }
  const dispose = term?.agentOverlay?.(-totalLines, el, heightPx) ?? null
  if (dispose) {
    // 浮层成功：记录空行数，启动 spinner，不写纯文本
    overlayDisposers.set(tabId, dispose)
    overlayLines.set(tabId, totalLines)
    startConfirmSpinner(tabId)
  } else {
    // 浮层失败：先真删预留空行，再在光标处写纯文本 + 快捷键提示（键盘确认模式）
    term?.eraseReservedLines?.(totalLines)
    term?.write(danger
      ? `\x1b[31m⚠ 高危操作，请确认后执行${r.guardReason ? `（${r.guardReason}）` : ''}\x1b[0m\r\n`
      : `\x1b[33m⚠ 该操作存在风险，请确认后执行${r.guardReason ? `（${r.guardReason}）` : ''}\x1b[0m\r\n`)
    for (const c of cmds) term?.write(`\x1b[32m$ ${c}\x1b[0m\r\n`)
    term?.write(`\x1b[93m[Enter] 执行   [M] 修改   [Esc] 拒绝\x1b[0m`)
  }
  // pending 保留，TerminalView 拦截确认按键
}

/** 把命令逐条写入会话执行（与真人打字同一链路；首条前补仿真 prompt 使回显自然） */
async function executeAgentCommands(tabId: string, commands: string[]): Promise<void> {
  const tab = termStore.tabs.find(t => t.id === tabId)
  if (!tab?.sessionId) return
  const term = agentTerm(tabId)
  // 记录本次任务的代次：Ctrl+C 会自增代次，据此立刻停止继续下发后续命令
  const epoch = agentStore.currentEpoch(tabId)
  const cancelled = () => agentStore.currentEpoch(tabId) !== epoch
  try {
    term?.write(agentStore.getPrompt(tabId) || '> ')
    for (const cmd of commands) {
      if (cancelled()) return
      // 记入 Agent 历史：Agent 模式下 ↑ 也能像 Shell 模式一样调出刚执行过的命令
      agentStore.pushHistory(tabId, cmd)
      agentStore.setLastCommand(tabId, cmd)
      await invoke('send_session_input', { dto: { session_id: tab.sessionId, data: cmd + '\r' } })
      await new Promise(r => setTimeout(r, 300))
    }
  } catch (e: any) {
    if (cancelled()) return
    term?.write(`\x1b[31m命令写入终端失败: ${friendlyText(e?.message || String(e))}\x1b[0m\r\n`)
    newAgentInputLine(tabId)
  } finally {
    if (!cancelled()) {
      term?.focus?.()
      refreshAgentPrompt(tabId)
    }
  }
}

/** 终端内确认/控制动作：执行 / 修改 / 拒绝 / 中止 */
async function onAgentAction(tabId: string, action: AgentAction): Promise<void> {
  const p = agentStore.pending
  if (!p || p.tabId !== tabId) return
  const term = agentTerm(tabId)
  const tab = termStore.tabs.find(t => t.id === tabId)
  // 先收尾：浮层与它占用的空行一并清除，光标回到方案说明的下一行
  closeAgentConfirm(tabId)
  if (action === 'abort') {
    // 用本次请求的客户端 ID 通知后端真正取消在途 AI 请求（否则模型仍会跑完并计费）
    invoke('ai_abort', { conversationId: p.clientReqId ?? '' }).catch(() => { /* 取消失败忽略 */ })
    term?.write('\x1b[33m已终止\x1b[0m\r\n')
    agentStore.clearPending()
    newAgentInputLine(tabId)
    return
  }
  if (!p.result || !tab?.sessionId) return
  const cmds = (p.result.commands ?? []).map(c => c.cmd).filter(Boolean)
  if (action === 'execute') {
    agentStore.clearPending()
    await executeAgentCommands(tabId, cmds)
    return
  }
  if (action === 'modify') {
    // 命令文本直达真实 shell（不带回车），用户在 shell 里用 readline 编辑后自行回车执行
    term?.write(agentStore.getPrompt(tabId) || '> ')
    agentStore.clearPending()
    agentStore.setPassthrough(tabId, true)
    const joined = cmds.join(' && ')
    agentStore.pushHistory(tabId, joined)
    agentStore.setLastCommand(tabId, joined)
    try {
      await invoke('send_session_input', { dto: { session_id: tab.sessionId, data: joined } })
    } catch { /* 失败时保持 Agent 模式 */ }
    term?.focus?.()
    return
  }
  // reject
  term?.write('\x1b[90m已取消，未执行任何命令\x1b[0m\r\n')
  agentStore.clearPending()
  newAgentInputLine(tabId)
}

/** 后台刷新 Agent 模式的仿真 prompt：
 *  直接读屏幕上 shell 的真实提示符（cd 后目录变化也会自然反映）；
 *  若此刻命令仍在执行（不在提示符行）则保持现有 prompt，等 TerminalView 的幽灵提示时机自动再捕获 */
function refreshAgentPrompt(tabId: string): void {
  if (!agentStore.isEnabled(tabId)) return
  const real = (agentTerm(tabId) as any)?.currentPromptText?.() ?? ''
  if (real) agentStore.setPrompt(tabId, real)
}

// ===== 悬浮快捷操作：精简模式 / 全屏（F11）/ 重连 =====
/** 进入精简模式时的窗口缩小比例（退出时恢复原尺寸） */
const ZEN_SCALE = 0.85
/** 进入精简模式前的窗口尺寸/位置（退出时恢复；最大化进入不缩放） */
let zenRestore: { size: PhysicalSize; pos: PhysicalPosition } | null = null

async function toggleZen(): Promise<void> {
  // 精简模式窗口尺寸：进入时稍微缩小并居中，退出时恢复原尺寸/位置
  if (isTauriEnv()) {
    const win = getCurrentWindow()
    if (!app.zenMode) {
      try {
        if (!(await win.isMaximized())) {
          zenRestore = { size: await win.outerSize(), pos: await win.outerPosition() }
          await win.setSize(new PhysicalSize(
            Math.round(zenRestore.size.width * ZEN_SCALE),
            Math.round(zenRestore.size.height * ZEN_SCALE)
          ))
          await win.center()
        }
      } catch { /* 缩放失败不阻塞模式切换 */ }
    } else if (zenRestore) {
      try {
        await win.setSize(zenRestore.size)
        await win.setPosition(zenRestore.pos)
      } catch { /* 恢复失败忽略 */ }
      zenRestore = null
    }
  }
  app.toggleZenMode()
}

/** 当前激活 Tab 是否断连（重连按钮高亮提示） */
const activeDisconnected = computed(() => {
  const tab = termStore.tabs.find(t => t.id === termStore.activeTabId)
  return !!tab && !tab.connected
})

// ===== 左下角系统状态条：当前激活 Tab 的会话上下文（本地读本机 / SSH 读远端）=====
const activeSessionTab = computed(() => termStore.tabs.find(tab => tab.id === termStore.activeTabId))
/** 移动端连接会话过渡遮罩：仅当「连接中」标记命中当前激活 Tab 时显示 */
const showConnectingOverlay = computed(() =>
  CAP.connectingOverlay() && termStore.connectingIds.includes(termStore.activeTabId)
)
/** 遮罩 key：取当前正在连接的激活 Tab id。
 *  这样在「上一个会话还连接中时又点开下一个」等连续场景里，遮罩元素会被判定为不同节点，
 *  Transition 因此同时跑 leave + enter，进入动画每次都重新播放（否则 v-if 一直为 true，动画只播一次）。 */
const connectingKey = computed(() =>
  termStore.connectingIds.includes(termStore.activeTabId) ? termStore.activeTabId : ''
)
const connectingHost = computed(() => activeSessionTab.value?.hostname || activeSessionTab.value?.host || '')
/** 本地会话无 host 也算已连接（与会话列表/设备面板口径一致） */
const sysConnected = computed(() =>
  !!activeSessionTab.value?.connected && (!!activeSessionTab.value?.host || activeSessionTab.value?.kind === 'local')
)
/** 会话类型：local 走本机 sysinfo，ssh 走远端采样 */
const sysKind = computed<'local' | 'ssh'>(() => activeSessionTab.value?.kind ?? 'local')
const sysSessionId = computed(() => activeSessionTab.value?.sessionId)
/** 当前激活终端的视口最底行是否被文字占用：占用时状态条降到 25% 不透明度，让下方内容透出来 */
const sysStatusDimmed = computed(() => !!termBottomOccupied[termStore.activeTabId])

/** 窗口全屏状态（驱动悬浮按钮高亮；同时同步到 app store 供 DockLayout 隐藏面板区） */
const isFullscreen = ref(false)
async function syncFullscreenState(): Promise<void> {
  try {
    const fs = await getCurrentWindow().isFullscreen()
    isFullscreen.value = fs
    app.setAppFullscreen(fs)
  } catch { /* ignore */ }
}

/** 全屏切换：Tauri 用窗口级全屏；非 Tauri 环境回退 Web Fullscreen API */
async function toggleFullscreen(): Promise<void> {
  if (isTauriEnv()) {
    const win = getCurrentWindow()
    await win.setFullscreen(!(await win.isFullscreen()))
    await syncFullscreenState()
    return
  }
  if (document.fullscreenElement) await document.exitFullscreen()
  else await document.documentElement.requestFullscreen?.()
}

/** F11 全屏快捷键：捕获阶段监听，终端输入框聚焦时也能拦截 */
function onFullscreenKey(e: KeyboardEvent): void {
  if (e.key !== 'F11') return
  e.preventDefault()
  toggleFullscreen().catch(() => {})
}

/** 切换 Agent/Shell 模式快捷键（捕获阶段监听，终端聚焦时也能拦截；组合键可在「设置-快捷键」自定义，默认 Ctrl+Shift+I） */
function onAgentKey(e: KeyboardEvent): void {
  // 设置弹窗的快捷键录入框等表单控件内不触发；但终端自身的隐藏输入框（xterm-helper-textarea）必须放行，
  // 否则快捷键在终端聚焦时失效。
  const t = e.target as HTMLElement | null
  if (t) {
    const tag = t.tagName
    const isXtermInput = tag === 'TEXTAREA' && t.classList.contains('xterm-helper-textarea')
    if (!isXtermInput && (tag === 'INPUT' || tag === 'TEXTAREA' || t.isContentEditable)) return
  }
  const combo = settingsStore.settings.shortcuts.toggleAgent
  if (!combo || comboOfEvent(e) !== combo) return
  e.preventDefault()
  toggleAgent().catch(() => {})
}

/**
 * ESC 快捷键（捕获阶段监听，终端聚焦时也能拦截）
 * - 全屏模式 → 退出全屏
 * - 精简模式 → 退出精简模式（并恢复窗口原尺寸）
 * 全屏原有的 F11 / 全屏按钮、精简原有的悬浮按钮退出方式均保持不变。
 */
function onEscapeKey(e: KeyboardEvent): void {
  if (e.key !== 'Escape') return
  // 有模态弹窗打开时 ESC 优先交给弹窗自己关闭。
  // 注意：不能直接用 querySelector('.el-overlay') 判断——弹窗关闭后其 DOM 仍常驻（display:none），
  // 一旦打开过任意弹窗就会永久命中，导致 ESC 彻底失效。改用 Element Plus 打开弹窗时给 body 加的标记类。
  if (document.body.classList.contains('el-popup-parent--hidden')) return
  if (app.appFullscreen) {
    e.preventDefault()
    e.stopPropagation()
    toggleFullscreen().catch(() => {})
    return
  }
  if (app.zenMode) {
    e.preventDefault()
    e.stopPropagation()
    toggleZen().catch(() => {})
  }
}

/** Web 全屏变化同步（非 Tauri 环境） */
function onWebFullscreenChange(): void {
  isFullscreen.value = !!document.fullscreenElement
  app.setAppFullscreen(isFullscreen.value)
}

// ===== 布局容器 class =====
// 分隔不再用 gap：改由插入的 SplitBar（5px）本身充当分界线，可直接拖拽
const containerClass = computed(() => {
  switch (termStore.splitMode) {
    // 分屏分界线：比单元内终端/fs 分隔条更宽更亮，突出分屏边界
    case 'h': return 'flex flex-row bg-[var(--jc-split-line)]'
    case 'v': return 'flex flex-col bg-[var(--jc-split-line)]'
    case 'grid': return 'grid bg-[var(--jc-split-line)]'
    default: return 'relative bg-[var(--jc-bg)]'
  }
})
const paneClass = computed(() => termStore.splitMode === 'none' ? 'absolute inset-0' : 'flex-1 min-w-0 min-h-0')

// ===== 分屏尺寸（可拖拽） =====
/** 分屏布局容器（用于把拖拽像素换算成权重增量） */
const layoutRef = ref<HTMLElement | null>(null)
/**
 * h / v 模式下各面板的尺寸权重（与面板顺序一一对应，按 flex-grow 分配）。
 * 用权重而非固定像素，窗口缩放时比例自然保持。
 */
const weights = ref<number[]>([])
/** grid 模式：每一列 / 每一行的尺寸权重（按 fr 分配） */
const gridColWeights = ref<number[]>([])
const gridRowWeights = ref<number[]>([])
/** 网格列数（与旧逻辑一致：≤4 个面板两列，更多三列） */
const gridCols = computed(() => {
  const n = Math.max(termStore.tabs.length, 1)
  return n <= 4 ? 2 : 3
})
const gridRows = computed(() => Math.ceil(Math.max(termStore.tabs.length, 1) / gridCols.value))
const gridRowGapCount = computed(() => Math.max(gridRows.value - 1, 0))

/**
 * 网格竖向分界线的槽位（按行生成）。
 * 每行需要「该行面板数 - 1」根分界线；末行面板不满时，由最后一个面板横跨补齐剩余列，
 * 因此末行实际可分隔数同样等于面板数 - 1，不会出现画到空白处的线条。
 */
const gridColDividerSlots = computed(() => {
  const cols = gridCols.value
  const n = termStore.tabs.length
  const slots: { key: string; row: number; gap: number }[] = []
  for (let r = 0; r < gridRows.value; r++) {
    const items = Math.min(cols, n - r * cols)
    for (let g = 0; g < items - 1; g++) {
      slots.push({ key: `grid-col-${r}-${g}`, row: r, gap: g })
    }
  }
  return slots
})

/** 进入/切换分屏、面板数量变化时重置为等分 */
function resetWeights() {
  const mode = termStore.splitMode
  if (mode === 'grid') {
    weights.value = []
    gridColWeights.value = Array.from({ length: gridCols.value }, () => 1)
    gridRowWeights.value = Array.from({ length: gridRows.value }, () => 1)
    return
  }
  gridColWeights.value = []
  gridRowWeights.value = []
  weights.value = mode === 'none'
    ? []
    : Array.from({ length: termStore.tabs.length }, () => 1)
}
watch(
  [() => termStore.splitMode, () => termStore.tabs.length],
  resetWeights,
  { immediate: true }
)

// 分屏下关闭到仅剩一个终端时，自动退出分屏（回到单终端视图）
watch(
  () => termStore.tabs.length,
  (n) => {
    if (n <= 1 && termStore.splitMode !== 'none') termStore.setSplitMode('none')
    // 兜底：全部关闭后清理任何脱离 Vue 管理的残留终端容器（历史 WebGL 画布泄漏会整片盖黑，
    // 连空态与新会话一起挡掉）。正常情况下列表清空即已随组件卸载移除，这里只处理异常残留。
    if (n === 0) {
      nextTick(() => {
        layoutRef.value?.querySelectorAll('.terminal-container').forEach(el => el.remove())
      })
    }
  }
)

/** 面板内联尺寸：grid 用显式行列定位（奇数轨放面板，偶数轨留给分界线），h/v 用 flex-grow */
function paneStyle(i: number) {
  const mode = termStore.splitMode
  if (mode === 'none') return undefined
  if (mode === 'grid') {
    const cols = gridCols.value
    const n = termStore.tabs.length
    const c = i % cols
    const r = Math.floor(i / cols)
    const colStart = c * 2 + 1
    // 末行不满时，最后一个面板横跨剩余列，把关闭窗口腾出的空间填满
    const itemsInRow = Math.min(cols, n - r * cols)
    const colEnd = (i === n - 1 && itemsInRow < cols) ? cols * 2 : colStart + 1
    return { gridColumn: `${colStart} / ${colEnd}`, gridRow: String(r * 2 + 1) }
  }
  return { flexGrow: String(weights.value[i] ?? 1) }
}

/** 把权重数组拼成 CSS 轨道串：fr 轨道与 5px 分界线交替 */
function buildTracks(w: number[], n: number): string {
  const parts: string[] = []
  for (let i = 0; i < n; i++) {
    if (i > 0) parts.push('5px')
    parts.push(`${w[i] ?? 1}fr`)
  }
  return parts.join(' ')
}

const containerStyle = computed(() => {
  if (termStore.splitMode !== 'grid') return undefined
  return {
    gridTemplateColumns: buildTracks(gridColWeights.value, gridCols.value),
    gridTemplateRows: buildTracks(gridRowWeights.value, gridRows.value),
  }
})

/** 单个面板的最小尺寸，避免被拖到不可用 */
const MIN_PANE_PX = 80

/**
 * 在 arr[i] 与 arr[i+1] 之间转移权重：
 * delta 为鼠标位移（px），totalPx 为容器在该方向上的总尺寸。
 * 先按比例把像素换算成权重增量，再夹紧以保证两侧都不小于 MIN_PANE_PX。
 */
function shiftWeights(arr: number[], i: number, delta: number, totalPx: number) {
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

/** 左右/上下分屏：拖动第 i 与 i+1 个面板之间的分界线 */
function onDividerDrag(i: number, delta: number) {
  const el = layoutRef.value
  if (!el) return
  const totalPx = termStore.splitMode === 'v' ? el.clientHeight : el.clientWidth
  shiftWeights(weights.value, i, delta, totalPx)
}

/** 网格分屏：拖动列分界线（调整左右列宽） */
function onGridColDrag(gapIndex: number, delta: number) {
  const el = layoutRef.value
  if (!el) return
  shiftWeights(gridColWeights.value, gapIndex, delta, el.clientWidth)
}

/** 网格分屏：拖动行分界线（调整上下行高） */
function onGridRowDrag(gapIndex: number, delta: number) {
  const el = layoutRef.value
  if (!el) return
  shiftWeights(gridRowWeights.value, gapIndex, delta, el.clientHeight)
}

// ===== 终端引用 =====
const paneRefs = new Map<string, InstanceType<typeof SplitPane>>()
function setPaneRef(id: string, el: InstanceType<typeof SplitPane> | null) {
  if (el) paneRefs.set(id, el)
  else paneRefs.delete(id)
}

/** 等待 SplitPane 内终端挂载完成 */
function waitTermRef(tabId: string, timeout = 3000): Promise<any> {
  return new Promise((resolve, reject) => {
    const t0 = Date.now()
    const timer = setInterval(() => {
      const el = paneRefs.get(tabId)?.term
      if (el) { clearInterval(timer); resolve(el) }
      else if (Date.now() - t0 > timeout) { clearInterval(timer); reject(new Error('Terminal ref 等待超时')) }
    }, 20)
  })
}

/** sessionId → tabId 映射（ssh-output 路由） */
const sessionTabMap = new Map<string, string>()

// ===== 断连重连：按 R 键手动重连 + 按设置自动重连 =====
/** 每个 Tab 的自动重连已尝试次数（连接成功后清零） */
const retryCount = new Map<string, number>()
/** 重连并发保护：同一 Tab 同时只允许一次重连 */
const reconnecting = new Set<string>()
const AUTO_RETRY_DELAY_MS = 2000

/** 自动重连调度：开启时按最大次数重试，超限/未开启时由终端内按 R 键手动重连 */
function maybeAutoReconnect(tabId: string): void {
  const tab = termStore.tabs.find(t => t.id === tabId)
  if (!tab || tab.connected) return
  const s = settingsStore.settings
  if (!s.autoReconnect || (retryCount.get(tabId) || 0) >= s.maxReconnectTimes) return
  retryCount.set(tabId, (retryCount.get(tabId) || 0) + 1)
  paneRefs.get(tabId)?.term?.writeln(`\r\n\x1b[33m[自动重连 ${retryCount.get(tabId)}/${s.maxReconnectTimes}...，失败后按 R 键手动重连]\x1b[0m`)
  setTimeout(async () => {
    const t = termStore.tabs.find(x => x.id === tabId)
    if (!t || t.connected) return
    const ok = await reconnectTab(tabId)
    if (!ok) maybeAutoReconnect(tabId)
  }, AUTO_RETRY_DELAY_MS)
}

/** 打开本地终端（真实本地 PTY，进家目录）；指定 tabId 时在已有 Tab 内重连 */
async function openLocalTerminal(tabId?: string): Promise<boolean> {
  if (!CAP.localTerminal()) return false
  const targetTabId = tabId ?? termStore.createTab('本地终端', undefined, 'local')
  termStore.addConnecting(targetTabId)
  await nextTick()
  let term: any
  try {
    term = await waitTermRef(targetTabId)
  } catch {
    return false
  }
  try {
    // 用终端实际尺寸建 PTY：否则远端/本地 shell 按默认 80x24 建模，readline 重绘会错位
    const dim = term.dimensions?.() ?? { cols: 80, rows: 24 }
    const session = await invoke<{ id: string }>('create_local_shell', {
      cols: dim.cols,
      rows: dim.rows,
      // 默认 Shell 设置项（auto = 系统自动识别）
      shell: settingsStore.settings.defaultShell || 'auto'
    })
    sessionTabMap.set(session.id, targetTabId)
    termStore.updateTab(targetTabId, { sessionId: session.id, connected: true })
    retryCount.delete(targetTabId)
    // 立即重放缓存的输出（在 ssh-status connected 事件到达之前可能已缓存）
    const cached = pendingOutputs.get(session.id)
    if (cached && cached.length > 0) {
      for (const d of cached) term.write(d)
      pendingOutputs.delete(session.id)
    }
    term.focus()
    termStore.removeConnecting(targetTabId)
    return true
  } catch (e: any) {
    term.writeln(`\x1b[31m本地终端创建失败: ${friendlyText(e?.message || e)}\x1b[0m`)
    term.writeln('\x1b[36m按 R 键重新连接\x1b[0m')
    termStore.updateTab(targetTabId, { connected: false })
    termStore.removeConnecting(targetTabId)
    return false
  }
}

// ===== 打开主机（SSH） =====
/** 解析地址；缺省用户名/端口回退「主机与 SSH」设置项 */
function parseAddr(addr: string): { user: string; host: string; port: number } {
  const [user, rest] = addr.split('@')
  const [host, portStr] = (rest || '').split(':')
  return {
    user: user || settingsStore.settings.defaultUser || 'root',
    host: host || addr,
    port: portStr ? parseInt(portStr, 10) : (settingsStore.settings.defaultPort || 22)
  }
}

/** 按「通用-网络代理」设置解析代理参数（实现见 utils/sshProxy，与「测试连接」共用同一份） */
const resolveProxy = () => resolveSshProxy(settingsStore.settings)

/** 是否属于「认证失败（用户名/密码错误）」类错误：需要用弹窗让用户重输密码 */
function isAuthFailure(msg: string): boolean {
  return /认证失败|用户名或密码错误|permission denied|authentication failed|auth(?:entication)? fail|invalid password|incorrect password/i.test(msg)
}

/** 「记住密码」：写回该主机的会话配置，下次连接直接复用、不再弹窗 */
function persistHostPassword(host: HostItem, password: string): void {
  if (!host.id) return
  try {
    hostStore.updateHost(host.id, {
      config: { ...(host.config || {}), authType: 'password', password }
    })
  } catch { /* 保存失败不影响本次连接 */ }
}

/** 弹出 SSH 密码输入框（密码 + 记住密码复选框）；返回 null 表示用户取消。
 *  ElMessageBox.prompt 放不下额外控件，这里用内联组件承载输入框与复选框。 */
async function promptSshPassword(target: string, warn?: string): Promise<{ password: string; remember: boolean } | null> {
  const state = reactive({ password: '', remember: true })
  const PasswordForm = {
    setup() {
      return () => h('div', { style: 'display:flex;flex-direction:column;gap:10px;' }, [
        h('div', { style: 'font-size:12px;line-height:1.5;color:var(--el-text-color-secondary);word-break:break-all;' },
          t('terminal.ssh.connectTo', { target })),
        warn ? h('div', { style: 'font-size:12px;line-height:1.5;color:var(--el-color-danger);' }, warn) : null,
        h(ElInput, {
          modelValue: state.password,
          'onUpdate:modelValue': (v: string) => { state.password = v },
          type: 'password',
          placeholder: t('terminal.ssh.passwordPlaceholder'),
          autofocus: true,
          // 回车即确认（自定义内容时 MessageBox 不会自动绑定回车）
          onKeyup: (ev: KeyboardEvent) => {
            if (ev.key !== 'Enter') return
            const box = (ev.target as HTMLElement | null)?.closest?.('.el-message-box')
            const btn = box?.querySelector('.el-message-box__btns .el-button--primary') as HTMLElement | null
            btn?.click()
          }
        }),
        h(ElCheckbox, {
          modelValue: state.remember,
          'onUpdate:modelValue': (v: boolean | string | number) => { state.remember = !!v }
        }, () => t('terminal.ssh.rememberPassword'))
      ])
    }
  }
  try {
    await ElMessageBox({
      title: t('terminal.ssh.loginTitle'),
      message: h(PasswordForm),
      showCancelButton: true,
      confirmButtonText: t('terminal.ssh.connect'),
      cancelButtonText: t('common.cancel'),
      closeOnClickModal: false,
      beforeClose: (action: string, _instance: unknown, done: () => void) => {
        if (action === 'confirm' && !state.password) {
          ElMessage.warning(t('terminal.ssh.passwordRequired'))
          return
        }
        done()
      }
    })
  } catch {
    return null
  }
  return { password: state.password, remember: state.remember }
}

async function openHost(host: HostItem, tabId?: string): Promise<boolean> {
  if (!host.addr || host.type === 'folder') return false
  const { user, host: hostname, port } = parseAddr(host.addr)
  const cfg = host.config
  const username = cfg?.username || user
  const serverName = host.name || hostname
  const target = `${username}@${hostname}:${port}`
  const targetTabId = tabId ?? termStore.createTab(`${username}@${serverName}`, host.addr, 'ssh')
  // 标记「连接中」：驱动移动端连接过渡遮罩（连接成功/失败即移除，见下方各 return 前）
  termStore.addConnecting(targetTabId)
  // 上次该 Tab 是否就是「认证失败」结束的（重连时必须重新确认密码，不能静默复用会话里存的旧密码）
  const authFailedBefore = !!termStore.tabs.find(x => x.id === targetTabId)?.authFailed
  const tabPatch: Partial<TerminalTab> = { username, hostname: serverName }
  // 记住「从哪个会话条目打开」：同一地址可能存在多个会话（密码/密钥不同），
  // 重连必须对号入座，否则会串用另一个会话保存的凭据（表现为「密码不对却连上了」）
  if (host.id) tabPatch.hostId = host.id
  termStore.updateTab(targetTabId, tabPatch)

  const authType = cfg?.authType || (settingsStore.settings.defaultAuth === 'key' ? 'key' : 'password')
  let password = cfg?.password || ''
  // 私钥优先用「托管内容」：交给后端落到应用数据目录，返回可直接认证的绝对路径，
  // 因此用户删掉 / 移动原文件也不影响连接；没有托管内容时回退到手填路径（兼容旧数据）
  let keyPath = cfg?.keyPath || ''
  if (authType === 'key' && cfg?.keyContent) {
    try {
      keyPath = await invoke<string>('materialize_key', { content: cfg.keyContent })
    } catch (e: any) {
      handleApiError(e, 'components/TerminalWorkspace.materializeKey', { prefix: '私钥准备失败' })
      if (!tabId) termStore.closeTab(targetTabId)
      return false
    }
  }
  const passphrase = authType === 'key' ? (cfg?.passphrase || null) : null

  // 安全设置「校验密钥权限」：密钥认证连接前先校验文件存在/权限（失败则拒绝连接）
  if (authType === 'key' && settingsStore.settings.verifyKeyPermission && keyPath) {
    try {
      await invoke('check_key_file', { path: keyPath })
    } catch (e: any) {
      handleApiError(e, 'components/TerminalWorkspace.checkKeyFile', {
        prefix: '密钥文件校验失败',
        url: `check_key_file/${keyPath}`,
      })
      if (!tabId) termStore.closeTab(targetTabId)
      return false
    }
  }

  // 密码认证：没有保存密码、或上次连接就是认证失败 —— 都先弹窗让用户确认密码。
  // 上一次认证失败时，即使会话里存有密码也不静默复用（那正是「密码不对却连上了」的来源）
  if (authType === 'password' && (!password || authFailedBefore)) {
    const input = await promptSshPassword(target, authFailedBefore ? t('terminal.ssh.authFailedWarn') : undefined)
    if (!input) {
      if (!tabId) termStore.closeTab(targetTabId)
      return false
    }
    password = input.password
    if (input.remember) persistHostPassword(host, password)
  }

  await nextTick()
  let term: any
  try {
    term = await waitTermRef(targetTabId)
  } catch {
    return false
  }

  const proxy = await resolveProxy()
  // 用终端实际尺寸请求 PTY：否则远端 readline 按默认 80 列建模，长命令翻历史会重绘错位
  const dim = term.dimensions?.() ?? { cols: 80, rows: 24 }
  // 认证失败最多让用户重输几次，避免无限弹窗
  const MAX_AUTH_RETRY = 3
  let authTries = 0

  while (true) {
    term.writeln(`\x1b[36m${t('terminal.ssh.connecting', { target })}\x1b[0m`)
    term.focus()
    try {
      const session = await invoke<{ id: string }>('connect_ssh', {
        dto: {
          host: hostname,
          port,
          username,
          auth_type: authType === 'key' ? 'private_key' : 'password',
          password: authType === 'password' ? password : null,
          private_key_path: authType === 'key' ? keyPath : null,
          passphrase,
          cols: dim.cols,
          rows: dim.rows,
          // 连接超时/保活设置项 → 后端会话参数
          connect_timeout_secs: settingsStore.settings.sshConnectTimeout,
          keep_alive_secs: settingsStore.settings.sshKeepAlive,
          // 网络代理（「通用-网络代理」设置驱动；null = 直连）
          proxy_addr: proxy.addr,
          proxy_kind: proxy.kind
        }
      }, { timeoutMs: Math.max(settingsStore.settings.sshConnectTimeout + 5, 20) * 1000 })
      sessionTabMap.set(session.id, targetTabId)
      termStore.updateTab(targetTabId, { sessionId: session.id, connected: true, authFailed: false })
      retryCount.delete(targetTabId)
      addKnownHost(host.addr)
      // 重放缓存的输出
      const cached = pendingOutputs.get(session.id)
      if (cached && cached.length > 0) {
        for (const d of cached) term.write(d)
        pendingOutputs.delete(session.id)
      }
      term.focus()
      termStore.removeConnecting(targetTabId)
      return true
    } catch (e: any) {
      const raw = e?.message || String(e)
      // 密码错误：直接弹窗让用户重新输入并重试（而不是只提示一句「按 R 重连」）
      if (authType === 'password' && isAuthFailure(raw) && authTries < MAX_AUTH_RETRY) {
        authTries++
        const input = await promptSshPassword(target, t('terminal.ssh.authFailedWarn'))
        if (!input) {
          term.writeln(`\x1b[31m${t('terminal.ssh.connectFailed', { msg: friendlyText(raw) })}\x1b[0m`)
          term.writeln(`\x1b[36m${t('terminal.ssh.pressReconnect')}\x1b[0m`)
          termStore.updateTab(targetTabId, { connected: false, authFailed: true })
          termStore.removeConnecting(targetTabId)
          return false
        }
        password = input.password
        if (input.remember) persistHostPassword(host, password)
        continue
      }
      const failedByAuth = isAuthFailure(raw)
      const msg = failedByAuth && authTries >= MAX_AUTH_RETRY
        ? t('terminal.ssh.authRetryExceeded')
        : friendlyText(raw)
      term.writeln(`\x1b[31m${t('terminal.ssh.connectFailed', { msg })}\x1b[0m`)
      term.writeln(`\x1b[36m${t('terminal.ssh.pressReconnect')}\x1b[0m`)
      termStore.updateTab(targetTabId, { connected: false, authFailed: failedByAuth })
      termStore.removeConnecting(targetTabId)
      return false
    }
  }
}

// ===== Tab 关闭 =====
function onTabRemove(tabId: string): void {
  dismissAgentOverlay(tabId)
  const tab = termStore.tabs.find(t => t.id === tabId)
  if (tab?.sessionId) {
    sessionTabMap.delete(tab.sessionId)
    invoke('close_session', { sessionId: tab.sessionId }).catch(() => {})
  }
  termStore.closeTab(tabId)
  if (termStore.tabs.length === 0 && windowLabel() !== 'main') closeCurrentWindow()
}

function onReorder(id: string, toIndex: number): void {
  termStore.reorderTab(id, toIndex)
}

// ===== Tab 右键菜单动作：重连/复制会话/复制地址/批量关闭 =====
function onTabMenuAction(action: 'reconnect' | 'duplicate' | 'copyAddress' | 'close' | 'closeLeft' | 'closeRight' | 'closeOthers' | 'closeAll', tabId: string): void {
  if (action === 'reconnect') { reconnectTab(tabId); return }
  if (action === 'duplicate') { duplicateTab(tabId); return }
  if (action === 'copyAddress') { copyAddress(tabId); return }
  if (action === 'close') { onTabRemove(tabId); return }
  const idx = termStore.tabs.findIndex(t => t.id === tabId)
  const targets = termStore.tabs.filter((t, i) => {
    switch (action) {
      case 'closeLeft': return i < idx
      case 'closeRight': return i > idx
      case 'closeOthers': return i !== idx
      case 'closeAll': return true
      default: return false
    }
  })
  for (const t of targets) onTabRemove(t.id)
}

/** 解析 Tab 对应的会话条目。优先按打开时记录的会话 ID —— 同一个地址可能对应多个
 *  会话（密码/密钥各不相同），按地址匹配会串用别人的凭据，导致「密码不对却连上了」。 */
function resolveHostForTab(tab: TerminalTab): HostItem {
  return (tab.hostId ? hostStore.hosts.find(h => h.id === tab.hostId) : undefined)
    ?? hostStore.hosts.find(h => h.addr === tab.host)
    ?? ({
      id: '', name: tab.hostname || tab.title, addr: tab.host || '', namespaceId: '',
      parentId: null, type: 'host', sortOrder: 0,
      config: { username: tab.username, authType: 'password' }
    } as HostItem)
}

/** 重新连接：在原 Tab 内重建会话（本地：新起本地 PTY；SSH：按主机配置重连） */
async function reconnectTab(tabId: string): Promise<boolean> {
  const tab = termStore.tabs.find(t => t.id === tabId)
  if (!tab || tab.connected) return !!tab?.connected
  if (reconnecting.has(tabId)) return false
  reconnecting.add(tabId)
  try {
    // 清理旧会话残留
    if (tab.sessionId) {
      sessionTabMap.delete(tab.sessionId)
      invoke('close_session', { sessionId: tab.sessionId }).catch(() => {})
    }
    termStore.updateTab(tabId, { sessionId: undefined, connected: false })
    termStore.setActive(tabId)
    if (tab.kind === 'local') {
      return await openLocalTerminal(tabId)
    }
    return await openHost(resolveHostForTab(tab), tabId)
  } finally {
    reconnecting.delete(tabId)
  }
}

/** 复制：本地新开一个本地终端；SSH 重新连接同一主机（新 Tab） */
function duplicateTab(tabId: string): void {
  const tab = termStore.tabs.find(t => t.id === tabId)
  if (!tab) return
  if (tab.kind === 'local') { openLocalTerminal(); return }
  openHost(resolveHostForTab(tab))
}

/** 复制会话地址：把会话连接地址（形如 user@host:port）写入剪贴板 */
async function copyAddress(tabId: string): Promise<void> {
  const tab = termStore.tabs.find(t => t.id === tabId)
  const addr = tab?.host?.trim() || ''
  if (!addr) { ElMessage.warning('该会话没有可复制的地址'); return }
  try {
    await copyText(addr)
    ElMessage.success(`已复制地址：${addr}`)
  } catch {
    ElMessage.error('复制地址失败')
  }
}

/** 关闭当前激活 Tab（快捷键用） */
function closeActiveTab(): void {
  if (termStore.activeTabId) onTabRemove(termStore.activeTabId)
}

/** 循环切换激活 Tab（快捷键用；dir=1 下一个，dir=-1 上一个） */
function cycleTab(dir: 1 | -1): void {
  const tabs = termStore.tabs
  if (tabs.length === 0) return
  const idx = tabs.findIndex(t => t.id === termStore.activeTabId)
  const next = tabs[(idx + dir + tabs.length) % tabs.length]
  termStore.setActive(next.id)
}

// ===== 分屏模式切换（不足补本地终端：h/v≥2，grid≥4） =====
async function onSplitMode(mode: SplitMode): Promise<void> {
  if (mode === termStore.splitMode) return
  if (mode === 'none') {
    termStore.setSplitMode('none')
    return
  }
  const required = mode === 'grid' ? 4 : 2
  termStore.setSplitMode(mode)
  if (CAP.localTerminal()) {
    const need = Math.max(0, required - termStore.tabs.length)
    for (let i = 0; i < need; i++) {
      openLocalTerminal().catch(() => {})
    }
  }
}

// ===== Tab 拖出：跨窗口迁移 / 新开独立窗口 =====
async function onTabDetach(tab: TerminalTab, clientX: number, clientY: number): Promise<void> {
  // 移动端不支持独立窗口，禁用例拖出
  if (!CAP.tabDrag()) return
  // 先序列化缓冲区并暂存（接收窗口/新窗口通过 take_term_buffer 还原）
  const term = paneRefs.get(tab.id)?.term as any
  try {
    const data: string = term?.serialize ? term.serialize() : ''
    if (data) await invoke('stash_term_buffer', { tabId: tab.id, data })
  } catch { /* 序列化失败不阻塞迁移 */ }

  const meta: TransferredTab = {
    id: tab.id,
    title: tab.title,
    host: tab.host,
    username: tab.username,
    hostname: tab.hostname,
    sessionId: tab.sessionId,
    kind: tab.kind,
    connected: tab.connected
  }

  let accepted = false
  try {
    accepted = await requestTabTransfer(meta, clientX, clientY)
  } catch { /* 无其他窗口时走新开 */ }

  if (!accepted) {
    // 独立窗口仅剩这一个 Tab：直接把本窗口移动到落点（窗口即 Tab 容器，跟随拖拽，
    // 避免"关旧窗开新窗"的闪烁；也规避同一 Tab 反复拖出时的 label 冲突）
    if (windowLabel() !== 'main' && termStore.tabs.length === 1) {
      try {
        const win = getCurrentWindow()
        const pos = await win.outerPosition()
        const scale = await win.scaleFactor()
        await win.setPosition(new PhysicalPosition(
          Math.round(pos.x + clientX * scale) - 480,
          Math.round(pos.y + clientY * scale) - 20
        ))
        await win.setFocus()
      } catch { /* 移动失败则留在原位 */ }
      return
    }
    const q = new URLSearchParams({
      take: tab.id,
      title: tab.title,
      kind: tab.kind,
      connected: tab.connected ? '1' : '0',
      host: tab.host || '',
      username: tab.username || '',
      hostname: tab.hostname || '',
      sid: tab.sessionId || ''
    })
    // 新窗口出现在松开鼠标的位置（屏幕物理坐标，鼠标大致位于窗口顶部中央）
    let winX: number | undefined
    let winY: number | undefined
    try {
      const win = getCurrentWindow()
      const pos = await win.outerPosition()
      const scale = await win.scaleFactor()
      winX = Math.round(pos.x + clientX * scale) - 480
      winY = Math.round(pos.y + clientY * scale) - 20
    } catch { /* 取窗口位置失败则由系统默认摆放 */ }
    try {
      await invoke('open_terminal_window', {
        // label 必须全局唯一：同一 Tab 从多 Tab 窗口再次拖出时，'term-<tabId>' 可能已被占用
        label: 'term-' + tab.id + '-' + Math.random().toString(36).slice(2, 8),
        title: tab.title,
        url: '#/terminal-window?' + q.toString(),
        x: winX,
        y: winY
      })
    } catch (e) {
      console.error('[workspace] open_terminal_window failed:', e)
      handleApiError(e, 'components/TerminalWorkspace.openTerminalWindow', {
        prefix: '打开独立窗口失败',
      })
      return // 保留 Tab
    }
  }
  finishDetach(tab.id)
}

/** 源窗口移除已迁出的 Tab（后端会话全局共享，不中断） */
function finishDetach(tabId: string): void {
  const tab = termStore.tabs.find(t => t.id === tabId)
  if (tab?.sessionId) sessionTabMap.delete(tab.sessionId)
  termStore.detachTab(tabId)
  if (termStore.tabs.length === 0 && windowLabel() !== 'main') closeCurrentWindow()
}

/** 接管迁移来的 Tab：还原缓冲区并建立输出路由 */
async function acceptTransferredTab(t: TransferredTab): Promise<void> {
  termStore.adoptTab({
    id: t.id,
    title: t.title,
    host: t.host,
    username: t.username,
    hostname: t.hostname,
    sessionId: t.sessionId,
    connected: t.connected,
    kind: t.kind
  })
  if (t.sessionId) sessionTabMap.set(t.sessionId, t.id)
  await nextTick()
  try {
    const term = await waitTermRef(t.id)
    const data = await invoke<string | null>('take_term_buffer', { tabId: t.id })
    if (data) term.restore(data)
    term.resize?.()
    term.focus()
  } catch { /* 缓冲区还原失败不影响会话 */ }
}

// ===== 事件监听 =====
let unlistenOutput: UnlistenFn | null = null
let unlistenStatus: UnlistenFn | null = null
let unlistenOsDrop: UnlistenFn | null = null
let unlistenResize: UnlistenFn | null = null
/** 无法路由的输出缓存（竞态条件保护：sessionTabMap 尚未建立时缓存输出，映射建立后重放） */
const pendingOutputs = new Map<string, string[]>()
/** 窗口拖拽期间暂存的终端输出（松手后由 onUiThaw 一次性重放，避免 xterm.write 阻塞窗口移动） */
const frozenOutputs = new Map<string, string[]>()
let offThawReplay: (() => void) | null = null

const CIRC = 2 * Math.PI * 11

// ===== Ctrl 批量操作（分屏同步） =====
// 开启方式由「设置 → 通用 → 如何开启多屏协同」决定：
//  - doubleCtrl（默认）：连按两次 Ctrl 切换常驻批量模式，单次按下不生效
//  - longCtrl：按住 Ctrl 达到阈值才临时生效，松开即关（避开 Ctrl+C/V 等组合键）
//  - alwaysOn：分屏下永久开启
//  - alwaysOff：永久关闭
// 双击窗口放宽到 700ms：人手指的"双击 Ctrl"往往超过 500ms，过短会导致常驻模式几乎触发不了。
const DOUBLE_CTRL_MS = 700
/** 长按 Ctrl 判定阈值：短于此视为普通 Ctrl 组合键，不进入批量模式 */
const LONG_CTRL_MS = 500
let lastCtrlDownAt = 0
let longCtrlTimer: number | null = null

function clearLongCtrlTimer() {
  if (longCtrlTimer !== null) { window.clearTimeout(longCtrlTimer); longCtrlTimer = null }
}

/** 按当前设置与分屏状态重算批量模式（切换设置 / 切换分屏 / 窗口重新聚焦时调用） */
function applySyncMode() {
  const mode = settingsStore.settings.multiSyncTrigger
  if (termStore.splitMode === 'none' || mode === 'alwaysOff') {
    clearLongCtrlTimer()
    termSync.active = false
    termSync.sticky = false
    return
  }
  if (mode === 'alwaysOn') {
    clearLongCtrlTimer()
    termSync.active = true
    termSync.sticky = true
    return
  }
  // 常驻(sticky)是"连按两次 Ctrl"专属状态：切到其它方式时清掉，
  // 否则从双击切到长按后，遗留的常驻标记会让协同一直处于开启
  if (mode !== 'doubleCtrl') termSync.sticky = false
  termSync.active = termSync.sticky
}

function onSyncKey(e: KeyboardEvent) {
  if (e.key !== 'Control' || e.repeat) return
  const mode = settingsStore.settings.multiSyncTrigger
  // 常开/常关直接由 applySyncMode 决定，按键不改变状态
  if (termStore.splitMode === 'none' || mode === 'alwaysOff' || mode === 'alwaysOn') {
    applySyncMode()
    return
  }
  if (e.type === 'keydown') {
    if (mode === 'longCtrl') {
      // 长按：按住达到阈值才生效，避免与 Ctrl+C / Ctrl+V 等组合键冲突
      clearLongCtrlTimer()
      longCtrlTimer = window.setTimeout(() => {
        longCtrlTimer = null
        termSync.active = true
      }, LONG_CTRL_MS)
      return
    }
    // 连按两次 Ctrl：切换常驻批量模式
    const now = Date.now()
    if (lastCtrlDownAt > 0 && now - lastCtrlDownAt < DOUBLE_CTRL_MS) {
      termSync.sticky = !termSync.sticky
      lastCtrlDownAt = 0
    } else {
      lastCtrlDownAt = now
    }
    termSync.active = termSync.sticky
  } else {
    if (mode === 'longCtrl') {
      clearLongCtrlTimer()
      termSync.active = termSync.sticky
      return
    }
    // 松开 Ctrl：此处不重置 lastCtrlDownAt，否则「按下1→松开→按下2」之间计时器被清零，
    // 双击切换常驻模式永远无法触发。
    termSync.active = termSync.sticky
  }
}
function onSyncBlur() { termSync.active = false; termSync.sticky = false }
/** 重新聚焦：常驻模式需要立刻恢复（失焦时已被清掉） */
function onSyncFocus() { applySyncMode() }
// 切换开启方式或分屏状态后立即重算，保证「永久开启 / 永久关闭」当次生效
watch(
  [() => settingsStore.settings.multiSyncTrigger, () => termStore.splitMode],
  () => applySyncMode(),
)

/** 批量模式指示器文案：随开启方式变化，避免「永久开启」时还提示"双击 Ctrl 关闭" */
const syncBadgeText = computed(() => {
  const mode = settingsStore.settings.multiSyncTrigger
  if (mode === 'alwaysOn') return '批量模式（常驻）'
  if (mode === 'longCtrl') return 'Ctrl 批量模式'
  return termSync.sticky ? '批量模式（常驻，双击 Ctrl 关闭）' : 'Ctrl 批量模式'
})

/** 注销输入广播转发 */
let offSyncInput: (() => void) | null = null
/** 注销 fs 状态广播转发 */
let offSyncFs: (() => void) | null = null
/** 注销独立终端窗口的关闭拦截 */
let unlistenCloseReq: UnlistenFn | null = null

onMounted(async () => {
  initDragHoverClick()
  initWindowRegistry(tab => { acceptTransferredTab(tab) })

  // 全屏：F11 快捷键（捕获阶段）+ 状态同步（系统级退出全屏时经 resize 事件回同步）
  window.addEventListener('keydown', onFullscreenKey, true)
  // AI Agent 模式：Ctrl+Shift+I 快捷键（捕获阶段）
  window.addEventListener('keydown', onAgentKey, true)
  // ESC：退出全屏 / 退出精简模式（捕获阶段）
  window.addEventListener('keydown', onEscapeKey, true)
  if (isTauriEnv()) {
    syncFullscreenState()
    unlistenResize = await getCurrentWindow().onResized(() => { syncFullscreenState() })
    // 独立终端窗口（term-*）：点 X 关闭前先清理本窗口全部后端会话（会话为进程级全局共享，不清理会残留）
    if (windowLabel() !== 'main') {
      unlistenCloseReq = await getCurrentWindow().onCloseRequested(async (event) => {
        event.preventDefault()
        const closing = termStore.tabs
          .filter(t => t.sessionId)
          .map(t => invoke('close_session', { sessionId: t.sessionId }).catch(() => {}))
        await Promise.race([Promise.allSettled(closing), new Promise(r => setTimeout(r, 1500))])
        await getCurrentWindow().destroy().catch(() => {})
      })
    }
  } else {
    document.addEventListener('fullscreenchange', onWebFullscreenChange)
  }

  // Ctrl 批量模式：按键状态跟踪（捕获阶段，避免被消费/拦截）+ 输入广播转发到本窗口其他会话
  window.addEventListener('keydown', onSyncKey, true)
  window.addEventListener('keyup', onSyncKey, true)
  window.addEventListener('blur', onSyncBlur)
  window.addEventListener('focus', onSyncFocus)
  offSyncInput = onSyncInput((data, sourceSessionId) => {
    for (const t of termStore.tabs) {
      if (t.sessionId && t.sessionId !== sourceSessionId) {
        invoke('send_session_input', { dto: { session_id: t.sessionId, data } }).catch(() => { /* ignore */ })
      }
    }
  })
  // 文件系统折叠/尺寸：批量模式下把源分屏状态同步到其它分屏
  offSyncFs = onSyncFs((data: FsSyncPayload, sourceSessionId: string) => {
    for (const t of termStore.tabs) {
      if (t.sessionId && t.sessionId !== sourceSessionId) {
        paneRefs.get(t.id)?.applyFsSync(data)
      }
    }
  })

  if (isTauriEnv()) {
    // SSH/本地 PTY 输出 → 路由到对应终端；映射未建立时缓存，建立后重放
    unlistenOutput = await listen<{ session_id: string; data: string }>('ssh-output', ev => {
      const { session_id, data } = ev.payload
      // 窗口拖拽中：先暂存，松手后由 onUiThaw 一次性重放。
      // 直接 write 会在每次 WebView2 重绘（WM_PAINT）时触发 xterm canvas 重画，
      // 与主线程窗口移动消息争抢 CPU，导致拖拽掉帧。
      if (isUiFrozen()) {
        const buf = frozenOutputs.get(session_id) || []
        buf.push(data)
        frozenOutputs.set(session_id, buf)
        return
      }
      const tabId = sessionTabMap.get(session_id)
      if (tabId) {
        paneRefs.get(tabId)?.term?.write(data)
      } else {
        const pending = pendingOutputs.get(session_id) || []
        pending.push(data)
        pendingOutputs.set(session_id, pending)
      }
    })
    // 解冻回调：一次性把拖拽期间累积的输出合并到 xterm（单次 write 整段，内部批量渲染）
    offThawReplay = onUiThaw(() => {
      if (frozenOutputs.size === 0) return
      for (const [sessionId, chunks] of frozenOutputs) {
        const tabId = sessionTabMap.get(sessionId)
        if (!tabId) {
          // 会话尚未在本窗口建立映射：合并到 pendingOutputs（由 ssh-status connected 重放）
          const pending = pendingOutputs.get(sessionId) || []
          pending.push(...chunks)
          pendingOutputs.set(sessionId, pending)
          continue
        }
        const termEl = paneRefs.get(tabId)?.term
        if (termEl) termEl.write(chunks.join(''))
      }
      frozenOutputs.clear()
    })
    unlistenStatus = await listen<{ session_id: string; event: string; message?: string }>('ssh-status', ev => {
      const p = ev.payload
      const tabId = sessionTabMap.get(p.session_id)
      if (!tabId) {
        // 尚未建立映射：如果是 connected 事件，缓存映射建立后重放
        if (p.event === 'connected') {
          const cached = pendingOutputs.get(p.session_id) || []
          if (cached.length > 0) {
            // 找到未映射的会话，等待 openLocalTerminal/openHost 建立映射
            console.debug('[term] ssh-status connected 未映射，等待重放', p.session_id, `缓存${cached.length}条`)
          }
        }
        return
      }
      if (p.event === 'connected') {
        termStore.updateTab(tabId, { connected: true })
        // 重放缓存的输出
        const cached = pendingOutputs.get(p.session_id)
        if (cached && cached.length > 0) {
          const termEl = paneRefs.get(tabId)?.term
          if (termEl) {
            for (const d of cached) termEl.write(d)
          }
          pendingOutputs.delete(p.session_id)
        }
      } else if (p.event === 'disconnected') {
        termStore.updateTab(tabId, { connected: false })
        paneRefs.get(tabId)?.term?.writeln('\r\n\x1b[33m[会话已断开]\x1b[0m')
        paneRefs.get(tabId)?.term?.writeln('\x1b[36m按 R 键重新连接\x1b[0m')
        maybeAutoReconnect(tabId)
      } else if (p.event === 'error') {
        termStore.updateTab(tabId, { connected: false })
        paneRefs.get(tabId)?.term?.writeln(`\r\n\x1b[31m[错误] ${p.message || ''}\x1b[0m`)
      }
    })
    // 本地 OS 文件拖入：按坐标命中放置区；over/leave 阶段驱动拖拽悬停点击（按钮/文件夹）
    try {
      // 预先取缩放因子：over 高频事件不再异步取（无拖拽时也无开销）
      let osDragScale = await getCurrentWindow().scaleFactor()
      unlistenOsDrop = await getCurrentWebview().onDragDropEvent(async ev => {
        const p = ev.payload as { type: string; paths?: string[]; position?: { x: number; y: number } }
        // 悬停点击：拖拽经过时按坐标命中 [data-hover-click] 元素（文件夹/按钮）
        if (p.type === 'over') {
          if (!p.position) return
          const x = p.position.x / osDragScale
          const y = p.position.y / osDragScale
          osDragHoverAt(x, y)
          return
        }
        if (p.type === 'leave') {
          osDragHoverClear()
          return
        }
        if (p.type !== 'drop' || !p.paths || p.paths.length === 0) return
        osDragHoverClear()
        // 「允许拖拽上传」开关关闭时不响应本地文件拖入
        if (!settingsStore.settings.allowDragUpload) {
          ElMessage.info('拖拽上传已关闭，可在 设置-终端 中开启')
          return
        }
        const scale = await getCurrentWindow().scaleFactor()
        osDragScale = scale
        const x = (p.position?.x ?? 0) / scale
        const y = (p.position?.y ?? 0) / scale
        const zone = findOsDropZoneAt(x, y)
        if (!zone) return
        try {
          const result = await handleOsFilesToZone(p.paths, zone, x, y)
          if (result.ok.length > 0) {
            ElMessage.success(result.ok.length === 1 ? `文件已传输: ${result.ok[0]}` : `${result.ok.length} 个文件已传输`)
          }
          if (result.failed.length > 0) {
            // f.error 来自 Rust 层，先用 friendlyText 收敛，绝不裸展示技术细节
            ElMessage.error(
              result.failed
                .map(f => `${f.path.split(/[\\/]/).pop()}: ${friendlyText(f.error)}`)
                .join('；')
            )
          }
        } catch (e: any) {
          handleApiError(e, 'components/TerminalWorkspace.dragDropTransfer', {
            prefix: '文件传输失败',
          })
        }
      })
    } catch (e) {
      console.warn('[workspace] onDragDropEvent unavailable:', e)
    }
  }

  // 初始 Tab：接管模式 → 还原迁入 Tab；否则打开本地终端
  if (props.takeTab) {
    acceptTransferredTab(props.takeTab)
  } else if (termStore.tabs.length === 0) {
    if (CAP.localTerminal()) {
      openLocalTerminal()
    }
  }
})

onBeforeUnmount(() => {
  for (const t of spinnerTimers.values()) clearInterval(t)
  spinnerTimers.clear()
  for (const d of overlayDisposers.values()) { try { d() } catch { /* 忽略 */ } }
  overlayDisposers.clear()
  unlistenOutput?.()
  unlistenStatus?.()
  unlistenOsDrop?.()
  unlistenResize?.()
  unlistenCloseReq?.()
  offThawReplay?.()
  frozenOutputs.clear()
  window.removeEventListener('keydown', onFullscreenKey, true)
  window.removeEventListener('keydown', onAgentKey, true)
  window.removeEventListener('keydown', onEscapeKey, true)
  document.removeEventListener('fullscreenchange', onWebFullscreenChange)
  window.removeEventListener('keydown', onSyncKey, true)
  window.removeEventListener('keyup', onSyncKey, true)
  window.removeEventListener('blur', onSyncBlur)
  window.removeEventListener('focus', onSyncFocus)
  offSyncInput?.()
  offSyncFs?.()
  termSync.active = false
  termSync.sticky = false
})

/** 清除当前激活终端屏幕 */
function clearActiveTerminal() {
  const tabId = termStore.activeTabId
  if (tabId) paneRefs.get(tabId)?.term?.clear()
}

/** 导出当前激活终端日志（序列化 scrollback → 写入文件） */
async function exportActiveTerminalLog() {
  const tabId = termStore.activeTabId
  const tab = termStore.tabs.find(t => t.id === tabId)
  if (!tabId || !tab) return
  const data = paneRefs.get(tabId)?.term?.serialize?.() ?? ''
  if (!data) { ElMessage.warning('终端内容为空'); return }
  const ts = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19)
  const name = `${tab.title || 'terminal'}-${ts}.log`
  try {
    const path = await invoke<string | null>('pick_save_path', { title: '导出终端日志', defaultName: name }).catch(() => null)
    if (!path) return
    await invoke('save_text_file', { path, content: data })
    ElMessage.success(`已导出到 ${path}`)
  } catch (e: any) {
    handleApiError(e, 'components/TerminalWorkspace.exportLog', { prefix: '导出失败' })
  }
}

/** 重连当前激活终端 */
async function reconnectActive() {
  const tabId = termStore.activeTabId
  if (tabId) await reconnectTab(tabId)
}

/** 复制选区到剪贴板 */
function copyActiveSelection() {
  const tabId = termStore.activeTabId
  if (tabId) paneRefs.get(tabId)?.term?.copySelection?.()
}

/** 粘贴剪贴板到终端 */
function pasteToActive() {
  const tabId = termStore.activeTabId
  if (tabId) paneRefs.get(tabId)?.term?.paste?.()
}

/** 全选 */
function selectAllActive() {
  const tabId = termStore.activeTabId
  if (tabId) paneRefs.get(tabId)?.term?.selectAll?.()
}

/** 字号缩放 */
function zoomInActive() { paneRefs.get(termStore.activeTabId ?? '')?.term?.zoomIn?.() }
function zoomOutActive() { paneRefs.get(termStore.activeTabId ?? '')?.term?.zoomOut?.() }
function zoomResetActive() { paneRefs.get(termStore.activeTabId ?? '')?.term?.zoomReset?.() }

// ===== 终端空白区右键菜单：复用通用两级菜单组件 =====
/** 当前右键命中的 Tab 与菜单位置 */
const ctxTabId = ref('')
const ctxMenu = reactive({ visible: false, x: 0, y: 0 })

/** 菜单项（含子菜单：分屏 / 字号） */
const sessionMenuItems = computed<FsMenuItem[]>(() => [
  ...(CAP.localTerminal() ? [{ key: 'newLocal', label: t('context.session.newLocal'), icon: Plus } as FsMenuItem] : []),
  { key: 'duplicate', label: t('context.session.duplicate'), icon: CopyDocument },
  { key: 'reconnect', label: t('context.session.reconnect'), icon: RefreshRight },
  { key: 'copyAddress', label: t('context.session.copyAddress'), icon: Link },
  {
    key: 'split', label: t('context.session.split'), icon: Operation, children: [
      { key: 'splitH', label: t('context.session.splitH') },
      { key: 'splitV', label: t('context.session.splitV') },
      { key: 'splitGrid', label: t('context.session.splitGrid') },
      { key: 'splitNone', label: t('context.session.splitNone') },
    ]
  },
  { key: 'clear', label: t('context.session.clear'), icon: Delete },
  { key: 'exportLog', label: t('context.session.exportLog'), icon: DocumentCopy },
  { key: 'copy', label: t('context.session.copy'), icon: DocumentCopy },
  { key: 'paste', label: t('context.session.paste') },
  { key: 'selectAll', label: t('context.session.selectAll') },
  {
    key: 'zoom', label: t('context.session.zoom'), icon: ZoomIn, children: [
      { key: 'zoomIn', label: t('context.session.zoomIn') },
      { key: 'zoomOut', label: t('context.session.zoomOut') },
      { key: 'zoomReset', label: t('context.session.zoomReset') },
    ]
  },
  { key: 'close', label: t('context.session.close'), icon: Close, danger: true },
])

/** 收到分屏单元抛来的右键事件：记录命中 Tab 与坐标，弹出菜单 */
function onTermContextMenu(payload: { x: number; y: number; tabId: string }): void {
  ctxTabId.value = payload.tabId
  ctxMenu.x = payload.x
  ctxMenu.y = payload.y
  ctxMenu.visible = true
}

/** 菜单项点击：先把命中 Tab 设为激活（复制/粘贴/清除/字号等动作都作用于激活 Tab） */
function onSessionMenuSelect(key: string): void {
  const tabId = ctxTabId.value
  if (!tabId) return
  termStore.setActive(tabId)
  switch (key) {
    case 'newLocal': openLocalTerminal(); break
    case 'duplicate': duplicateTab(tabId); break
    case 'reconnect': reconnectTab(tabId); break
    case 'copyAddress': copyAddress(tabId); break
    case 'splitH': onSplitMode('h'); break
    case 'splitV': onSplitMode('v'); break
    case 'splitGrid': onSplitMode('grid'); break
    case 'splitNone': onSplitMode('none'); break
    case 'clear': clearActiveTerminal(); break
    case 'exportLog': exportActiveTerminalLog(); break
    case 'copy': copyActiveSelection(); break
    case 'paste': pasteToActive(); break
    case 'selectAll': selectAllActive(); break
    case 'zoomIn': zoomInActive(); break
    case 'zoomOut': zoomOutActive(); break
    case 'zoomReset': zoomResetActive(); break
    case 'close': onTabRemove(tabId); break
  }
}

defineExpose({ openHost, openLocalTerminal, acceptTransferredTab, closeActiveTab, cycleTab, setSplitMode: onSplitMode, clearActiveTerminal, exportActiveTerminalLog, reconnectActive, copyActiveSelection, pasteToActive, selectAllActive, zoomInActive, zoomOutActive, zoomResetActive })
</script>
