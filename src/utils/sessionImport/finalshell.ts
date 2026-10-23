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
 * FinalShell 会话解析（conn 目录）
 * 作者：JC
 *
 * 目录结构（一个文件一条记录，靠 parent_id 组织层级）：
 *   conn/<connId>_connect_config.json              根目录下的会话
 *   conn/<folderId>/folder.json                    文件夹：{ id, name, parent_id }
 *   conn/<folderId>/<connId>_connect_config.json   文件夹内的会话（parent_id = 文件夹 id）
 *
 * 会话 JSON 字段：name / host / port / user_name / parent_id /
 *   conection_type（100 = SSH，其它为 RDP/VNC 等）/ password（DES-ECB 密文）
 *
 * 因此「文件夹名」必须跨文件解析：先读所有 folder.json 建 id→名称映射，再解析会话。
 * 口令为密文，不迁移。
 */
import type { NormSession, ParseResult } from './types'
import { cleanName, parsePort, validHost } from './types'

/** FinalShell 中 conection_type = 100 表示 SSH */
const FINALSHELL_SSH_TYPE = 100

function asRecord(v: unknown): Record<string, unknown> {
  return v && typeof v === 'object' && !Array.isArray(v) ? (v as Record<string, unknown>) : {}
}

/**
 * 解析一个 FinalShell conn 目录下的全部记录。
 * @param folderTexts folder.json 内容
 * @param connTexts   *_connect_config.json 内容
 */
export function parseFinalShell(folderTexts: string[], connTexts: string[]): ParseResult {
  const warnings: string[] = []

  // 1) 文件夹 id -> { 名称, 父文件夹 }
  const folderMap = new Map<string, { name: string; parentId: string }>()
  for (const t of folderTexts) {
    try {
      const j = asRecord(JSON.parse(t))
      const id = String(j.id ?? '').trim()
      if (!id) continue
      folderMap.set(id, {
        name: String(j.name ?? '').trim(),
        parentId: String(j.parent_id ?? '').trim(),
      })
    } catch {
      // 单个 folder.json 坏了不影响其它会话
    }
  }

  /** 由 parent_id 一路向上还原文件夹层级（含防环） */
  const folderPathOf = (parentId: string): string[] => {
    const out: string[] = []
    let cur = parentId
    const guard = new Set<string>()
    while (cur && cur !== 'root' && folderMap.has(cur) && !guard.has(cur)) {
      guard.add(cur)
      const f = folderMap.get(cur)!
      if (f.name) out.unshift(cleanName(f.name, '未命名文件夹'))
      cur = f.parentId
    }
    return out
  }

  // 2) 会话
  const sessions: NormSession[] = []
  for (const t of connTexts) {
    let j: Record<string, unknown>
    try {
      j = asRecord(JSON.parse(t))
    } catch {
      warnings.push('有会话文件 JSON 解析失败，已跳过')
      continue
    }

    const host = String(j.host ?? '').trim()
    if (!validHost(host)) continue

    // 非 SSH（RDP/VNC/telnet 等）跳过
    const ctype = j.conection_type ?? j.connection_type
    if (typeof ctype === 'number' && ctype !== FINALSHELL_SSH_TYPE) continue

    const port = parsePort(j.port ?? 22)
    const username = String(j.user_name ?? j.userName ?? '').trim() || 'root'
    const name = String(j.name ?? '').trim() || host

    sessions.push({
      name: cleanName(name, host),
      host,
      port,
      username,
      authType: 'password',
      folderPath: folderPathOf(String(j.parent_id ?? '').trim()),
      source: 'finalshell',
      note: 'FinalShell 口令为密文，未迁移，请手动补全',
    })
  }

  if (sessions.length === 0) {
    warnings.push('未在 FinalShell 数据中识别到 SSH 会话')
  }
  return { sessions, warnings }
}
