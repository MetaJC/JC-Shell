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
 * jc-shell 自有导出/导入格式 —— 唯一支持密码迁移的格式
 * 作者：JC
 *
 * 导出：密码统一先用 crypto.encryptText 加密再写入文件，避免明文口令散落在备份里；
 * 导入：检测到加密串自动解密，再交给 host store（store 会按「凭证加密」开关自行落盘加密）。
 * 外部客户端格式一律不迁移密码（各家均为密文/钥匙串，取出来也不能用）。
 */
import type { NormSession, ParseResult } from './types'
import { splitHostPort, validHost } from './types'
import { decryptText, encryptText, isEncrypted } from '@/utils/crypto'

export const JC_EXPORT_FORMAT = 'jc-shell-sessions'
export const JC_EXPORT_VERSION = 1

export interface JcNodeConfig {
  username?: string
  port?: number
  authType?: 'password' | 'key'
  password?: string
  keyPath?: string
  keyContent?: string
  keyName?: string
  passphrase?: string
}

export interface JcExportNode {
  name: string
  type: 'host' | 'folder'
  addr?: string
  config?: JcNodeConfig
  children?: JcExportNode[]
}

export interface JcExportDoc {
  format: typeof JC_EXPORT_FORMAT
  version: number
  exportedAt: string
  namespace: string
  includePasswords: boolean
  nodes: JcExportNode[]
}

/** 待导出的树节点（结构与 host store 的 HostItem 树兼容） */
export interface ExportSourceNode {
  name: string
  type: 'host' | 'folder'
  addr?: string
  config?: JcNodeConfig
  children?: ExportSourceNode[]
}

/** 会话树 -> 导出文档；includePasswords=false 剔除密码，true 则加密后写入 */
export async function buildExportDoc(
  nodes: ExportSourceNode[],
  namespace: string,
  includePasswords: boolean,
): Promise<JcExportDoc> {
  async function conv(n: ExportSourceNode): Promise<JcExportNode> {
    const node: JcExportNode = { name: n.name, type: n.type }
    if (n.type === 'host') {
      node.addr = n.addr || ''
      if (n.config) {
        const c: JcNodeConfig = { ...n.config }
        if (includePasswords && c.password) {
          c.password = await encryptText(c.password)
        } else {
          delete c.password
        }
        node.config = c
      }
    }
    if (n.children && n.children.length > 0) {
      node.children = []
      for (const ch of n.children) node.children.push(await conv(ch))
    }
    return node
  }

  const out: JcExportNode[] = []
  for (const n of nodes) out.push(await conv(n))
  return {
    format: JC_EXPORT_FORMAT,
    version: JC_EXPORT_VERSION,
    exportedAt: new Date().toISOString(),
    namespace,
    includePasswords,
    nodes: out,
  }
}

/** 导出文档 -> 归一化会话（还原文件夹层级与密码） */
export async function parseJcExport(text: string): Promise<ParseResult> {
  let doc: JcExportDoc
  try {
    doc = JSON.parse(text) as JcExportDoc
  } catch {
    return { sessions: [], warnings: ['不是合法的 jc-shell 导出文件（JSON 解析失败）'] }
  }
  if (!doc || doc.format !== JC_EXPORT_FORMAT) {
    return { sessions: [], warnings: ['不是 jc-shell 会话导出文件'] }
  }

  const sessions: NormSession[] = []
  const warnings: string[] = []

  async function walk(nodes: JcExportNode[], folderPath: string[]): Promise<void> {
    for (const n of nodes) {
      if (n.type === 'folder') {
        await walk(n.children ?? [], [...folderPath, n.name])
        continue
      }
      const { host, port } = splitHostPort(n.addr || '', 22)
      if (!validHost(host)) {
        warnings.push(`跳过无效会话：${n.name || '(无名)'}`)
        continue
      }
      const cfg = n.config ?? {}
      let password = cfg.password
      if (password && isEncrypted(password)) {
        try {
          password = await decryptText(password)
        } catch {
          password = undefined
          warnings.push(`${n.name}：密码解密失败，已留空`)
        }
      }
      sessions.push({
        name: n.name || host,
        host,
        port: cfg.port || port,
        username: cfg.username || 'root',
        authType: cfg.authType === 'key' ? 'key' : 'password',
        password,
        keyPath: cfg.keyPath,
        keyContent: cfg.keyContent,
        passphrase: cfg.passphrase,
        folderPath,
        source: 'jc',
      })
    }
  }

  await walk(doc.nodes ?? [], [])
  return { sessions, warnings }
}
