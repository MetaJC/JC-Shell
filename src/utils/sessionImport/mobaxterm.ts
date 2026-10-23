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
 * MobaXterm 会话解析（MobaXterm.ini / .mxtsessions 的 [Bookmarks] 段）
 * 作者：JC
 *
 * MobaXterm 有两代写法，这里都兼容：
 *
 * A) 新格式 / 导出格式（实测最常见）：
 *    SubRep=<子目录>          ← 其后的会话都归属该目录
 *    ImgNum=42
 *    192.168.1.10 (root)=#109#0%192.168.1.10%22%root%%-1%-1%%%22%%...#MobaFont%...
 *    字段以 % 分隔：<子类型0> % <主机> % <端口> % <用户名> % <密码> % ...
 *
 * B) 旧 ini 格式：
 *    Bookmark1= #109#0#1#root@host:22#文件夹\名称#备注#
 *
 * 口令不迁移：MobaXterm 的口令是密文且与本机绑定。
 */
import type { NormSession, ParseResult } from './types'
import { DEFAULT_SSH_PORT, cleanName, parsePort, validHost } from './types'

/** 明确非 SSH 的会话，跳过 */
const NON_SSH = ['rdp', 'vnc', 'telnet', 'serial', 'rlogin', 'mosh', 'ftp']

function splitFolderPath(s: string): string[] {
  return s
    .split(/[\\/]+/)
    .map((x) => x.trim())
    .filter(Boolean)
}

/** 是否是像主机名的字段（排除 '0'、颜色 '236,236,236' 这类） */
function isHostLike(s: string): boolean {
  const t = (s || '').trim()
  return t.length > 0 && !/\s/.test(t) && !t.includes(',')
}

export function parseMobaXterm(text: string): ParseResult {
  const sessions: NormSession[] = []
  const warnings: string[] = []
  const lines = text.split(/\r?\n/)

  let inBookmarks = false
  let baseFolder: string[] = []

  for (const raw of lines) {
    const line = raw.trim()
    if (line.startsWith('[')) {
      inBookmarks = line.toLowerCase().startsWith('[bookmarks')
      if (!inBookmarks) baseFolder = []
      continue
    }
    if (!line) continue

    // 段名没识别出来也兜底：形如 xxx=#109#0%... 的行直接当会话解析
    const looksLikeSession = /=\s*#\d+#\d+%/.test(line)
    if (!inBookmarks && !looksLikeSession) continue

    // 分组声明：SubRep=<相对目录>，其后的会话归属该目录
    const sub = line.match(/^subrep\s*=\s*(.*)$/i)
    if (sub) {
      baseFolder = splitFolderPath(sub[1])
      continue
    }
    if (/^imgnum\s*=/i.test(line)) continue

    const eq = line.indexOf('=')
    if (eq <= 0) continue
    const key = line.slice(0, eq).trim()
    const value = line.slice(eq + 1).trim()
    if (!value) continue

    const lower = value.toLowerCase()
    if (NON_SSH.some((p) => lower.includes(p))) {
      warnings.push(`跳过非 SSH 会话：${key || value.slice(0, 40)}`)
      continue
    }

    const parsed = parseSessionLine(key, value, baseFolder)
    if (parsed) sessions.push(parsed)
    // 只对「看起来就是 SSH（#109#）」却解析失败的行告警；其它协议静默跳过
    else if (value.includes('#109#')) warnings.push(`无法解析会话：${key || value.slice(0, 40)}`)
  }

  return { sessions, warnings }
}

function parseSessionLine(key: string, value: string, baseFolder: string[]): NormSession | null {
  const hash = value.split('#')
  let host = ''
  let port = DEFAULT_SSH_PORT
  let username = ''

  // 格式 A：#<类型>#<子类型0>%<主机>%<端口>%<用户>%<密码>%...
  // 类型 109 = SSH；其它数字类型（本地/WSL/RDP 等）一律不当成 SSH
  if (hash.length >= 3 && hash[1].trim() === '109') {
    const cf = hash[2].split('%')
    // cf[0] 是子类型(0)，主机/端口/用户名在 1/2/3
    if (cf.length >= 4 && isHostLike(cf[1])) {
      host = cf[1].trim()
      port = parsePort(cf[2])
      username = (cf[3] || '').trim()
    }
  }

  // 格式 B：...#<类型>#...<user>@<host>:<port>#...
  if (!host) {
    const conn = hash.map((s) => s.trim()).find((p) => p.includes('@') && !p.includes('%'))
    if (conn) {
      const clean = conn.replace(/%/g, '')
      const m = clean.match(/^([^@]+)@([^:@]+)(?::(\d+))?$/)
      if (m) {
        username = m[1].trim()
        host = m[2].trim()
        if (m[3]) port = parsePort(m[3])
      }
    }
  }

  if (!validHost(host)) return null

  // 名称 / 目录：优先 key（旧格式的 key 是 BookmarkN，无意义则忽略），再退回 value 里带 \ 的段
  let name = ''
  let folderPath = [...baseFolder]
  const keyIsGeneric = /^bookmark\d+$/i.test(key)

  if (key && !keyIsGeneric) {
    if (key.includes('\\')) {
      const segs = splitFolderPath(key)
      name = segs[segs.length - 1] || ''
      folderPath = [...baseFolder, ...segs.slice(0, -1)]
    } else {
      name = key
    }
  }
  if (!name) {
    const pathSeg = hash.map((s) => s.trim()).find((p) => p.includes('\\'))
    if (pathSeg) {
      const segs = splitFolderPath(pathSeg)
      if (segs.length > 0) {
        name = segs[segs.length - 1]
        folderPath = [...baseFolder, ...segs.slice(0, -1)]
      }
    }
  }
  if (!name) name = host

  return {
    name: cleanName(name, host),
    host,
    port,
    username: username || 'root',
    authType: 'password',
    folderPath,
    source: 'mobaxterm',
    note: 'MobaXterm 口令为密文，未迁移，请手动补全',
  }
}
