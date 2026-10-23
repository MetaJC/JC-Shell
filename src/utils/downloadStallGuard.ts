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
 * 下载 / 网络命令停滞看门狗
 * 作者：JC
 *
 * 终端里下载类命令遇到「源不可达」时有两种表现，本看门狗都要能兜住：
 * 1) 有进度条但进度不动（curl 不带 -s 时）：耗时计数继续涨、字节数不动 —— 假进度；
 * 2) 完全静默卡死（curl/wget 带 -s 时无进度输出）：屏幕上什么都没有，只是卡住不动。
 *
 * 判定：
 * - 「进度停滞」：解析进度行的数字签名（剔除耗时/ETA），长时间不前进即判定卡死。
 * - 「静默停滞」：识别网络类命令（curl/wget/git clone/npm install/apt…），
 *   命令发出后长时间无任何输出且尚未回到提示符，即判定卡死。
 *
 * 命中后回调上层（自动 Ctrl+C 中断 + 换源重试）。
 */

/** 「像下载进度」的输出特征（ETA 必须带词边界，否则 beta/metadata 等普通词会误命中） */
const PROGRESS_LIKE =
  /(--:--:--|Xferd|% Total|\d+K\/s|\d+M\/s|\d+G\/s|\bETA\b|\.\.\.\.\.)|\d+:\d+:\d+[^\r\n]*\d+:\d+:\d+/i

/**
 * 网络/下载类命令「正在执行」特征（用于静默卡死判定）。
 * 仅当命令位于行首或提示符字符（#$>%）之后才算武装 —— 此前用宽松的 \b 提及匹配，
 * README/日志/Agent 回显里只要出现 "npm install"、"curl" 等字样就会武装看门狗，
 * 用户没跑任何下载也会在 30s 静默后被误判卡死并强发 Ctrl+C。
 */
const NET_CMD_RUN =
  /(?:^|[#$>%]\s)\s*(?:sudo\s+|doas\s+)?(?:curl|wget|git\s+clone|npm\s+(?:install|i|ci)|pnpm\s+(?:install|add)|yarn\s+(?:add|install)|pip3?\s+install|apt(?:-get)?\s+(?:install|update|upgrade)|yum\s+install|dnf\s+install|docker\s+pull|go\s+(?:get|install)|brew\s+install)\b/i

/** 命令行提示符（用户回来等待输入）特征：去 ANSI 后匹配，且至少含一个字母以排除纯数字进度行 */
const PROMPT_LINE = /^[\w.@:\/~\\ -]{0,80}[#$>%]\s*$/

/** 去除 ANSI 转义序列 */
// eslint-disable-next-line no-control-regex
const ANSI_RE = /\x1b(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~]|\][^\x07]*(?:\x07|\x1b\\))/g

/** 去掉时间 / 耗时字段，仅保留进度数字，作为「进度是否前进」的签名 */
function progressSignature(line: string): string {
  return line
    .replace(/--:--:--/g, ' ')
    .replace(/\d+:\d+(?::\d+)?/g, ' ')
    .replace(/[^\d.\s]/g, ' ')
    .replace(/\s+/g, ' ')
    .trim()
}

export interface DownloadStallGuardOptions {
  /** 进度停滞多久判定为卡死（毫秒），默认 20000 */
  stallMs?: number
  /** 网络命令完全无输出多久判定为卡死（毫秒），默认 30000 */
  silentMs?: number
  /** 定时检查间隔（毫秒），默认 3000 */
  checkIntervalMs?: number
  /** 判定停滞时回调（参数为最后一条进度/命令输出，便于日志 / 提示） */
  onStall: (lastLine: string) => void
}

export class DownloadStallGuard {
  private readonly stallMs: number
  private readonly silentMs: number
  private readonly onStall: (line: string) => void

  /** 未按 \r / \n 结束的残留片段（行可能被拆包） */
  private tail = ''
  /** 最近一次看到任何输出的时间 */
  private lastOutputAt = 0
  /** 是否检测到「有进度条」 */
  private progressActive = false
  /** 最近一次看到「进度行」的时间（用于判断这段下载是否已结束） */
  private lastProgressSeenAt = 0
  /** 最近一次进度真正前进的时间 */
  private lastAdvanceAt = 0
  /** 最近一次进度签名 */
  private lastSignature = ''
  /** 最近一条进度输出 */
  private lastLine = ''
  /** 是否处于「网络命令执行中」（尚未回到提示符） */
  private netPending = false
  /** 网络命令发出的时间 */
  private netPendingAt = 0
  private timer: ReturnType<typeof setInterval> | null = null
  private disposed = false

  constructor(opts: DownloadStallGuardOptions) {
    this.stallMs = opts.stallMs && opts.stallMs > 0 ? opts.stallMs : 20000
    this.silentMs = opts.silentMs && opts.silentMs > 0 ? opts.silentMs : 30000
    const interval = opts.checkIntervalMs && opts.checkIntervalMs > 0 ? opts.checkIntervalMs : 3000
    this.onStall = opts.onStall
    this.timer = setInterval(() => this.tick(), interval)
  }

  /** 是否正在监视一段下载 / 网络命令 */
  get isActive(): boolean {
    return this.progressActive || this.netPending
  }

  /** 喂入终端输出片段 */
  feed(chunk: string): void {
    if (this.disposed || !chunk) return
    const now = Date.now()
    this.lastOutputAt = now
    this.tail += chunk
    // 防御性截断：异常大块输出不无限增长
    if (this.tail.length > 16384) this.tail = this.tail.slice(-8192)
    const parts = this.tail.split(/[\r\n]/)
    this.tail = parts.pop() ?? ''
    for (const part of parts) this.consume(part, now)
    // shell 提示符通常【不带换行】就停住等待输入，会一直留在未完成行里；
    // 若尾部正是提示符，说明命令已结束，清掉「执行中」状态（否则会误判为静默卡死）
    const tail = this.tail.replace(ANSI_RE, '')
    if (tail && /[A-Za-z]/.test(tail) && PROMPT_LINE.test(tail)) {
      this.netPending = false
      this.progressActive = false
      this.lastSignature = ''
    }
  }

  dispose(): void {
    this.disposed = true
    if (this.timer) {
      clearInterval(this.timer)
      this.timer = null
    }
    this.reset()
  }

  private consume(rawLine: string, now: number): void {
    if (!rawLine) return
    const line = rawLine.replace(ANSI_RE, '')
    if (!line) return

    // 提示符出现 = 命令结束，清掉「网络命令执行中」状态
    if (PROMPT_LINE.test(line) && /[A-Za-z]/.test(line)) {
      this.netPending = false
      this.progressActive = false
      this.lastSignature = ''
      return
    }

    if (NET_CMD_RUN.test(line)) {
      this.netPending = true
      this.netPendingAt = now
    }

    if (PROGRESS_LIKE.test(line)) {
      this.progressActive = true
      this.lastProgressSeenAt = now
      this.lastLine = line
      const sig = progressSignature(line)
      if (sig !== this.lastSignature) {
        this.lastSignature = sig
        this.lastAdvanceAt = now
      }
    }
  }

  private tick(): void {
    if (this.disposed) return
    const now = Date.now()
    // 很久没再出现「进度行」→ 这段下载已结束（或输出类型变了）：结束进度监视。
    // 不做这步的话，下载正常结束后 progressActive 会一直挂起，20s 后误判为卡死并打断用户。
    // 用 >= 且先于下面的停滞判断执行，确保与停滞阈值同刻时优先判定为"已结束"而非"卡死"。
    if (this.progressActive && now - this.lastProgressSeenAt >= this.stallMs) {
      this.progressActive = false
      this.lastSignature = ''
    }
    // 进度停滞
    if (this.progressActive && now - this.lastAdvanceAt >= this.stallMs) {
      const line = this.lastLine
      this.reset()
      this.onStall(line)
      return
    }
    // 静默停滞：网络命令已发出但长时间没有任何输出，且未回到提示符（保护期 5 分钟）
    if (this.netPending && now - this.netPendingAt <= 5 * 60_000
      && now - this.lastOutputAt >= this.silentMs) {
      this.reset()
      this.onStall(`（网络命令静默无输出 ${Math.round((now - this.lastOutputAt) / 1000)}s）`)
    }
  }

  private reset(): void {
    this.progressActive = false
    this.lastProgressSeenAt = 0
    this.lastSignature = ''
    this.lastLine = ''
    this.netPending = false
    this.tail = ''
  }
}
