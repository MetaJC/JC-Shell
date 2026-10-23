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
    ref="containerRef"
    class="terminal-container"
    :style="{ backgroundColor: app.terminalBgColor, opacity: settings.settings.terminalOpacity / 100 }"
    @contextmenu="onContextMenu"
  ></div>
</template>

<script setup lang="ts">
/**
 * 终端视图（xterm.js）
 * 作者：JC
 * 统一 sessionId 驱动：输入转发后端 send_session_input，输出由父组件经
 * ssh-output 事件写入（本地 PTY 与 SSH 复用同一事件链路）。
 * Agent 模式：输入本地缓冲 + 手工回显；回车时区分命令与自然语言——
 * 命令直接移交真实 shell 执行（Tab 补全/历史/编辑全部原生），自然语言才发给大模型。
 */
import { onBeforeUnmount, onMounted, ref, watch, computed } from 'vue'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { SerializeAddon } from '@xterm/addon-serialize'
import { WebglAddon } from '@xterm/addon-webgl'
import { CAP, isMobileWebview } from '@/utils/platform'
import { KB_TOGGLE_EVENT } from '@/utils/mobileKeyboard'
import { ElMessage } from 'element-plus'
import { invoke, isTauriEnv } from '@/api'
import { emitSyncInput } from '@/composables/useTermSync'
import { copyText, readClipboardText } from '@/utils/clipboard'
import { useAppStore } from '@/stores/app'
import { useSettingsStore } from '@/stores/settings'
import { useAgentStore, AGENT_GHOST_HINT, type AgentAction } from '@/stores/agent'
import {
  classifyLine, isConfidentCommandLine, isForegroundCommand, stripForcePrefix
} from '@/utils/shellInput'
import { DownloadStallGuard } from '@/utils/downloadStallGuard'
import { effFontSize } from '@/utils/terminalFont'
import { PromptHighlighter, boldPrompt } from '@/utils/promptHighlight'
import { setTermBottomOccupied, clearTermBottomOccupied } from './termBottomStatus'

interface Props {
  tabId: string
  /** 后端会话 ID（本地 PTY 或 SSH，均走 send_session_input） */
  sessionId?: string
  /** 会话类型：本地 PTY 输出自带 \r\n，不做 EOL 转换 */
  kind?: 'local' | 'ssh'
  /** 会话是否已连接；断连时拦截按键（R 键触发重连） */
  connected?: boolean
  /** AI Agent 模式：开启时键盘输入不进 shell，回车提交自然语言指令 */
  agentMode?: boolean
}

const props = withDefaults(defineProps<Props>(), { sessionId: '', kind: 'ssh', connected: false, agentMode: false })

const emit = defineEmits<{
  (e: 'reconnect'): void
  /** Agent 模式下回车提交的自然语言指令 */
  (e: 'agent-submit', prompt: string): void
  /** Agent 终端内确认/控制动作（执行/修改/拒绝/中止） */
  (e: 'agent-action', action: AgentAction): void
  /** 输入行被识别为命令并直接交给 shell 执行：工作区据此刷新仿真 prompt（cd 会改目录） */
  (e: 'shell-command'): void
  /** 空白区右键（无选区）：把光标坐标抛给上层弹「会话右键菜单」 */
  (e: 'termContextMenu', payload: { x: number; y: number }): void
}>()

const containerRef = ref<HTMLDivElement>()
let term: Terminal | null = null
let fitAddon: FitAddon | null = null
let serializeAddon: SerializeAddon | null = null
let webglAddon: WebglAddon | null = null
let resizeObserver: ResizeObserver | null = null

/** 是否移动端 WebView：移动端有软键盘避让/贴底等专属行为（见 resize、onKbToggle） */
const isMobile = isMobileWebview()

const app = useAppStore()
const settings = useSettingsStore()
const agentStore = useAgentStore()

/** Ctrl+滚轮调字号的范围（换算函数见 utils/terminalFont.ts） */
const FONT_MIN = 8
const FONT_MAX = 32

/** 根据当前主题获取终端颜色配置（背景色取用户自定义，其次主题默认）。
 *  完整提供 ANSI 0-15 全部 16 色（含 bright 系列）——只设 brightBlack 会让
 *  brightRed/Green/... 落到 xterm 默认暗色，导致高亮/加粗输出昏暗难读；
 *  这里统一提升饱和度与亮度，避免阅读昏沉。 */
function getTermThemeColors() {
  const theme = app.effectiveTheme
  const background = app.terminalBgColor
  // 字体色：用户自定义优先，否则按主题取默认（与 app store 的 defaultTermFgFor 一致）
  const foreground = app.terminalFgColor
  if (theme === 'eye-care') {
    return {
      background,
      foreground,
      cursor: '#8fd07a',
      cursorAccent: background,
      selectionBackground: 'rgba(158, 206, 106, 0.35)',
      black: '#5a6a52', red: '#f08a76', green: '#a8d896', yellow: '#f0d488',
      blue: '#8ab4d8', magenta: '#d0a8d0', cyan: '#9cd8cf', white: '#eef5e9',
      brightBlack: '#93a38b', brightRed: '#ff9d88', brightGreen: '#c0eaac',
      brightYellow: '#ffe49c', brightBlue: '#a2c8ea', brightMagenta: '#e2bce2',
      brightCyan: '#b4eadf', brightWhite: '#f7fcf3'
    }
  }
  if (theme === 'dark') {
    return {
      background,
      foreground,
      cursor: '#6cb8ff',
      cursorAccent: background,
      selectionBackground: 'rgba(108, 184, 255, 0.30)',
      black: '#5a6478', red: '#ff6b6b', green: '#51e98f', yellow: '#ffd76d',
      blue: '#6cb8ff', magenta: '#d0a0ff', cyan: '#4ae3e3', white: '#f1f5f9',
      brightBlack: '#93a0bd', brightRed: '#ff8f8f', brightGreen: '#7dffb0',
      brightYellow: '#ffe08f', brightBlue: '#8fd0ff', brightMagenta: '#e0c0ff',
      brightCyan: '#7ff0f0', brightWhite: '#ffffff'
    }
  }
  // light theme：亮色背景用深色高饱和文字色保证对比度
  return {
    background,
    foreground,
    cursor: '#0066dd',
    cursorAccent: background,
    selectionBackground: 'rgba(0, 122, 255, 0.22)',
    black: '#3f4756', red: '#d92d20', green: '#128807', yellow: '#b54708',
    blue: '#175cd3', magenta: '#8620d8', cyan: '#0e7490', white: '#161617',
    brightBlack: '#5b6472', brightRed: '#f04438', brightGreen: '#12b76a',
    brightYellow: '#dc6803', brightBlue: '#1570ef', brightMagenta: '#a32ede',
    brightCyan: '#06aed4', brightWhite: '#000000'
  }
}

// ===== Agent 模式幽灵提示（ghost hint）=====
// 开启 Agent 模式且输入缓冲为空时，在光标处用浅灰色显示内联提示；
// 用户开始输入即消失，清空后复现。真实 shell 提示符始终保留在屏幕上。

/** 幽灵提示是否正显示在终端上 */
let ghostShown = false
/** 幽灵提示额外占用的行数（0=单行；窄屏提示折行时 >0，回位与擦除都要用它） */
let ghostRows = 0
/** shell 输出停顿时重新显示幽灵提示的防抖计时器 */
let ghostTimer: ReturnType<typeof setTimeout> | null = null
/** 切 Agent 清理 shell 半截输入时的输入封锁兜底解除计时器
 *  （正常由 scheduleGhost 在输出静默 400ms 后解除，这个只是防止 shell 完全不回显把用户永久锁住） */
let switchFallback: ReturnType<typeof setTimeout> | null = null
/** 每 Tab Agent 仿真 prompt 起始的绝对行号（baseY + cursorY），用于正确擦除多行输入 */
const agentPromptRows = new Map<string, number>()

/** 下载卡死后是否已自动换源重试过（用户手动回车后复位，避免无限自动重试） */
let autoRetriedAfterStall = false

/** 提示符高亮：给行首的 shell 提示符（如 root@host:~#）注入 ANSI 粗体，
 *  与普通输出区分开；可在「设置-终端-交互行为」关闭 */
const promptHighlighter = new PromptHighlighter()

/** 下载停滞看门狗：curl/wget 进度长时间不前进 → 自动 Ctrl+C 中断；
 *  Agent 模式下若记得最近的请求，再自动换源重试一次 */
const stallGuard = new DownloadStallGuard({
  stallMs: 20000,
  checkIntervalMs: 3000,
  onStall: () => {
    if (!props.connected || !props.sessionId || !term) return
    hideGhost()
    // 必须同时满足：Agent 模式 + 确有正在执行的下载任务，才介入；
    // 否则（手动模式 / 无任务 / 命令已结束或已被用户中断回到提示符 / 无可重试命令）
    // 一律不发送 Ctrl+C、不弹任何提示，避免无端打断用户操作。
    if (!props.agentMode) return
    if (cursorAtPrompt()) return
    const lastPrompt = agentStore.getLastPrompt(props.tabId)
    const lastCmd = agentStore.getLastCommand(props.tabId)
    if (!lastPrompt && !lastCmd) return
    // 中断卡死的命令 + 自增任务代次（仅确属活动任务时），停掉后续命令循环
    sendRawToShell('\x03')
    agentStore.bumpEpoch(props.tabId)
    const epoch = agentStore.currentEpoch(props.tabId)
    // 仅首次自动换源重试时给出提示（提示出现在重试任务执行期间）；
    // 之后静默处理，不再打扰。绝不在任务已终止时弹出任何提示。
    if (autoRetriedAfterStall) return
    autoRetriedAfterStall = true
    term.write('\r\n\x1b[33m⚠ 检测到下载长时间无进度，已自动中断，正在改用国内镜像源重试…\x1b[0m\r\n')
    const retry = lastPrompt
      ? `${lastPrompt}（上一条命令因网络访问不了境外源、长时间无进度已被中断。请改用国内镜像源重新执行，禁止再使用 raw.githubusercontent.com / github.com 等境外地址）`
      : `刚才执行的命令「${lastCmd}」因访问不了境外源、长时间无进度已被中断。请改用国内镜像源完成同样的目标，禁止再使用境外地址。`
    setTimeout(() => {
      // 期间用户若按 Ctrl+C（任务代次变化）或任务已结束 → 放弃自动重试
      if (!props.connected) return
      if (agentStore.currentEpoch(props.tabId) !== epoch) return
      if (agentStore.pending?.tabId === props.tabId) return
      emit('agent-submit', retry)
    }, 1000)
  }
})

/** 估算字符串在终端中的显示列宽（CJK 等宽字符计 2 列） */
function displayWidth(s: string): number {
  let w = 0
  for (const ch of s) {
    const cp = ch.codePointAt(0) ?? 0
    const wide =
      (cp >= 0x1100 && cp <= 0x115f) || // Hangul Jamo
      (cp >= 0x2e80 && cp <= 0xa4cf) || // CJK 部首/假名/汉字等
      (cp >= 0xac00 && cp <= 0xd7a3) || // Hangul Syllables
      (cp >= 0xf900 && cp <= 0xfaff) || // CJK 兼容汉字
      (cp >= 0xfe30 && cp <= 0xfe4f) || // CJK 兼容形式
      (cp >= 0xff00 && cp <= 0xff60) || // 全角形式
      (cp >= 0xffe0 && cp <= 0xffe6) ||
      (cp >= 0x20000 && cp <= 0x3fffd)
    w += wide ? 2 : 1
  }
  return w
}

/** 光标所在行（光标前内容）是否像「等待输入的命令提示符」。
 *  命令执行中光标停在输出行（如 curl 进度行，以数字/speed 结尾），不会命中提示符特征，
 *  从而避免任务执行期间反复写入幽灵提示（长提示还会把折行的进度条撑乱）。 */
function cursorAtPrompt(): boolean {
  if (!term) return false
  const buf = term.buffer.active
  const line = buf.getLine(buf.cursorY + buf.baseY)
  // 折行续行中（输出中间）不算提示符
  if (!line || line.isWrapped) return false
  const before = line.translateToString(true, 0, buf.cursorX)
  if (before === '') return false
  const sim = agentStore.getPrompt(props.tabId)
  if (sim && before === sim) return true
  // bash/zsh 的 `$ ` `# ` `% `、PowerShell/cmd 的 `> `
  return /[$#>%]\s?$/.test(before)
}

/** 是否满足显示幽灵提示的条件：Agent 模式 + 已连接 + 输入缓冲为空
 *  + 无进行中的 AI 请求/确认 + 非「修改」透传状态 + 光标确在提示符后 */
function ghostEligible(): boolean {
  return !!term && props.agentMode && props.connected && !!props.sessionId &&
    agentStore.getBuffer(props.tabId) === '' &&
    agentStore.pending?.tabId !== props.tabId &&
    !agentStore.isPassthrough(props.tabId) &&
    cursorAtPrompt()
}

/** 读取屏幕上光标前的提示符文本（保留原样、含尾随空格），作为 Agent 仿真 prompt 的来源。
 *  直接对齐真实 shell 提示符，避免出现 root@IP 与 root@host 两种写法。
 *  注意：xterm 的 translateToString(trimRight, start, end) 是 [start, end) 左闭右开，
 *  这里必须传 cursorX（不是 cursorX-1），否则每次都会少取末字符、导致提示符逐次变短。 */
function readPromptText(): string {
  if (!term) return ''
  const buf = term.buffer.active
  const line = buf.getLine(buf.cursorY + buf.baseY)
  if (!line || line.isWrapped) return ''
  if (buf.cursorX <= 0) return ''
  return line.translateToString(false, 0, buf.cursorX)
}

/** 在当前光标处显示浅灰幽灵提示（显示后光标移回输入位置） */
function showGhost(): void {
  if (!term || ghostShown || !ghostEligible()) return
  ghostShown = true
  // 幽灵提示只在提示符后显示，此刻光标所在行即输入行起点：
  // 顺手刷新 prompt 起始行记录 + 用真实提示符刷新仿真 prompt（二者保持一致）
  recordAgentPromptRow(props.tabId)
  const real = readPromptText()
  if (real) agentStore.setPrompt(props.tabId, real)
  // 回位到输入位置：窄屏上提示会折行，此时【不能】用 CUB(\x1b[nD)——
  // xterm 的 cursorBackward 会把列钳在 >= 0（reverseWraparound 默认关闭），
  // 跨不了行，光标只能退到「折行那一行的行首」，也就是停在提示文字尾部而不是提示符后。
  // 折行时改用「上移行数 + 回车 + 右移列」，与 eraseAgentTextAfterPrompt / replaceAgentInput 一致。
  const buf = term.buffer.active
  const cols = term.cols || 80
  const col = buf.cursorX
  const w = displayWidth(AGENT_GHOST_HINT)
  // 折行行数：按 xterm 模型（写字符前 x >= cols 才换行，故末列占满不算折行）
  ghostRows = w > 0 ? Math.floor((col + w - 1) / cols) : 0
  const back = ghostRows > 0
    ? `\x1b[${ghostRows}A\r${col > 0 ? `\x1b[${col}C` : ''}`
    : `\x1b[${w}D`
  // bright black(90) 浅灰提示 + 回位到输入位置
  term.write(`\x1b[90m${AGENT_GHOST_HINT}\x1b[0m${back}`)
}

/** 记录当前 Agent 仿真 prompt 的起始绝对行号 */
function recordAgentPromptRow(tabId: string): void {
  if (!term) return
  const buf = term.buffer.active
  agentPromptRows.set(tabId, buf.cursorY + buf.baseY)
}

/** 仅擦除 prompt 之后的用户输入（保留 prompt），用于历史回忆等场景；
 *  提示符起始行记录失效时退化为按缓冲显示宽度回退 */
function eraseAgentTextAfterPrompt(tabId: string): void {
  if (!term) return
  const startAbs = agentPromptRows.get(tabId)
  const buf = term.buffer.active
  const curAbs = buf.cursorY + buf.baseY
  if (startAbs != null && startAbs <= curAbs && startAbs >= buf.baseY) {
    const prompt = agentStore.getPrompt(tabId) || ''
    const cols = term.cols || 80
    const col = displayWidth(prompt) % cols
    const up = curAbs - startAbs
    term.write(`${up > 0 ? `\x1b[${up}A` : ''}\r\x1b[${col}C\x1b[J`)
  } else {
    const w = displayWidth(agentStore.getBuffer(tabId))
    if (w > 0) term.write('\b \b'.repeat(w))
  }
}

/** 用新文本整体替换 Agent 输入行（保留提示符）。
 *  单次 write 完成"移回提示符末尾 + 写新文本 + 清除余量"，xterm 只渲染一帧，
 *  避免"先清空再写入"造成的可感知闪烁（光标先跳回再跳走）。 */
function replaceAgentInput(tabId: string, text: string): void {
  if (!term) return
  const startAbs = agentPromptRows.get(tabId)
  const buf = term.buffer.active
  const curAbs = buf.cursorY + buf.baseY
  const up = (startAbs != null && startAbs <= curAbs && startAbs >= buf.baseY) ? curAbs - startAbs : 0
  const prompt = agentStore.getPrompt(tabId) || ''
  const cols = term.cols || 80
  const col = displayWidth(prompt) % cols
  term.write(`${up > 0 ? `\x1b[${up}A` : ''}\r\x1b[${col}C${text}\x1b[J`)
}

/** 彻底重置当前输入行：从提示符所在行【行首】清到屏幕末尾（连提示符一起清），
 *  可选随后重写提示符。用于模式切换时清除任何本地回显残留，避免留下"删不掉"的字符。
 *  单次 write 保证原子性（不会出现中间态）。 */
function resetAgentLine(tabId: string, reprint: boolean): void {
  if (!term) return
  const startAbs = agentPromptRows.get(tabId)
  const buf = term.buffer.active
  const curAbs = buf.cursorY + buf.baseY
  const up = (startAbs != null && startAbs <= curAbs && startAbs >= buf.baseY) ? curAbs - startAbs : 0
  const prompt = reprint ? (agentStore.getPrompt(tabId) || '') : ''
  term.write(`${up > 0 ? `\x1b[${up}A` : ''}\r\x1b[J${prompt}`)
}

/** 擦除幽灵提示（光标正处于提示文本起点） */
function hideGhost(): void {
  if (!term || !ghostShown) return
  ghostShown = false
  // 单行：擦到行尾即可；折行：提示尾部在下面几行，\x1b[0K 擦不掉，改用 ED(0)
  // 从输入位置清到屏幕末尾（输入行以下没有需要保留的内容，与 eraseAgentTextAfterPrompt
  // 同一策略），ED(0) 不移动光标，擦完仍在输入位置。
  term.write(ghostRows > 0 ? '\x1b[J' : '\x1b[0K')
  ghostRows = 0
}

/** shell 输出停顿后重新显示幽灵提示（输出流中间停顿会自愈：下次写入前先擦除） */
function scheduleGhost(): void {
  if (ghostTimer) clearTimeout(ghostTimer)
  ghostTimer = null
  if (!props.agentMode) return
  ghostTimer = setTimeout(() => {
    ghostTimer = null
    // 输出已静默 400ms = shell 的退格回显落定, 切模式时的输入封锁到此为止
    if (agentSwitching) {
      agentSwitching = false
      if (switchFallback) { clearTimeout(switchFallback); switchFallback = null }
    }
    showGhost()
  }, 400)
}

/** 处理 xterm 输入：转发后端会话；Ctrl 批量模式开启时同步广播到其他会话；
 * 断连状态下按 R 键触发重连（不再向后端发送）；
 * Agent 模式下输入进入本地缓冲并回显，回车按「命令 / 自然语言」分流 */
function handleData(data: string) {
  // 任意输入（键盘 / 粘贴）前先把光标带回可视区并恢复跟随：翻历史翻过头时也能直接继续输入
  followCursorOnInput()
  // 软键盘打开期间的输入：收起键盘后不再把视口拽回弹起前的历史位置
  if (kbSavedViewport >= 0) kbDirty = true
  if (!props.connected) {
    if (data === 'r' || data === 'R') emit('reconnect')
    return
  }
  if (!term || !props.sessionId) return
  // Shell 直通（识别为命令 / AI 方案选「修改」）：输入直达真实 shell，readline 补全、历史、编辑全可用
  if (agentStore.isPassthrough(props.tabId)) {
    sendToShell(data)
    return
  }
  if (props.agentMode) {
    handleAgentInput(data)
    return
  }
  // Shell 模式下也把提交过的命令记入 Agent 历史：切到 Agent 后 ↑ 能调出这些命令
  const submitted = /[\r\n]/.test(data)
  const lineBefore = shellLine
  trackShellLine(data)
  if (submitted) {
    agentStore.pushHistory(props.tabId, lineBefore)
    if (lineBefore.trim()) agentStore.setLastCommand(props.tabId, lineBefore.trim())
  }
  sendRawToShell(data)
  emitSyncInput(data, props.sessionId)
}

/**
 * Shell 模式下「用户敲了但还没回车」的半截输入。
 * <p>用途只有一个: 切进 Agent 模式时判断 shell 的 readline 里有没有残留文本 ——
 * 那些字符是 shell 自己回显的, Agent 缓冲里没有, 会变成看得见却删不掉、回车也不执行的幽灵文本。
 * 只关心"是否为空"和大致内容, 不追求逐字符精确(Tab 补全回来的部分由 shell 输出, 这里拿不到)。</p>
 */
let shellLine = ''
/**
 * 切进 Agent 时发了 Ctrl+C 让 shell 放弃当前输入行, 正在等 shell 输出新提示符的过渡态。
 * 此期间禁止本地输入, 避免用户敲的字和稍后到达的 "^C\r\n<prompt>" 串在同一行上互相覆盖。
 */
let agentSwitching = false

function trackShellLine(data: string): void {
  // 回车 / Ctrl+C / Ctrl+D: 这一行已经交出去了, 不再有半截输入
  if (/[\r\n\x03\x04]/.test(data)) { shellLine = ''; return }
  // 方括号粘贴: 整段计入(粘贴内容若含换行会被 shell 直接执行, 谈不上半截输入)
  if (data.startsWith('\x1b[200~')) {
    const body = data.replace(/^\x1b\[200~/, '').replace(/\x1b\[201~\s*$/, '')
    if (/[\r\n]/.test(body)) shellLine = ''
    else shellLine += body.replace(/[\r\n\x00-\x1f\x7f]/g, '')
    return
  }
  // Ctrl+U: 用户在 shell 里清掉了整行
  if (data.includes('\x15')) { shellLine = ''; return }
  if (data === '\x7f' || data === '\b') { shellLine = Array.from(shellLine).slice(0, -1).join(''); return }
  // 方向键/Home/End 等转义序列与其它控制字符不纳入
  if (/[\x00-\x1f\x7f]/.test(data) || data.startsWith('\x1b')) return
  shellLine += data
}

/** 原样转发按键给会话（不做任何模式判定） */
function sendRawToShell(data: string): void {
  if (!props.sessionId) return
  invoke('send_session_input', {
    dto: { session_id: props.sessionId, data }
  }).catch(e => console.error('[term] send input failed:', e))
}

/** 直通执行完一行的收尾：前台常驻/交互式命令（vim、top、tail -f、ssh…）保持直通，
 *  否则 Ctrl+C 会被 Agent 吞掉、用户无法中断程序；普通命令跑完就交还 Agent，
 *  下一次输入仍按「命令 / 自然语言」分流（屏幕上留着 shell 自己打出的真实 prompt） */
function finishDirectRun(line: string): void {
  if (isForegroundCommand(line)) {
    agentStore.setPassthrough(props.tabId, true, true)
    ElMessage.info('Shell 直通中（前台命令运行）：Ctrl+C 中断并返回 Agent，或 Ctrl+Shift+I 转纯 Shell 模式')
  } else {
    agentStore.setPassthrough(props.tabId, false)
  }
}

/** Shell 直通下的输入：整串转发给 shell；回车/Ctrl+C/Ctrl+D 后交还 Agent 拦截 */
function sendToShell(data: string) {
  const submit = /[\r\n]/.test(data)
  const interrupt = data.includes('\x03') || data.includes('\x04')
  // 先取影子行再清零：trackShadowLine 遇回车会把影子行抹掉
  const line = shadowLine
  trackShadowLine(data)
  // 非 sticky 直通时输入是落在 shell 的 readline 上(而不是 vim/top 这类前台程序里),
  // 一并计入 shellLine —— 否则「修改命令后不回车、直接退出 Agent」留下的半截命令行,
  // 下次再切进 Agent 又会变成删不掉的幽灵文本。
  if (!agentStore.isPassthroughSticky(props.tabId)) trackShellLine(data)
  sendRawToShell(data)
  if (submit) {
    if (line) agentStore.setLastCommand(props.tabId, line)
    emit('shell-command')
    // 已在 sticky 直通中：这一行回车只是前台程序的交互输入，不重新判定
    if (!agentStore.isPassthroughSticky(props.tabId)) finishDirectRun(line)
  } else if (interrupt) {
    // Ctrl+C/Ctrl+D：终止本 Tab 进行中的任务链（含逐条发送命令的循环与卡死自动重试）
    agentStore.bumpEpoch(props.tabId)
    agentStore.setPassthrough(props.tabId, false)
  }
}

/** Agent 提交/Ctrl+C 后：换新行并写入仿真 prompt + 幽灵提示，
 *  之后确认执行的命令会回显在该 prompt 之后，视觉上与真实命令行一致 */
function startAgentLine(): void {
  if (!term) return
  const prompt = agentStore.getPrompt(props.tabId) || '> '
  // 仿真提示符与真实提示符保持同样的加粗样式（视觉一致）；
  // 这里是直接写 xterm、不走高亮器，故显式加粗并同步行首状态，避免后续行首判断被带偏。
  // xterm 的 write 异步解析：行号记录与幽灵提示都要等 prompt 真正写入后再读缓冲
  term.write(boldPrompt(prompt), () => {
    if (!term) return
    recordAgentPromptRow(props.tabId)
    showGhost()
  })
  promptHighlighter.noteRaw(prompt)
}

// ===== Agent 模式：命令识别与 Shell 直通 =====
/** 命令探测最长等待（超时就按启发式结论走，不把用户挂在回车前） */
const PROBE_TIMEOUT_MS = 900
/** 会话环境命令名探测结果缓存（同名只探一次；探测异常不缓存，下次重试） */
const probeCache = new Map<string, Promise<boolean>>()
/** 直通期间的影子命令行（只用于回车时判断该命令会不会长期占用前台，不参与显示） */
let shadowLine = ''

/** 探测命令名在会话环境里是否真实存在（静态表覆盖不到的自研 CLI / 本地内建） */
function probeCommand(name: string): Promise<boolean> {
  const key = name.toLowerCase()
  if (!key || !props.sessionId || !isTauriEnv()) return Promise.resolve(false)
  const hit = probeCache.get(key)
  if (hit) return hit
  const p = invoke<boolean>(
    'is_shell_command',
    { dto: { session_id: props.sessionId, name } },
    { timeoutMs: PROBE_TIMEOUT_MS }
  ).then(ok => ok === true, () => { probeCache.delete(key); return false })
  probeCache.set(key, p)
  return p
}

/** 命令名一输入完整就预热探测，回车时无需再等一次往返 */
function prefetchProbe(line: string): void {
  if (!/^\S+\s+\S/.test(line)) return
  const { kind, head } = classifyLine(line)
  if (kind === 'unknown' && head) void probeCommand(head)
}

/** 把已本地回显的整行交还真实 shell：先擦掉 Agent 的本地回显，再由 shell 重新回显并接管这一行，
 *  之后 Tab 补全、方向键翻历史、readline 编辑全是 shell 原生行为。
 *  注意：只擦「提示符之后的输入文本」，必须保留提示符本身 —— shell 对半截命令行只会回显命令文本，
 *  不会重画提示符；连提示符一起擦掉就会变成"命令提示符消失"。 */
async function takeoverShell(text: string, trailing: string): Promise<void> {
  if (!term || !props.sessionId) return
  const cmd = stripForcePrefix(text)
  hideGhost()
  // 擦除已回显的输入文本（跨物理行安全），保留提示符；无文本（如空行按 Tab）时无需擦
  if (displayWidth(text) > 0) eraseAgentTextAfterPrompt(props.tabId)
  // 缓冲必须同步清空：这一行已由 shell 接管，留着会在直通结束后「隐形」拼到下一个输入行
  agentStore.clearBuffer(props.tabId)
  agentStore.resetHistoryIndex(props.tabId)
  agentStore.setPassthrough(props.tabId, true)
  shadowLine = cmd
  const submit = /[\r\n]/.test(trailing)
  // 非回车式移交（Tab 补全、 confident 命令、空行↑/↓等）且确有内容：记入历史
  if (!submit && cmd) agentStore.pushHistory(props.tabId, cmd)
  if (cmd) agentStore.setLastCommand(props.tabId, cmd)
  // 未带回车(如 Tab 补全、AI 方案选「修改」): 这行命令就此躺在 shell 的 readline 里,
  // 计入 shellLine, 之后退出直通/切回 Agent 才知道命令行上还剩了什么要清理
  if (!submit) shellLine = cmd
  try {
    await invoke('send_session_input', { dto: { session_id: props.sessionId, data: cmd + trailing } })
    if (submit) {
      // 整行已带回车执行完毕：按命令类型决定继续直通还是交还 Agent（不交还的话，下一行会默默进 shell）
      shadowLine = ''
      finishDirectRun(cmd)
    }
  } catch (e) {
    console.error('[term] takeover send failed:', e)
    agentStore.setPassthrough(props.tabId, false)
    term.write('\x1b[31m✗ 命令写入终端失败\x1b[0m\r\n')
    startAgentLine()
  }
}

/** 回车提交：命令直接交给 shell 执行（不进大模型，不花积分），自然语言才发 AI */
async function submitAgentLine(line: string): Promise<void> {
  const { kind, head } = classifyLine(line)
  const direct = kind === 'command' || (kind === 'unknown' ? await probeCommand(head) : false)
  if (direct) {
    await takeoverShell(line, '\r')
    emit('shell-command')
    return
  }
  term?.write('\r\n')
  emit('agent-submit', line)
}

/** 直通期间维护影子行（控制字符/转义序列不计入，补全回来的文本不影响判定） */
function trackShadowLine(data: string): void {
  if (/[\r\n\x03\x04]/.test(data)) { shadowLine = ''; return }
  if (data === '\x7f' || data === '\b') { shadowLine = Array.from(shadowLine).slice(0, -1).join(''); return }
  if (/[\x00-\x1f\x7f]/.test(data)) return
  shadowLine += data
}

/** Agent 模式输入处理：本地缓冲 + 手工回显（shell 不回显，因为输入未发给会话）。
 *  AI 思考中仅响应 Ctrl+C 中止；待确认时拦截 执行/修改/拒绝 按键 */
function handleAgentInput(data: string) {
  if (!term) return
  // 切 Agent 后等 shell 放弃当前行并输出新提示符，过渡态内忽略输入（Ctrl+C 也跳过，避免重复触发）
  if (agentSwitching) return
  // AI 请求进行中（思考中）：只响应 Ctrl+C 中止，其余输入吞掉
  const p = agentStore.pending
  if (p && p.tabId === props.tabId) {
    if (p.loading) {
      // Ctrl+C 中止 AI 请求：先自增任务代次（阻断后续任何自动流程），再通知工作区真正取消后端请求
      if (data === '\x03') { agentStore.bumpEpoch(props.tabId); emit('agent-action', 'abort') }
      return
    }
    // 方案待确认（safe/blocked/错误已由工作区即时处理并清除 pending，走到这即需确认）
    if (p.result) {
      if (data === '\r' || data === 'y' || data === 'Y') emit('agent-action', 'execute')
      else if (data === 'm' || data === 'M') emit('agent-action', 'modify')
      else if (data === '\x1b' || data === 'n' || data === 'N' || data === '\x03') {
        agentStore.bumpEpoch(props.tabId)
        emit('agent-action', 'reject')
      }
      return
    }
  }
  const buf = agentStore.getBuffer(props.tabId)
  // Tab：会去按补全就是在敲命令 → 整行移交 shell，由 readline 完成智能补全
  if (data === '\t') {
    void takeoverShell(buf, '\t')
    return
  }
  // ↑/↓：浏览 Agent 本地历史（与 Shell 一样可反复上下翻）。
  // 需同时兼容两类按键序列：普通模式 CSI(\x1b[A) 与应用光标键模式 SS3(\x1bOA，
  // 某些程序（vim/less…）开启 DECCKM 后 xterm 就会发这种）；只匹配前者会"完全没反应"。
  const isUp = data === '\x1b[A' || data === '\x1bOA'
  const isDown = data === '\x1b[B' || data === '\x1bOB'
  if (isUp || isDown) {
    const item = isUp ? agentStore.prevHistory(props.tabId) : agentStore.nextHistory(props.tabId)
    // 上翻且没有更多历史 → 不动
    if (item == null && isUp) return
    // 幽灵提示会被 replaceAgentInput 的写入顺带覆盖清掉，只需同步标志位，
    // 避免"先 hideGhost 再写内容"分成两次 write 产生中间帧导致光标闪烁
    ghostShown = false
    // 单次写入整体替换（保留提示符），避免"先清空再写入"的光标闪烁
    replaceAgentInput(props.tabId, item ?? '')
    if (item == null) {
      // 下翻越过最新一条 → 回到空输入行
      agentStore.clearBuffer(props.tabId)
      agentStore.resetHistoryIndex(props.tabId)
      // 等替换写入生效后再显示幽灵提示（xterm write 异步）
      setTimeout(() => showGhost(), 0)
    } else {
      agentStore.setBuffer(props.tabId, item)
    }
    return
  }
  // 回车：整行按「命令 / 自然语言」分流
  if (data === '\r' || data === '\n' || data === '\r\n') {
    if (ghostTimer) { clearTimeout(ghostTimer); ghostTimer = null }
    hideGhost()
    const line = agentStore.takeBuffer(props.tabId)
    // 空行回车：等价于在 shell 提示符上直接敲回车 —— 换行出一个新提示符即可,
    // 既不进大模型也不花积分。ghost 由随后到达的 shell 回显流自动复现。
    if (!line) { sendRawToShell('\r'); return }
    autoRetriedAfterStall = false
    agentStore.pushHistory(props.tabId, line)
    agentStore.resetHistoryIndex(props.tabId)
    void submitAgentLine(line)
    return
  }
  // Ctrl+C：有选区时复制选区（与通用终端一致）；无选区才缓冲非空放弃当前行、空行转发 shell 中断
  if (data === '\x03') {
    if (term && term.hasSelection && term.hasSelection()) {
      const sel = term.getSelection()
      if (sel) { copyText(sel).catch(() => { /* 剪贴板写入失败忽略 */ }); return }
    }
    // 无论当前是"命令执行中"还是"空行等待"，Ctrl+C 都终止本 Tab 上所有进行中的任务链
    // （逐条发送命令的循环 / 卡死自动重试），并中断远端正在跑的命令
    agentStore.bumpEpoch(props.tabId)
    if (!buf) { sendRawToShell('\x03'); return }
    hideGhost()
    agentStore.clearBuffer(props.tabId)
    // 刻意【不】重置历史浏览位置：方便"↑ 看到一条不满意 → Ctrl+C 清掉 → 再 ↑ 继续往更早翻"；
    // 一旦用户重新敲字（appendBuffer/退格）会自动重置浏览位置，不会串味。
    // 只擦输入文本、保留提示符，再补一个 ^C 换行（与真实 shell 的 Ctrl+C 观感一致）
    eraseAgentTextAfterPrompt(props.tabId)
    term.write('^C\r\n')
    startAgentLine()
    return
  }
  // 退格：缓冲非空时删一个码点并按该字符显示宽度回显擦除（CJK 占 2 格，只回退 1 格会残留左半字符）
  if (data === '\x7f' || data === '\b') {
    if (!buf) return // 缓冲空：禁止回退，避免越过输入区误擦 prompt
    agentStore.resetHistoryIndex(props.tabId)
    const w = displayWidth(Array.from(buf).slice(-1)[0])
    agentStore.backspaceBuffer(props.tabId)
    term.write('\b \b'.repeat(w))
    // 等这格擦除写入生效后再判断是否回到空行（xterm write 异步）
    if (agentStore.getBuffer(props.tabId) === '') setTimeout(() => showGhost(), 0)
    return
  }
  // Ctrl+U：清空整行输入（保留提示符，光标回到提示符后）
  if (data === '\x15') {
    agentStore.clearBuffer(props.tabId)
    agentStore.resetHistoryIndex(props.tabId)
    eraseAgentTextAfterPrompt(props.tabId)
    showGhost()
    return
  }
  // Bracketed paste：term.paste / 右键粘贴 / 外部粘贴事件都会以 \x1b[200~...\x1b[201~ 形式到达，
  // 不能被下面的 \x1b 早期返回吞掉，否则右键粘贴在 Agent 模式完全失效。
  if (data.startsWith('\x1b[200~')) {
    const body = data.replace(/^\x1b\[200~/, '').replace(/\x1b\[201~?\s*$/, '')
    hideGhost()
    const text = body
      .replace(/\r\n/g, ' ').replace(/[\r\n]/g, ' ')
      .replace(/\t/g, ' ')
      .replace(/[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]/g, '')
    if (text) {
      agentStore.resetHistoryIndex(props.tabId)
      agentStore.appendBuffer(props.tabId, text)
      term.write(text)
    }
    return
  }
  // Ctrl+V：Agent 模式下 readline 在本地缓冲，不走 shell；
  // xterm 不绑定 Ctrl+V，data 收到的是 \x16。读剪贴板后整段入 Agent 缓冲。
  if (data === '\x16') {
    readClipboardText().then(raw => {
      if (!raw || !term) return
      const text = raw
        .replace(/\r\n/g, ' ').replace(/[\r\n]/g, ' ')
        .replace(/\t/g, ' ')
        .replace(/[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]/g, '')
      hideGhost()
      agentStore.resetHistoryIndex(props.tabId)
      agentStore.appendBuffer(props.tabId, text)
      term.write(text)
    }).catch(() => { /* 剪贴板读取失败忽略 */ })
    return
  }
  // 转义序列（Home/End/Delete 等）：Agent 模式不支持行编辑，忽略
  if (data.startsWith('\x1b')) return
  // 普通可打印字符/粘贴文本：入缓冲并回显（换行/Tab 折叠为空格；其余控制字符丢弃）。
  // 注意：粘贴文本常含 Tab/控制字符，逐个丢弃会整段丢失，须先清洗再判定
  const text = data.replace(/\r\n/g, ' ').replace(/[\r\n]/g, ' ').replace(/\t/g, ' ').replace(/[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]/g, '')
  if (!text) return
  // 输入前先擦掉幽灵提示，再在输入位置回显
  hideGhost()
  agentStore.resetHistoryIndex(props.tabId)
  agentStore.appendBuffer(props.tabId, text)
  term.write(text)
  const next = agentStore.getBuffer(props.tabId)
  // 行首已构成明确命令行（! 前缀 / 路径 / 已知命令 + 参数）→ 立即移交 shell，
  // 剩下的字符直接在真实 readline 里输入，Tab 补全与历史翻阅从这一刻就能用
  if (isConfidentCommandLine(next)) void takeoverShell(next, '')
  else prefetchProbe(next)
}

function resize() {
  if (!term || !containerRef.value) return
  // 容器尺寸不足时跳过 fit：
  // 0 尺寸下 fit() 会把终端 cols/rows 压成 0/1，破坏换行与渲染，并把 0 行列同步给远端 PTY
  // （提示符丢失、无法输入）；隐藏期间 ssh-output 仍持续 write，会按错误列宽折行、
  // 切回可见再 re-fit 也无法还原（表现为光标错乱 / 输出缺失 / 乱码）。
  // 注意：必须「任一方向为 0」就跳过 —— 分屏重排的中间态常常只有高度为 0（宽度正常），
  // 旧逻辑用「宽高同时为 0」判断会漏掉这种情况，正是部分分屏窗口提示符丢失的根因。
  // 尺寸恢复后由 ResizeObserver / SplitPane 的 active、splitMode watch 再次触发 re-fit。
  const w = containerRef.value.offsetWidth
  const h = containerRef.value.offsetHeight
  if (w === 0 || h === 0) return
  try { fitAddon?.fit() } catch (_) { /* ignore */ }
  // 软键盘收起恢复期内：refit 会把视口顶行顶歪（rows 增 N → viewportY 减 N），
  // 恢复校正优先于「跟随贴底」，否则视口会被拽回末尾
  if (kbRestoreTarget >= 0) applyKbRestore()
  // 移动端：软键盘弹出/收起会让可视高度骤变（见 mobile.css 第 21 节），fit 后行数重算；
  // 处于「跟随光标」时把视口贴回末尾，避免提示符/输入行被挤出可视区。
  // 桌面端不做这一步，保持原有「调整窗口大小时视口行号不变」的行为。
  else if (isMobile && followCursor && term) term.scrollToBottom()
  // 行数变化后「最底一行是哪一行」随之改变，必须重新判定状态条是否该隐藏
  checkBottomOccupied()
  if (props.sessionId && term && isTauriEnv()) {
    // 尺寸没变就不重复下发：软键盘滑入/滑出会连续触发多帧尺寸变化，
    // 每帧都发一次 resize_session 会与用户输入/输出交错，纯属噪声
    const size = `${term.cols}x${term.rows}`
    if (size !== lastSentSize) {
      lastSentSize = size
      invoke('resize_session', {
        dto: {
          session_id: props.sessionId,
          cols: term.cols,
          rows: term.rows
        }
      }).catch(() => { /* ignore */ })
    }
  }
}

/** 上一次已同步给远端的行列（`列x行`）：尺寸未变化时跳过重复下发 */
let lastSentSize = ''

/** 更新终端主题颜色 */
function updateTermTheme() {
  if (!term) return
  const colors = getTermThemeColors()
  term.options.theme = colors
}

/** 把设置里的终端偏好应用到当前实例（初始化与实时变更共用） */
function applyTermPrefs() {
  if (!term) return
  const s = settings.settings
  term.options.fontSize = effFontSize(s.terminalFontSize)
  term.options.lineHeight = s.terminalLineHeight
  term.options.cursorStyle = s.cursorStyle
  term.options.cursorBlink = s.cursorBlink
  term.options.scrollback = s.scrollbackLines
  term.options.fontFamily = termFontFamily(s.terminalFont)
}

/** 铃声：xterm v5 通过 onBell 事件通知，开启铃声提醒时用 WebAudio 短促提示音 */
let bellCtx: AudioContext | null = null
function playBell() {
  try {
    bellCtx = bellCtx || new AudioContext()
    const osc = bellCtx.createOscillator()
    const gain = bellCtx.createGain()
    osc.frequency.value = 880
    gain.gain.setValueAtTime(0.08, bellCtx.currentTime)
    gain.gain.exponentialRampToValueAtTime(0.0001, bellCtx.currentTime + 0.15)
    osc.connect(gain).connect(bellCtx.destination)
    osc.start()
    osc.stop(bellCtx.currentTime + 0.15)
  } catch { /* 音频不可用时忽略 */ }
}

/** 终端字体（带等宽回退，未安装时不至于乱排） */
function termFontFamily(font: string): string {
  return `"${font}", Consolas, "Cascadia Code", Menlo, monospace`
}

/** 选区稳定后触发复制的防抖计时器（见 onSelectionChanged） */
let selectionCopyTimer: number | null = null
/** 最近一次自动复制过的文本：避免「抬起立即复制」与「选区防抖复制」重复写两次 */
let lastAutoCopied = ''

/** 选中即复制：开关开启且有内容时写入剪贴板（同一内容不重复写，支持自动清除） */
function autoCopySelection(sel: string) {
  if (!settings.settings.selectToCopy || !sel) return
  if (sel === lastAutoCopied) return
  lastAutoCopied = sel
  copyText(sel).catch(() => { /* 剪贴板写入失败忽略 */ })
}

/** 计算选区文本（xterm 未初始化时为 ''） */
function currentSelection(): string {
  return term ? term.getSelection() : ''
}

/**
 * 选区变化 → 选区稳定后自动复制（选中即复制的主触发）。
 * 必须走 onSelectionChange：xterm 是在 **document 级 mouseup** 里才最终定型选区，
 * 而挂在终端容器上的 mouseup 监听先于它触发——双击选词 / 三击选行时那一刻
 * getSelection() 还是空串，只挂 mouseup 会静默失效。这里等选区定型后再防抖复制。
 */
function onSelectionChanged() {
  if (selectionCopyTimer !== null) { clearTimeout(selectionCopyTimer); selectionCopyTimer = null }
  if (!settings.settings.selectToCopy) return
  selectionCopyTimer = window.setTimeout(() => {
    selectionCopyTimer = null
    autoCopySelection(currentSelection())
  }, 120)
}

/** 鼠标抬起：拖拽选择时 xterm 已实时更新选区，这里做一次快路径复制（同一内容会被去重） */
function onTermMouseUp() {
  autoCopySelection(currentSelection())
}

/** 粘贴文本预处理：统一换行为 \n。
 *  Windows 剪贴板多行文本是 \r\n，若与 \n 混排，readline/PSReadLine 会把多余的 \r
 *  当作独立字符计入光标列位置，导致粘贴后按左右箭头移动光标"窜行"（跳错视觉行）。
 *  统一为 \n 后由 shell 在 bracketed paste 下自行解释多行，光标位置计算即恢复正常。 */
function normalizePasteText(text: string): string {
  return text.replace(/\r\n/g, '\n').replace(/\r/g, '\n')
}

/** 把文本按 bracketed paste 语义写入会话（统一换行，避免多行粘贴后窜行） */
function pasteText(text: string) {
  if (term) term.paste(normalizePasteText(text))
}

/** 把剪贴板内容粘贴到当前会话（「右键直接粘贴」与会话菜单「粘贴」的共用入口）。
 *  走 xterm.paste 自动启用 bracketed paste，避免多行内容粘贴后窜行 */
async function pasteFromClipboard() {
  if (!props.sessionId || !term) return
  const text = await readClipboardText()
  if (text) pasteText(text)
}

/**
 * 右键：终端任意位置右键，行为都一样（与光标位置无关）。
 * 「右键直接粘贴」开启时：直接粘贴——
 *   有选区 → 把选中文本粘到光标处（配合「选中即复制」实现"在原处选中、右键即贴回"）；
 *   无选区 → 粘贴剪贴板内容。
 * 未开启时：有选区 → 复制选区（传统终端行为）；无选区 → 弹「会话右键菜单」。
 */
function onContextMenu(e: MouseEvent) {
  e.preventDefault()
  if (settings.settings.rightClickPaste && props.sessionId) {
    const sel = currentSelection()
    if (sel) pasteText(sel)
    else void pasteFromClipboard()
    return
  }
  if (term && term.hasSelection()) {
    const sel = term.getSelection()
    if (sel) { copyText(sel).catch(() => { /* 剪贴板写入失败忽略 */ }); return }
  }
  emit('termContextMenu', { x: e.clientX, y: e.clientY })
}

/**
 * 滚轮：Ctrl + 滚轮调字号（同步设置项并持久化，跨窗口同步由 store 广播）；
 * 普通滚轮完全交回 xterm 原生滚动 —— 视口 / 滚动条 / 渲染层三者始终一致。
 *
 * 为什么不再做「越底留白（把 .xterm-screen 用 transform 抬起来）」：
 * 平移只改变视觉，xterm 的视口位置与滚动条并不知道，于是
 *   1) 滚轮能抬起内容但滚动条不动，两者立刻脱钩；
 *   2) 抬起后腾出的空白被当成「容器底部」，再拖滚动条时内容还没到底就开始消失。
 * 现在终端铺满整个面板（底部不留死区），状态条的遮挡问题改由「底行有文字就隐藏状态条」
 * 解决（见 checkBottomOccupied），因此滚动完全交回原生，上述副作用不存在。
 */
function onWheelWithCtrl(e: WheelEvent) {
  if (!term || !e.ctrlKey) return
  e.preventDefault()
  const step = e.deltaY < 0 ? 1 : -1
  const next = Math.min(FONT_MAX, Math.max(FONT_MIN, settings.settings.terminalFontSize + step))
  if (next !== settings.settings.terminalFontSize) {
    settings.patchSettings({ terminalFontSize: next })
  }
}

// ===== 移动端触摸滚动（带惯性）=====
/**
 * 为什么必须自己实现：
 * 1) xterm 的 `.xterm-viewport` 虽然是原生滚动容器，但被 `.xterm-screen`（承载文字/canvas）
 *    盖在上层，触摸落在不可滚动的屏幕层上，浏览器不会去滚它；
 * 2) 即便触摸命中视口，xterm 的 `Viewport._handleScroll` 会
 *    `Math.round(scrollTop / 行高)` 把滚动量取整到「行」并回写 scrollTop，
 *    浏览器原生的惯性会被逐帧打断，观感发涩。
 * 桌面端的滚轮走 xterm 自己的 wheel 处理，触摸端没有等价实现 ——
 * 这里在终端容器上接管竖向触摸：按「行」滚动（`scrollLines` 只接受整数，
 * 非整数会直接抛错），抬手后按速度做指数衰减的惯性滑动。
 */
/** 判定为「滚动」而不是点击的最小位移（px） */
const TOUCH_MIN_MOVE = 6
/** 触发惯性滑动的最小抬手速度（px/ms） */
const FLING_MIN_V = 0.12
/** 惯性停止阈值（px/ms） */
const FLING_STOP_V = 0.02
/** 惯性速度衰减：速度每 120ms 减半 */
const FLING_DECAY_PER_MS = Math.LN2 / 120

let touchActive = false
/** 本次手势的方向：'' 未定 / 'v' 竖向（我们接管）/ 'h' 横向（交给既有手势） */
let touchAxis: '' | 'v' | 'h' = ''
let touchStartX = 0
let touchStartY = 0
let touchLastY = 0
let touchLastT = 0
/** 手指竖向速度（px/ms，正数 = 向下拖） */
let touchV = 0
/** 未满一行的滚动余量（px）：`scrollLines` 只接受整数行，余量留到下次累计 */
let scrollRemainderPx = 0
let flingRaf = 0

/** 单元格高度（px）：像素位移 → 行数 的换算基准 */
function cellHeightPx(): number {
  const h = (term as any)?._core?._renderService?.dimensions?.css?.cell?.height
  return typeof h === 'number' && h > 0 ? h : 0
}

/** 按像素滚动终端：正数 = 手指向下拖 = 视口上移、看更早的历史 */
function scrollByPx(px: number): void {
  if (!term) return
  const rh = cellHeightPx()
  if (rh <= 0) return
  scrollRemainderPx += px
  const lines = Math.trunc(scrollRemainderPx / rh)
  if (lines === 0) return
  scrollRemainderPx -= lines * rh
  term.scrollLines(-lines)
}

function cancelFling(): void {
  if (flingRaf) { cancelAnimationFrame(flingRaf); flingRaf = 0 }
}

/** 抬手惯性：按当前速度继续滚动并指数衰减，贴到缓冲两端即自然停止 */
function startFling(v0: number): void {
  cancelFling()
  let v = v0
  let last = performance.now()
  const step = (now: number): void => {
    flingRaf = 0
    if (!term) return
    // 帧间隔封顶：后台切回/卡顿后不补动一大段
    const dt = Math.min(40, Math.max(1, now - last))
    last = now
    v *= Math.exp(-FLING_DECAY_PER_MS * dt)
    if (Math.abs(v) < FLING_STOP_V) return
    const buf = term.buffer.active
    // 已经贴到缓冲顶/底：惯性结束（view 向上 = 正速度）
    if (v > 0 ? buf.viewportY <= 0 : buf.viewportY >= buf.baseY) return
    scrollByPx(v * dt)
    flingRaf = requestAnimationFrame(step)
  }
  flingRaf = requestAnimationFrame(step)
}

function onTermTouchStart(e: TouchEvent): void {
  if (!isMobile || !term) return
  cancelFling()
  scrollRemainderPx = 0
  touchActive = false
  touchAxis = ''
  touchV = 0
  // 多指（缩放等）不接管，保持浏览器默认行为
  if (e.touches.length !== 1) return
  const t = e.touches[0]
  touchActive = true
  touchStartX = t.clientX
  touchStartY = t.clientY
  touchLastY = t.clientY
  touchLastT = performance.now()
}

function onTermTouchMove(e: TouchEvent): void {
  if (!touchActive || !term) return
  const t = e.touches[0]
  if (!t) return
  if (!touchAxis) {
    const dy0 = t.clientY - touchStartY
    const dx0 = t.clientX - touchStartX
    if (Math.abs(dy0) < TOUCH_MIN_MOVE && Math.abs(dx0) < TOUCH_MIN_MOVE) return
    // 先定方向：只有竖向才接管（横向留给页面切换等既有手势）
    touchAxis = Math.abs(dy0) >= Math.abs(dx0) ? 'v' : 'h'
  }
  if (touchAxis !== 'v') return
  // 竖向滚动已由我们实现，阻止浏览器默认行为，避免页面/容器跟着动
  if (e.cancelable) e.preventDefault()
  const now = performance.now()
  const dy = t.clientY - touchLastY
  const dt = Math.max(1, now - touchLastT)
  // 速度取滑动平均，避免单帧抖动让抬手速度失真
  touchV = touchV * 0.4 + (dy / dt) * 0.6
  touchLastY = t.clientY
  touchLastT = now
  scrollByPx(dy)
}

function onTermTouchEnd(): void {
  if (!touchActive) return
  touchActive = false
  const axis = touchAxis
  touchAxis = ''
  if (axis !== 'v') return
  if (Math.abs(touchV) >= FLING_MIN_V) startFling(touchV)
  touchV = 0
}

/**
 * 底行占用检测：左下角系统状态条悬浮在面板最底边（bottom:4px + 约 15px 高 ≈ 19px），
 * 终端铺满整个面板时它只可能压住「视口最底一行」（状态条高度小于行高，够不到倒数第二行；
 * 纵向取整余量最多再让出 d∈[0,17) 像素，那一行永远是「最底一行」）。
 * 因此：最底一行有可见文字（提示符/输出/vim 状态行）→ 上报 true，工作区把状态条降到 25%
 * 让内容透出来；该行为空（清屏 / 输出较少留白 / 内容滚走）→ 上报 false，自动恢复 100%。
 * 开销仅 O(列数)，高频触发无压力。
 */
function checkBottomOccupied() {
  if (!term) return
  const buf = term.buffer.active
  const r = buf.viewportY + term.rows - 1
  const line = r >= 0 ? buf.getLine(r) : null
  let occupied = false
  if (line) {
    for (let x = 0; x < term.cols; x++) {
      const cell = line.getCell(x)
      // getChars()：空白单元格返回空串；有任意可见字符即算占用
      if (cell && cell.getChars()) { occupied = true; break }
    }
  }
  setTermBottomOccupied(props.tabId, occupied)
}

/**
 * 状态条占用判定循环：每 120ms 复核一次「视口最底行是否被终端文字占用」。
 *
 * 为什么不止靠事件驱动：终端重绘 / 滚动 / 主题切换在时间上会交错，事件驱动总有漏判窗口，
 * 表现为偶发「底行明明有文字、状态条却没隐藏，把文字压住」。
 * 轮询（值不变时不触发响应式更新）+ 事件钩子双保险，任何变化都会在 ~120ms 内被纠正。
 */
let occupiedTimer: ReturnType<typeof setInterval> | null = null
function startBottomOccupiedLoop(): void {
  if (occupiedTimer) return
  // 用 setInterval 而非 rAF：rAF 循环若 checkBottomOccupied 抛一次异常就永久停摆；
  // 轮询 + try/catch 保证异常不影响下一次。
  occupiedTimer = setInterval(() => {
    if (!term) return
    try { checkBottomOccupied() } catch { /* 保底：单次异常不影响后续复核 */ }
  }, 120)
}
function stopBottomOccupiedLoop(): void {
  if (occupiedTimer) { clearInterval(occupiedTimer); occupiedTimer = null }
}

/**
 * 移动端滚动：把「底行占用复核」合并到每帧一次。
 *
 * onScroll 在滑动/fling 时会以远高于帧率的频率触发，每次都要做一遍 O(列数) 的单元格扫描；
 * 与渲染抢主线程后表现为滑动发涩。用 rAF 合并后，最多每帧算一次，结果完全一致
 * （复核只关心「当前视口最底行」的最终状态）。桌面端保持原来的同步调用，行为不变。
 */
let occupiedRaf = 0
function scheduleBottomOccupied(): void {
  if (!isMobile) { checkBottomOccupied(); return }
  if (occupiedRaf) return
  occupiedRaf = requestAnimationFrame(() => {
    occupiedRaf = 0
    try { checkBottomOccupied() } catch { /* 单次异常不影响后续复核 */ }
  })
}
function cancelScheduledOccupied(): void {
  if (occupiedRaf) { cancelAnimationFrame(occupiedRaf); occupiedRaf = 0 }
}

/**
 * 启用 WebGL 渲染器（全平台）：xterm 默认的 DOM 渲染器在每次滚动/输出时都要重建
 * 可见行的单元格 DOM（一屏几十行 × 上百个 span），输出多或快速滚动时帧率必然掉；
 * WebGL 渲染器把整屏画到一张纹理上，滚动只是重绘纹理，是滚动「丝滑 60 帧」的关键。
 *
 * 兜底策略：
 * - 加载失败 / 不支持 WebGL（老 WebView、远程桌面、显卡驱动被禁）→ 保持默认 DOM 渲染器；
 * - 上下文丢失（系统回收、开的终端过多）→ 释放插件，xterm 自动回退到 DOM 渲染器，
 *   不会出现整块终端空白。
 */
function enableGpuRenderer(): void {
  if (!term) return
  // 移动端禁用 WebGL（见 CAP.gpuTerminalRenderer 说明）：Android/iOS WebView 上
  // WebGL 上下文数量有限、易丢失，且容器在 display:none 期间尺寸为 0，
  // 会导致终端画布变黑且后续新建终端也黑。移动端回退到 xterm 默认 DOM 渲染器。
  if (!CAP.gpuTerminalRenderer()) return
  try {
    const addon = new WebglAddon()
    addon.onContextLoss(() => {
      addon.dispose()
      if (webglAddon === addon) webglAddon = null
    })
    term.loadAddon(addon)
    webglAddon = addon
  } catch {
    webglAddon = null
  }
}

/** 视口是否跟随光标：用户主动翻历史后置 false，敲键/粘贴后重新跟随 */
let followCursor = true

/**
 * 光标在缓冲中的绝对行号。
 *
 * 关键：xterm 的 buffer.cursorY 是「相对底部页顶 baseY」的行号（0 = 光标在 baseY，
 * rows-1 = 光标在最后一行），**不是**相对当前视口顶部的行号。
 * 早期实现直接拿 cursorY 与 0..rows-1 比较，于是视口翻到历史区时该值仍在这个区间内，
 * 判定恒为「可见」，回滚逻辑永不触发 —— 表现为「翻历史后敲键，光标不回可视区」。
 * 必须补回 baseY 才是可用于和 viewportY 比较的绝对行号。
 */
function cursorAbsRow(): number | null {
  if (!term) return null
  const buf = term.buffer.active
  return buf.baseY + buf.cursorY
}

/** 光标当前是否落在可视区内：绝对行号落在 [viewportY, viewportY + rows - 1] 内即可见 */
function isCursorVisible(): boolean {
  if (!term) return true
  const buf = term.buffer.active
  const abs = buf.baseY + buf.cursorY
  return abs >= buf.viewportY && abs <= buf.viewportY + term.rows - 1
}

/**
 * 输入（键盘 / 粘贴）时把光标带回可视区：
 * 光标已在可视区内 → 完全不动视口（不打断用户看历史）；
 * 光标已在可视区外 → 滚回缓冲末尾。
 *
 * 为什么用 scrollToBottom 而不是「最小滚动」：普通缓冲里光标始终位于底部页
 * [baseY, baseY + rows - 1]，视口只可能向上翻，因此光标只会落在视口下方；
 * 滚回末尾既能保证光标可见，又能把 followCursor 复位为「跟随」
 * （最小滚动可能停在半途，onScroll 会把 followCursor 又置回 false，
 * 于是后续输出不再跟随，光标会重新飘出可视区）。
 * 备用屏（vim / htop）的 viewportY 恒为 0、光标必然可见，这里不会误滚。
 *
 * 覆盖所有输入路径：handleData 是 xterm 的 onData 回调，
 * 键盘输入与 term.paste()（右键粘贴 / Ctrl+Shift+V）都从这里进来。
 */
function followCursorOnInput(): void {
  if (!term) return
  const abs = cursorAbsRow()
  if (abs !== null && !isCursorVisible()) {
    const buf = term.buffer.active
    if (abs < buf.viewportY) term.scrollLines(abs - buf.viewportY) // 理论不可达：光标在视口上方
    else term.scrollToBottom()
  }
  followCursor = true   // 放在滚动之后：onScroll 会按视口位置改写它
}

// ===== 移动端软键盘：弹起保光标可见，收起恢复原视口 =====
/**
 * 键盘弹起前保存的视口行号（-1 = 未保存）。
 * 兼作「键盘当前开着」的标记：只在弹起时写入、收起时清除，
 * handleData 据此标记键盘期间的输入（有输入则收起时不再回跳）。
 */
let kbSavedViewport = -1
/** 键盘打开期间是否有过输入：有输入说明用户已开始干活，收起键盘不应再把视口拽回历史区 */
let kbDirty = false
/** 待恢复的视口行号（-1 = 无）；键盘收起动画期间图层连续 refit，需按当前值反复校正 */
let kbRestoreTarget = -1
/** 恢复校正截止时间：覆盖 Android 键盘收起动画（~250ms）+ 余量 */
let kbRestoreUntil = 0
/** 恢复滚动进行中：onScroll 据此区分程序滚动与用户手势滚动 */
let kbApplying = false
const KB_RESTORE_WINDOW_MS = 400

/**
 * 软键盘收起后的视口恢复：把视口拉回弹起前保存的位置。
 * 必须做成「可重复校正」而非一次性 scrollLines：图层回弹引发 refit，
 * xterm 行数增大时视口顶行会整体上移（rows 增 N → viewportY 减 N），
 * 单次恢复会被随后的 refit 顶歪（实测偏差 = 行数增量）。
 */
function applyKbRestore(): void {
  if (!term || kbRestoreTarget < 0) return
  if (Date.now() > kbRestoreUntil) { kbRestoreTarget = -1; return }
  const buf = term.buffer.active
  const target = Math.min(kbRestoreTarget, buf.baseY)
  if (buf.viewportY === target) return
  kbApplying = true
  term.scrollLines(target - buf.viewportY)
  kbApplying = false
}

/**
 * 软键盘弹起/收起（mobileKeyboard 派发的 KB_TOGGLE_EVENT）：
 * 弹起 —— 终端图层被收短到键盘上沿（mobile.css 第 21 节 + DockLayout 内联精算），
 *   若用户此前翻在历史区，光标会被挤到键盘后方；这里保存原视口并强制贴底，
 *   保证键盘弹起后光标一定在键盘上方可见。
 * 收起 —— 若期间没有输入、且视口仍贴在底部（用户也没翻页），恢复弹起前的阅读位置；
 *   任一条件不满足则保持现状（布局高度恢复由 CSS/内联样式负责）。
 */
function onKbToggle(e: Event): void {
  if (!isMobile || !term) return
  const open = (e as CustomEvent<{ open: boolean }>).detail?.open === true
  if (open) {
    kbRestoreTarget = -1   // 新一轮键盘周期，取消未完成的恢复
    if (!isCursorVisible()) kbSavedViewport = term.buffer.active.viewportY
    kbDirty = false
    followCursor = true
    term.scrollToBottom()
    // 图层收短 → ResizeObserver → resize() refit 后还会再贴底一次；
    // 这里补一个宏任务兜底，覆盖 RO 回调延迟/不触发的机型
    setTimeout(() => {
      if (term && document.documentElement.classList.contains('jc-kb-open')) term.scrollToBottom()
    }, 60)
    return
  }
  if (kbSavedViewport < 0) return
  const saved = kbSavedViewport
  kbSavedViewport = -1
  const buf = term.buffer.active
  if (kbDirty || buf.viewportY < buf.baseY) return
  // 先显式置 followCursor=false：否则随后 refit 触发的 resize() 会重新贴底、毁掉恢复。
  followCursor = false
  // 记录目标行号进入恢复窗口：立即校正一次，之后由 resize()/applyKbRestore
  // 在键盘收起动画期间反复校正（refit 会把视口顶行顶歪，单次恢复无效）。
  kbRestoreTarget = saved
  kbRestoreUntil = Date.now() + KB_RESTORE_WINDOW_MS
  setTimeout(() => { kbRestoreTarget = -1 }, KB_RESTORE_WINDOW_MS + 50)
  applyKbRestore()
}

onMounted(() => {
  if (!containerRef.value) return
  const colors = getTermThemeColors()
  const s = settings.settings
  term = new Terminal({
    fontFamily: termFontFamily(s.terminalFont),
    fontSize: effFontSize(s.terminalFontSize),
    lineHeight: s.terminalLineHeight,
    cursorBlink: s.cursorBlink,
    cursorStyle: s.cursorStyle,
    scrollback: s.scrollbackLines,
    // 关掉 xterm 自带的「输入即回底」（它在 keydown 时无条件 scrollToBottom）。
    // 改用 followCursorOnInput + onWriteParsed 自己跟随：光标在可视区内就不动视口，
    // 只有光标被翻出/挤出可视区时才滚回末尾 —— 敲键不会被无谓地拽回底部。
    scrollOnUserInput: false,
    convertEol: props.kind !== 'local',
    // 本地 Windows 会话走 ConPTY：开启 windowsMode 让 xterm 按 Windows 行尾(\r\n)规则
    // 处理光标与折行，缓解粘贴长文本后左右箭头移动光标"窜行"（光标跳错视觉行）的问题
    windowsMode: props.kind === 'local',
    theme: colors
  })
  fitAddon = new FitAddon()
  serializeAddon = new SerializeAddon()
  term.loadAddon(fitAddon)
  term.loadAddon(serializeAddon)
  term.loadAddon(new WebLinksAddon())
  term.open(containerRef.value)
  // 必须在 open 之后加载渲染器插件（插件依赖 terminal 元素与字符度量）
  enableGpuRenderer()
  resize()
  term.onData(handleData)
  // 每次重绘复核状态条占用：输出渲染 / 清屏 / 切主题都会走到这里
  term.onRender(() => checkBottomOccupied())
  // 视口回到缓冲末尾 → 重新跟随光标；用户翻进历史 → 暂停跟随（不再抢视口）
  term.onScroll(() => {
    if (!term) return
    // 恢复窗口内的用户手势滚动（滚轮/触摸）：放弃恢复，视口交给用户
    if (!kbApplying && kbRestoreTarget >= 0) { kbRestoreTarget = -1; kbRestoreUntil = 0 }
    const buf = term.buffer.active
    followCursor = buf.viewportY >= buf.baseY
    // 滚动高频触发：移动端合并到每帧一次，避免逐事件扫描单元格拖慢滑动
    scheduleBottomOccupied()
  })
  // 跟随兜底：光标被输出推到视口外时滚回末尾（不依赖 xterm 自身的贴底行为）
  term.onWriteParsed(() => {
    if (term && followCursor && !isCursorVisible()) term.scrollToBottom()
    // 写入（含光标重绘）常早于 onRender 落地，这里补一次判定，避免状态条多压一帧
    checkBottomOccupied()
  })
  term.onBell(() => { if (settings.settings.bellAlert) playBell() })
  // 选区变化：缓存选区文本 + 选区稳定后兜底自动复制（见 onSelectionChanged）
  term.onSelectionChange(onSelectionChanged)
  containerRef.value.addEventListener('wheel', onWheelWithCtrl, { passive: false, capture: true })
  containerRef.value.addEventListener('mouseup', onTermMouseUp)
  // 移动端触摸滚动（带惯性）：桌面端不注册，滚轮行为完全不变
  if (isMobile) {
    containerRef.value.addEventListener('touchstart', onTermTouchStart, { passive: true })
    // touchmove 必须非被动：竖向手势要 preventDefault 阻止浏览器默认滚动
    containerRef.value.addEventListener('touchmove', onTermTouchMove, { passive: false })
    containerRef.value.addEventListener('touchend', onTermTouchEnd, { passive: true })
    containerRef.value.addEventListener('touchcancel', onTermTouchEnd, { passive: true })
  }
  // 轮询兜底：保证「底行有文字 → 状态条必隐藏」不发生偶发漏判
  startBottomOccupiedLoop()

  // 移动端软键盘弹起/收起：保光标可见 + 恢复原视口（桌面端安装为空操作，监听无害）
  document.addEventListener(KB_TOGGLE_EVENT, onKbToggle)

  resizeObserver = new ResizeObserver(() => resize())
  resizeObserver.observe(containerRef.value)
})

onBeforeUnmount(() => {
  if (ghostTimer) { clearTimeout(ghostTimer); ghostTimer = null }
  if (switchFallback) { clearTimeout(switchFallback); switchFallback = null }
  if (selectionCopyTimer !== null) { clearTimeout(selectionCopyTimer); selectionCopyTimer = null }
  stopBottomOccupiedLoop()
  cancelScheduledOccupied()
  ghostShown = false
  stallGuard.dispose()
  agentPromptRows.delete(props.tabId)
  agentStore.resetHistoryIndex(props.tabId)
  clearTermBottomOccupied(props.tabId)
  document.removeEventListener(KB_TOGGLE_EVENT, onKbToggle)
  // 触摸滚动：停掉未完成的惯性并摘掉监听
  cancelFling()
  containerRef.value?.removeEventListener('touchstart', onTermTouchStart)
  containerRef.value?.removeEventListener('touchmove', onTermTouchMove)
  containerRef.value?.removeEventListener('touchend', onTermTouchEnd)
  containerRef.value?.removeEventListener('touchcancel', onTermTouchEnd)
  containerRef.value?.removeEventListener('wheel', onWheelWithCtrl, { capture: true } as EventListenerOptions)
  resizeObserver?.disconnect()
  // 先释放渲染器插件，再销毁终端：避免 WebGL 上下文残留（Android WebView 上下文数量有限）
  webglAddon?.dispose()
  webglAddon = null
  fitAddon?.dispose()
  serializeAddon?.dispose()
  term?.dispose()
  term = null
  fitAddon = null
  serializeAddon = null
  resizeObserver = null
})

watch(() => props.tabId, () => resize())
watch(() => props.sessionId, (sid) => {
  // 换会话：命令探测缓存与影子行作废（上一个环境的命令不一定存在）
  probeCache.clear()
  shadowLine = ''
  shellLine = ''
  agentPromptRows.delete(props.tabId)
  agentStore.resetHistoryIndex(props.tabId)
  if (sid && term) {
    // 换会话：新会话还没收到过任何尺寸，清掉去重记录保证首帧尺寸一定下发
    lastSentSize = ''
    resize()
    term.focus()
  }
})

// 重连后后端会新建 PTY（默认 80x24）：同样清掉尺寸去重记录，
// 避免随后那次 resize 被误判成「尺寸没变」而不下发，导致远端列宽不对
watch(() => props.connected, (on) => { if (on) lastSentSize = '' })

/**
 * Agent 模式开关：开启时在真实 shell 提示符后的光标处显示幽灵提示；关闭时擦除。
 * <p>切进 Agent 时若 shell 的 readline 里还留着半截未回车的输入, 必须先让 shell 把它丢掉:
 * Agent 的本地缓冲读不到 shell 行的内容, 那些字符在屏幕上是 shell 回显的,
 * 于是既不在 Agent 缓冲里（退格被"缓冲空则禁止回退"挡住）、也提交不出去（回车时 buffer 为空直接返回）。
 * 做法是给 shell 发 N 个退格, 让它自己把 readline 里的半截输入删掉（详见下方实现注释）。
 * 过渡态内忽略本地输入, 避免用户敲的字被 shell 稍后到达的退格回显擦掉。</p>
 */
watch(() => props.agentMode, (on) => {
  if (ghostTimer) { clearTimeout(ghostTimer); ghostTimer = null }
  if (!on) {
    // 退出 Agent：把本地回显彻底清掉并把提示符原样写回，避免留下"删不掉"的幽灵字符。
    hideGhost()
    // shell 的 readline 里若还留着半截输入（直通期间敲进去的），先让 shell 自己删掉，
    // 否则切回 Shell 后命令行里残留却看不见，回车还会执行。
    if (shellLine && props.sessionId && props.connected) {
      sendRawToShell('\x7f'.repeat(Array.from(shellLine).length))
    }
    shellLine = ''
    agentStore.clearBuffer(props.tabId)
    // 连提示符一起清掉再重写（单次写入，无闪烁），彻底消除任何残留
    resetAgentLine(props.tabId, true)
    agentStore.resetHistoryIndex(props.tabId)
    agentPromptRows.delete(props.tabId)
    agentSwitching = false
    if (switchFallback) { clearTimeout(switchFallback); switchFallback = null }
    return
  }
  if (shellLine && props.sessionId && props.connected) {
    // 用 N 个退格让 shell 自己删掉 readline 里的半截输入, 而不是 Ctrl+C / Ctrl+U:
    //  - Ctrl+U 在 PowerShell 默认的 Windows 模式下没有绑定, 等于白发;
    //  - Ctrl+C 会另起一行并留下 "^C", 反复切模式时上一次的残留行仍留在屏幕上;
    //  - 退格是 bash/zsh/PowerShell/cmd 全都有的绑定, 且只在当前行内回退, 不换行、不发 SIGINT。
    // 多发几个也安全: readline 删到行首就停住, 不会吃掉提示符。
    const n = Array.from(shellLine).length
    shellLine = ''
    agentSwitching = true
    sendRawToShell('\x7f'.repeat(n))
    if (switchFallback) clearTimeout(switchFallback)
    switchFallback = setTimeout(() => {
      switchFallback = null
      agentSwitching = false
      showGhost()
    }, 1500)
    return
  }
  // 无 shell 残留：清一次行 + 重写提示符，确保没有上一模式遗留的本地回显；再显示幽灵提示。
  // 仅在光标确实停在提示符后时重置，避免命令执行途中误清输出行。
  if (cursorAtPrompt()) resetAgentLine(props.tabId, true)
  setTimeout(() => showGhost(), 0)
})

/** 监听主题/终端背景色/终端字体色变化，动态更新终端颜色 */
watch(() => [app.effectiveTheme, app.terminalBgColor, app.terminalFgColor], () => {
  updateTermTheme()
})

/** 设置项（字号/行高/光标/回滚等）变化时实时应用，字号变化后重新 fit */
watch(() => settings.settings, () => {
  const prevSize = term?.options.fontSize
  const prevFont = term?.options.fontFamily
  applyTermPrefs()
  const sizeChanged = prevSize !== term?.options.fontSize
  const fontChanged = prevFont !== term?.options.fontFamily
  // WebGL 渲染器把字模缓存在图集里：字体（族或字号）变了必须重建图集，否则会出现花字
  if (fontChanged && webglAddon) webglAddon.clearTextureAtlas()
  // 字号变化需要重新 fit；移动端字体族变化同样影响字符度量，一并重新 fit（桌面端保持原行为）
  if (term && (sizeChanged || (isMobile && fontChanged))) resize()
}, { deep: true })

defineExpose({
  /** 写入会话输出：先擦除幽灵提示避免字符残留，输出停顿后再复现 */
  write: (text: string) => {
    if (!term) return
    stallGuard.feed(text)
    hideGhost()
    term.write(promptHighlighter.transform(text, settings.settings.highlightPrompt))
    scheduleGhost()
  },
  /** 写入并返回 Promise（渲染完成后 resolve）：
   *  锚定浮层前调用，确保光标/缓冲区已更新，registerMarker 位置准确 */
  writeDone: (text: string): Promise<void> => new Promise(resolve => {
    if (!term) return resolve()
    stallGuard.feed(text)
    hideGhost()
    term.write(promptHighlighter.transform(text, settings.settings.highlightPrompt), () => { scheduleGhost(); resolve() })
  }),
  writeln: (text: string) => {
    if (text) stallGuard.feed(text)
    if (!term) return
    // 补上 \r\n 再变换：让高亮器知道这一行确实结束了（否则行首状态会被带偏）
    term.write(promptHighlighter.transform(`${text}\r\n`, settings.settings.highlightPrompt))
  },
  /** Agent 新输入行：写仿真 prompt + 幽灵提示（工作区在流程结束后调用） */
  agentNewLine: () => startAgentLine(),
  /** Agent 终端内浮层：把 HTML 锚定到光标处标记行（手动定位，监听滚动保持跟随）。
   *  返回释放函数；失败返回 null（调用方回退纯文本渲染） */
  agentOverlay: (lineOffset: number, el: HTMLElement, heightPx: number): (() => void) | null => {
    if (!term || !containerRef.value) return null
    const marker = term.registerMarker(lineOffset)
    if (!marker) return null
    
    // 获取单元格尺寸
    const cellH = (term as any)._core?._renderService?.dimensions?.css?.cell?.height || 17
    const cellW = (term as any)._core?._renderService?.dimensions?.css?.cell?.width || 8
    
    // 使用 xterm 的屏幕容器（.xterm-screen）作为父元素，定位更准确
    const screenEl = containerRef.value.querySelector('.xterm-screen') as HTMLElement | undefined
    if (!screenEl) return null
    
    // 创建定位容器（相对于 .xterm-screen）
    const container = document.createElement('div')
    container.style.cssText = 'position:absolute;left:0;top:0;width:100%;pointer-events:none;z-index:100;overflow:visible;'
    
    // 定位浮层元素（用单独属性赋值，不用 cssText 避免覆盖 buildConfirmOverlay 设置的样式）
    el.style.position = 'absolute'
    el.style.left = '0'
    el.style.pointerEvents = 'auto'
    el.style.width = (term.cols * cellW) + 'px'
    
    container.appendChild(el)
    screenEl.appendChild(container)
    
    // 更新位置的函数：marker.line 是绝对行号，需要减去视口起始行得到视口内位置。
    // 注意必须用 viewportY 而不是 baseY：baseY 只是「滚到底时视口的第一行」，
    // 用户上翻历史后两者不等，用 baseY 会让浮层相对内容错位（.xterm-screen 是视口坐标系）。
    const updatePosition = () => {
      if (!term) return
      const buffer = term.buffer.active
      const offsetY = (marker.line - buffer.viewportY) * cellH
      // 如果标记行在视口外，隐藏浮层
      if (offsetY < 0 || offsetY >= term.rows * cellH) {
        el.style.display = 'none'
      } else {
        el.style.display = 'block'
        el.style.top = offsetY + 'px'
      }
    }
    
    // 初始定位
    updatePosition()
    
    // 监听视口滚动和 resize 事件
    const onViewportChange = () => updatePosition()
    term.onScroll(onViewportChange)
    term.onResize(onViewportChange)
    
    let disposed = false
    return () => {
      if (disposed) return
      disposed = true
      container.remove()
      try { marker.dispose() } catch { /* 终端已销毁时忽略 */ }
    }
  },
  /** 离屏挂载浮层测量它实际占用的终端行数（跟随字号/换行自适应，避免预留过多空行） */
  measureOverlayLines: (el: HTMLElement): number => {
    if (!term || !containerRef.value) return 0
    const screenEl = containerRef.value.querySelector('.xterm-screen') as HTMLElement | null
    if (!screenEl) return 0
    const cellH = (term as any)._core?._renderService?.dimensions?.css?.cell?.height || 17
    const cellW = (term as any)._core?._renderService?.dimensions?.css?.cell?.width || 8
    el.style.position = 'absolute'
    el.style.left = '0'
    el.style.top = '0'
    el.style.visibility = 'hidden'
    el.style.width = (term.cols * cellW) + 'px'
    screenEl.appendChild(el)
    const h = el.offsetHeight
    el.remove()
    el.style.visibility = ''
    return Math.max(1, Math.ceil(h / cellH))
  },
  /** 删除光标上方 n 行（浮层预留的定位空行）：用 DEC DL 真删行，下方内容整体上移，不留残白。
   *  预留区已部分滚入回滚缓冲区时放弃操作并返回 false */
  eraseReservedLines: (n: number): boolean => {
    if (!term || n <= 0) return false
    const buf = term.buffer.active
    if (buf.cursorY < n) return false
    term.write(`\x1b[${n}A\x1b[${n}M`)
    return true
  },
  clear: () => term?.clear(),
  focus: () => term?.focus(),
  resize: () => resize(),
  /** 从隐藏(display:none)切回可见时调用：重新 fit + 整屏重绘。
   *  非激活 Tab 被 v-show 隐藏时容器尺寸为 0，mount/watcher 里的 resize() 被「0 尺寸跳过」逻辑挡掉；
   *  切回可见后 xterm（尤其 WebGL 渲染层）停留在旧尺寸画布上表现为黑屏，必须显式 refresh 整屏，
   *  否则要等下一条输出才刷新 —— 空闲会话会一直黑（关掉其它 Tab 后剩余 Tab 变黑的正是这个）。 */
  onShow: () => {
    resize()
    if (term) {
      try { term.refresh(0, Math.max(0, term.rows - 1)) } catch { /* 终端已销毁时忽略 */ }
    }
  },
  /** 当前终端尺寸：连接时告知远端 PTY，避免远端按默认 80x24 建模导致 readline 重绘错位 */
  dimensions: (): { cols: number; rows: number } => ({ cols: term?.cols ?? 80, rows: term?.rows ?? 24 }),
  /** 屏幕上 shell 的真实提示符文本（供 Agent 仿真 prompt 对齐，避免 IP/host 两种写法）；
   *  仅当光标确实停在提示符后（非命令执行中的输出行）才返回，避免把输出当成 prompt */
  currentPromptText: (): string => (cursorAtPrompt() ? readPromptText() : ''),
  serialize: (): string => {
    try {
      return serializeAddon?.serialize({ scrollback: 2000 }) ?? ''
    } catch {
      return ''
    }
  },
  restore: (data: string) => {
    if (!data || !term) return
    promptHighlighter.noteRaw(data)
    term.write(data)
  },
  /** 复制选区到剪贴板 */
  copySelection: async () => {
    const sel = term?.getSelection()
    if (sel) { try { await copyText(sel) } catch { /* 忽略 */ } }
  },
  /** 粘贴剪贴板内容到终端（与「右键直接粘贴」共用同一实现） */
  paste: pasteFromClipboard,
  /** 全选 */
  selectAll: () => term?.selectAll(),
  /** 放大字号（同步持久化） */
  zoomIn: () => {
    const next = Math.min(FONT_MAX, settings.settings.terminalFontSize + 1)
    if (next !== settings.settings.terminalFontSize)
      settings.patchSettings({ terminalFontSize: next })
  },
  /** 缩小字号（同步持久化） */
  zoomOut: () => {
    const next = Math.max(FONT_MIN, settings.settings.terminalFontSize - 1)
    if (next !== settings.settings.terminalFontSize)
      settings.patchSettings({ terminalFontSize: next })
  },
  /** 重置字号为默认值 14 */
  zoomReset: () => settings.patchSettings({ terminalFontSize: 14 }),
})
</script>

<style scoped>
.terminal-container {
  position: relative;
  width: 100%;
  height: 100%;
  padding: 4px;
  background-color: var(--jc-terminal-bg);
  /* 终端铺满整个面板（底部不留死区）；xterm 内部元素在极端尺寸下不外溢 */
  overflow: hidden;
}
</style>