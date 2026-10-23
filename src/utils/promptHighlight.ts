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
 * 提示符高亮：把会话输出里「行首的 shell 提示符」包上 ANSI 粗体，
 * 让 `root@host:~#` 这类提示符与普通输出区分开。
 *
 * 为什么把样式注入到输出流，而不是渲染完再改：
 *   远端 readline 每次按键、每次翻历史都会用 `\r` + PS1 + 输入行 重绘整行，
 *   事后补的样式会被下一次重绘直接抹掉；只有让粗体跟着 PS1 一起进流里，
 *   重绘之后才依然生效。
 *
 * 为什么安全（不会像插入可见字符那样顶坏光标）：
 *   SGR 序列（\x1b[1m / \x1b[22m）显示宽度为 0，
 *   xterm 与远端 shell 各自的列计算都不受影响，readline 的补全/历史重绘照常。
 */

const BOLD_ON = '\x1b[1m'
const BOLD_OFF = '\x1b[22m'

/** 可选前缀：虚拟环境名，如 `(venv) root@host:~/proj$ ` */
const VENV = '(?:\\([\\w.-]{1,24}\\)\\s+)?'

/** 行首提示符的候选形态，按顺序尝试，命中即止。
 *  刻意都不吃换行、也不匹配含转义的正文：
 *  带颜色的 PS1 本身已有区分度，且其内部的 \x1b[0m 会把我们加的粗体重置掉，
 *  强行加粗反而更乱，所以只处理「纯文本提示符」。 */
const PROMPT_PATTERNS: RegExp[] = [
  // user@host:path# / [user@host path]# / user@host path> —— bash / zsh / CentOS / macOS 常见 PS1
  new RegExp(`^${VENV}\\[?[\\w.-]{1,32}@[\\w.-]{1,64}[:\\s][^\\n\\x1b]{0,160}?\\]?[#$%>] ?`),
  // host:path$（PS1 里不带用户名的写法）
  new RegExp(`^${VENV}[\\w.-]{1,64}:[^\\n\\x1b]{0,160}?[#$%] ?`),
  // 纯符号提示符：只认 `$ ` / `% `（# 常出现在注释、> 常出现在引用行，误判代价不值得）
  new RegExp(`^${VENV}[$%] `),
  // Windows：PS C:\Users\用户名>  /  C:\Users\用户名>
  new RegExp(`^${VENV}(?:PS )?[A-Za-z]:\\\\[^\\n\\x1b]{0,160}?> ?`),
]

/** 从 i 处跳过行首的转义序列（如 bash 提示符前的 \x1b[?2004h 括号粘贴开关、光标显隐），
 *  返回可开始匹配提示符文本的下标；序列不完整时原样返回 i（本段不处理，等下一段）。 */
function skipEscapes(s: string, i: number): number {
  let p = i
  while (p < s.length && s[p] === '\x1b') {
    const next = s[p + 1]
    if (next === undefined) return i
    if (next === '[') {
      let q = p + 2
      while (q < s.length && !/[@-~]/.test(s[q])) q++
      if (q >= s.length) return i
      p = q + 1
    } else if (next === ']') {
      let q = p + 2
      while (q < s.length && s[q] !== '\x07' && !(s[q] === '\x1b' && s[q + 1] === '\\')) q++
      if (q >= s.length) return i
      p = s[q] === '\x07' ? q + 1 : q + 2
    } else {
      p += 2
    }
  }
  return p
}

/** 提示符片段的理论最长长度（含转义前缀），用于把每次匹配的扫描范围限制成常数，
 *  避免长输出（几千行）里每行都切片剩余全文导致 O(n²) */
const MATCH_WINDOW = 320

/** 尝试把 i 处（行首）识别成提示符，返回「含行首转义前缀在内」的片段长度；未命中返回 0 */
function matchPromptAt(s: string, i: number): number {
  const start = skipEscapes(s, i)
  const rest = s.slice(start, start + MATCH_WINDOW)
  for (const re of PROMPT_PATTERNS) {
    const m = re.exec(rest)
    if (m && m[0]) return start - i + m[0].length
  }
  return 0
}

/** 把一段已知的提示符文本包成粗体（用于前端自己重绘的仿真提示符） */
export function boldPrompt(prompt: string): string {
  return prompt ? `${BOLD_ON}${prompt}${BOLD_OFF}` : prompt
}

/**
 * 判断一行文本是否「整行就是一个空闲的 shell 提示符」（如 `root@host:~#`）。
 * 供「越底滚动」判断光标确实停在提示符处 —— 只在此时插空白行，
 * 不会插进正在输出的内容里；已输入命令的行（`root@host:~# ls -al`）不算。
 */
export function isIdlePromptLine(lineText: string): boolean {
  const s = lineText.replace(/\s+$/, '')
  if (!s) return false
  // 提示符可能本身以空格结尾（如 `$ `），缓冲区取文本会去掉行尾空白，故两种写法都比一次
  return matchPromptAt(s, 0) === s.length || matchPromptAt(s + ' ', 0) === s.length + 1
}

/** 会话输出的提示符高亮器：有状态（记住上一段是否停在行首）。
 *  每个终端实例一个，随实例生命周期存活即可。 */
export class PromptHighlighter {
  /** 上一段输出是否停在行首：决定下一段开头能不能当提示符匹配 */
  private atLineStart = true

  /** 处理一段「未经处理直接写入终端」的输出，仅同步行首状态，
   *  避免前端自己写的仿真提示符把状态判断带偏 */
  noteRaw(text: string): void {
    if (text) this.atLineStart = text.endsWith('\n') || text.endsWith('\r')
  }

  /** 处理一段会话输出；enabled 为 false 时原样返回（仍维护行首状态，便于随时开关） */
  transform(chunk: string, enabled: boolean): string {
    if (!chunk) return chunk
    const endsAtLineStart = chunk.endsWith('\n') || chunk.endsWith('\r')
    if (!enabled) {
      this.atLineStart = endsAtLineStart
      return chunk
    }

    let out = ''
    let i = 0
    while (i < chunk.length) {
      // 只在行首尝试匹配提示符
      if (this.atLineStart) {
        const len = matchPromptAt(chunk, i)
        if (len > 0) {
          out += BOLD_ON + chunk.slice(i, i + len) + BOLD_OFF
          i += len
          this.atLineStart = false
          continue
        }
      }
      const c = chunk[i]
      if (c === '\n' || c === '\r') {
        out += c
        i++
        this.atLineStart = true
        continue
      }
      // 普通正文：整段搬到下一个换行符，避免逐字符拼接
      let stop = chunk.length
      const nl = chunk.indexOf('\n', i)
      const cr = chunk.indexOf('\r', i)
      if (nl >= 0 && nl < stop) stop = nl
      if (cr >= 0 && cr < stop) stop = cr
      out += chunk.slice(i, stop)
      i = stop
    }
    this.atLineStart = endsAtLineStart
    return out
  }
}
