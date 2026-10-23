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
 * Electerm 数据文件（electerm.json / 导出的 JSON）解析
 * 作者：JC
 *
 * 结构（各版本略有差异）：
 *   {
 *     connections: [ { id, title, host, port, username, password, privateKey, privateKeyPath, authType, groupId } ],
 *     connectionGroups: [ { id, title, parentId } ]
 *   }
 * 私钥：privateKey 存的是密钥内容，可直接托管成 keyContent（原文件删除也不影响）。
 * 口令：用户若设过主密码则为密文，无法与明文区分，故一律不迁移。
 */
import type { NormSession, ParseResult } from './types'
import { cleanName, parsePort, pickField, validHost } from './types'

/** 非 SSH 类型，跳过（Electerm 也支持 telnet/serial/rdp 等） */
const NON_SSH_TYPES = ['telnet', 'serial', 'rdp', 'vnc', 'ftp', 'local']

function asArray(v: unknown): unknown[] {
  return Array.isArray(v) ? v : []
}
function asRecord(v: unknown): Record<string, unknown> {
  return v && typeof v === 'object' && !Array.isArray(v) ? (v as Record<string, unknown>) : {}
}

export function parseElecterm(text: string): ParseResult {
  let root: unknown
  try {
    root = JSON.parse(text)
  } catch {
    return { sessions: [], warnings: ['Electerm 数据不是合法的 JSON'] }
  }

  // 有的版本把内容包在 data 字段里
  const outer = asRecord(root)
  const data = 'data' in outer ? asRecord(outer.data) : outer
  const conns = asArray(data.connections).length ? asArray(data.connections) : asArray(data.bookmarks)
  const groups = asArray(data.connectionGroups)

  // groupId -> { name, parentId }，用于还原文件夹层级
  const groupMap = new Map<string, { name: string; parentId: string }>()
  for (const g of groups) {
    const r = asRecord(g)
    const id = String(r.id ?? '')
    if (!id) continue
    groupMap.set(id, {
      name: cleanName(String(pickField(r, ['title', 'name', 'label']) || '未命名分组')),
      parentId: String(r.parentId ?? ''),
    })
  }
  const folderPathOf = (groupId: string): string[] => {
    const out: string[] = []
    let cur = groupId
    const guard = new Set<string>()
    while (cur && groupMap.has(cur) && !guard.has(cur)) {
      guard.add(cur)
      const g = groupMap.get(cur)!
      out.unshift(g.name)
      cur = g.parentId
    }
    return out
  }

  const sessions: NormSession[] = []
  const warnings: string[] = []
  const seen = new Set<string>()

  for (const c of conns) {
    const r = asRecord(c)
    const type = String(pickField(r, ['type']) || 'ssh').toLowerCase()
    if (NON_SSH_TYPES.includes(type)) {
      warnings.push(`跳过非 SSH 会话：${String(pickField(r, ['title', 'name', 'host']) || type)}`)
      continue
    }

    const host = pickField(r, ['host', 'hostname', 'hostName', 'address', 'ip', 'server'])
    if (!validHost(host)) continue

    const username = pickField(r, ['username', 'user', 'login', 'loginName']) || 'root'
    const port = parsePort(pickField(r, ['port']) || 22)
    const key = `${host}:${port}:${username}`
    if (seen.has(key)) continue
    seen.add(key)

    const privateKey = pickField(r, ['privateKey', 'privateKeyContent', 'key'])
    const keyPath = pickField(r, ['privateKeyPath', 'keyPath'])
    const authRaw = String(pickField(r, ['authType', 'auth']) || '').toLowerCase()
    const authType: NormSession['authType'] =
      authRaw.includes('key') || !!privateKey || !!keyPath ? 'key' : 'password'

    sessions.push({
      name: cleanName(String(pickField(r, ['title', 'name', 'label', 'alias']) || host), host),
      host,
      port,
      username,
      authType,
      keyContent: privateKey || undefined,
      keyPath: keyPath || undefined,
      folderPath: folderPathOf(String(r.groupId ?? '')),
      source: 'electerm',
      note: privateKey || keyPath ? undefined : 'Electerm 口令未迁移，请手动补全',
    })
  }

  return { sessions, warnings }
}
