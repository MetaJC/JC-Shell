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
 * 通用 JSON 会话解析
 * 作者：JC
 *
 * 用途：Termius / FinalShell 等客户端没有稳定的公开格式，但它们「导出的 JSON」
 * 通常都是「对象数组 + 主机类字段」。这里不做硬编码猜测，而是递归扫描 JSON，
 * 找出"看起来像会话列表"的数组并映射，从而覆盖大多数未知/导出格式。
 *
 * 密码不迁移：这些客户端的本地库都是加密的，导出文件里的口令即便存在也多为密文。
 */
import type { NormSession, ParseResult } from './types'
import { cleanName, parsePort, pickField, validHost } from './types'

const HOST_KEYS = ['host', 'hostname', 'hostName', 'address', 'ip', 'server', 'addr', 'serverHost', 'remoteHost']
const USER_KEYS = ['username', 'user', 'login', 'loginName', 'login_name', 'account', 'userName', 'user_name']
const NAME_KEYS = ['title', 'name', 'label', 'alias', 'displayName', 'description']
const PORT_KEYS = ['port', 'sshPort', 'remotePort']
const KEY_KEYS = ['privateKey', 'privateKeyPath', 'keyPath', 'identityFile', 'key']
const FOLDER_KEYS = ['group', 'folder', 'groupName', 'category', 'parent']

function asRecord(v: unknown): Record<string, unknown> {
  return v && typeof v === 'object' && !Array.isArray(v) ? (v as Record<string, unknown>) : {}
}

/** 数组是否"看起来像会话列表"：抽样前若干项，多半带主机字段即认为命中 */
function looksLikeConnections(arr: unknown[]): boolean {
  const sample = arr.slice(0, 5).filter((x) => x && typeof x === 'object')
  if (sample.length === 0) return false
  const hit = sample.filter((x) => pickField(asRecord(x), HOST_KEYS)).length
  return hit * 2 >= sample.length
}

function collectConnectionArrays(node: unknown, out: unknown[][], depth: number): void {
  if (depth > 8) return
  if (Array.isArray(node)) {
    if (node.length > 0 && looksLikeConnections(node)) out.push(node)
    else for (const it of node) collectConnectionArrays(it, out, depth + 1)
    return
  }
  if (node && typeof node === 'object') {
    const rec = node as Record<string, unknown>
    // 单条连接对象：部分客户端（如 FinalShell）一个文件只存一条连接
    if (pickField(rec, HOST_KEYS)) {
      out.push([node])
      return
    }
    for (const v of Object.values(rec)) {
      collectConnectionArrays(v, out, depth + 1)
    }
  }
}

/** 文件夹名：可能是字符串，也可能是 {title} 对象 */
function folderNameOf(v: unknown): string {
  if (typeof v === 'string' && v.trim()) return cleanName(v, '未命名分组')
  if (v && typeof v === 'object') {
    const r = asRecord(v)
    const t = pickField(r, ['title', 'name', 'label'])
    if (t) return cleanName(t, '未命名分组')
  }
  return ''
}

export function parseGenericJson(text: string): ParseResult {
  let root: unknown
  try {
    root = JSON.parse(text)
  } catch {
    return { sessions: [], warnings: ['文件不是合法的 JSON'] }
  }

  const buckets: unknown[][] = []
  collectConnectionArrays(root, buckets, 0)

  const sessions: NormSession[] = []
  const warnings: string[] = []
  const seen = new Set<string>()

  for (const arr of buckets) {
    for (const item of arr) {
      const r = asRecord(item)
      const host = pickField(r, HOST_KEYS)
      if (!validHost(host)) continue

      const type = String(pickField(r, ['type', 'protocol']) || 'ssh').toLowerCase()
      if (['telnet', 'serial', 'rdp', 'vnc', 'ftp'].includes(type)) continue
      // FinalShell：conection_type=100 表示 SSH，其它数值是 RDP/VNC/telnet 等，跳过
      const ctype = r.conection_type ?? r.connection_type
      if (typeof ctype === 'number' && ctype !== 100) continue

      const username = pickField(r, USER_KEYS) || 'root'
      const { host: h, port } = (() => {
        const p = parsePort(pickField(r, PORT_KEYS) || 22)
        return { host, port: p }
      })()
      const key = `${h}:${port}:${username}`
      if (seen.has(key)) continue
      seen.add(key)

      const keyRef = pickField(r, KEY_KEYS)
      const folderRaw = pickField(r, FOLDER_KEYS) || folderNameOf(r.group ?? r.folder ?? r.parent)
      // 私钥字段里看起来像 PEM 内容的当作内容，否则当路径
      const isPem = keyRef.includes('PRIVATE KEY') || keyRef.includes('-----BEGIN')

      sessions.push({
        name: cleanName(pickField(r, NAME_KEYS) || h, h),
        host: h,
        port,
        username,
        authType: keyRef ? 'key' : 'password',
        keyContent: keyRef && isPem ? keyRef : undefined,
        keyPath: keyRef && !isPem ? keyRef : undefined,
        folderPath: folderRaw ? [folderRaw] : [],
        source: 'unknown',
        note: '口令未迁移，请手动补全',
      })
    }
  }

  if (sessions.length === 0) {
    warnings.push('未在该 JSON 中识别到会话条目，请确认文件是否为导出/备份文件')
  }
  return { sessions, warnings }
}
