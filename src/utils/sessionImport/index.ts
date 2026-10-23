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
 * 跨软件会话导入/导出：对外入口
 * 作者：JC
 *
 * 职责分层：
 *   Rust（session_import_cmd）  —— 选文件/选目录、读本地文本、列目录、探测已安装客户端
 *   本目录各 parser            —— 把各家格式解析成统一的 NormSession
 *   index.ts（本文件）          —— 格式探测、编排读取、把 NormSession 落到 host store
 *
 * 密码策略：外部客户端的口令一律不迁移（均为密文/钥匙串，取出来也不能用），
 * 只有 jc-shell 自有导出格式支持密码迁移。
 */
import { invoke } from '@/api'
import { useHostStore, type HostItem } from '@/stores/host'
import type { DetectedClient, ImportClientId, ImportFileEntry, NormSession, ParseResult } from './types'
import { joinAddr, splitHostPort } from './types'
import { parseXshell } from './xshell'
import { parseMobaXterm } from './mobaxterm'
import { parseElecterm } from './electerm'
import { parseGenericJson } from './genericJson'
import { parseFinalShell } from './finalshell'
import { JC_EXPORT_FORMAT, buildExportDoc, parseJcExport, type ExportSourceNode } from './jc'

// ===== Rust 原语封装 =====

/** 探测本机已安装的 SSH 客户端及其会话存储位置 */
export function scanInstalledClients(): Promise<DetectedClient[]> {
  return invoke<DetectedClient[]>('scan_installed_clients')
}
/** 弹出「打开文件」对话框，返回绝对路径；取消返回 null */
export function pickImportFile(title: string, filters?: string[]): Promise<string | null> {
  return invoke<string | null>('pick_import_file', { title, filters: filters ?? null })
}
/** 弹出「选择文件夹」对话框 */
export function pickImportFolder(title: string): Promise<string | null> {
  return invoke<string | null>('pick_import_folder', { title })
}
/** 读取本地文本（webview 无权读任意路径；Rust 侧已处理 UTF-16/BOM） */
export function readLocalTextFile(path: string): Promise<string> {
  return invoke<string>('read_local_text_file', { path })
}
/** 列出目录下（含子目录）匹配扩展名的文件 */
export function listImportFiles(dir: string, extensions?: string[]): Promise<ImportFileEntry[]> {
  return invoke<ImportFileEntry[]>('list_import_files', { dir, extensions: extensions ?? null })
}
/** 弹出保存对话框 */
export function pickSavePath(title: string, defaultName: string): Promise<string | null> {
  return invoke<string | null>('pick_save_path', { title, defaultName })
}
/** 写入文本文件 */
export function saveTextFile(path: string, content: string): Promise<void> {
  return invoke('save_text_file', { path, content })
}

// ===== 工具 =====

function normSep(s: string): string {
  return s.replace(/\\/g, '/')
}
function baseName(p: string): string {
  const s = normSep(p)
  return s.slice(s.lastIndexOf('/') + 1)
}
/** 文件相对根目录的所在子目录（用于还原 Xshell 会话目录里的层级） */
function relativeDir(path: string, root: string): string[] {
  const p = normSep(path)
  const r = normSep(root).replace(/\/+$/, '')
  if (!r || !p.startsWith(r)) return []
  const rest = p.slice(r.length).replace(/^\//, '')
  const segs = rest.split('/')
  segs.pop() // 去掉文件名
  return segs.filter(Boolean)
}

// ===== 格式探测 =====

/** 按内容（而非仅扩展名）判断属于哪家格式 */
export function detectFormat(text: string, fileName = ''): ImportClientId {
  const lowerName = fileName.toLowerCase()
  const trimmed = text.trimStart()

  // 1) JSON 对象：只认以 { 开头。不能把 .ini 的 [section] 也当 JSON 试，
  //    否则 MobaXterm.ini 会被误判成 unknown 而走通用解析。
  if (trimmed.startsWith('{')) {
    try {
      const o = JSON.parse(text) as Record<string, unknown>
      if (o && o.format === JC_EXPORT_FORMAT) return 'jc'
      const data = o && typeof o.data === 'object' && o.data ? (o.data as Record<string, unknown>) : o
      if (data && (Array.isArray(data.connections) || Array.isArray(data.connectionGroups))) return 'electerm'
      return 'unknown'
    } catch {
      return 'unknown'
    }
  }
  // 2) MobaXterm：含 [Bookmarks] 段
  if (/\[bookmarks/i.test(text)) return 'mobaxterm'
  // 3) Xshell：.xsh 文件或含 [CONNECTION] 段
  if (lowerName.endsWith('.xsh') || /\[connection/i.test(text)) return 'xshell'
  // 4) 纯 JSON 数组（部分导出格式）→ 交给通用解析
  if (trimmed.startsWith('[')) {
    try {
      JSON.parse(text)
      return 'unknown'
    } catch {
      // 不是合法 JSON，继续按 ini 处理
    }
  }
  if (lowerName.endsWith('.ini')) return 'mobaxterm'
  return 'unknown'
}

/** 按指定格式解析；folderPath 为外层目录层级（从客户端会话目录导入时使用） */
export async function parseByFormat(
  text: string,
  id: ImportClientId,
  displayName: string,
  folderPath: string[] = [],
): Promise<ParseResult> {
  let r: ParseResult
  switch (id) {
    case 'jc':
      r = await parseJcExport(text)
      break
    case 'xshell':
      r = parseXshell(text, displayName.replace(/\.xsh$/i, ''))
      break
    case 'mobaxterm':
      r = parseMobaXterm(text)
      break
    case 'electerm':
      r = parseElecterm(text)
      break
    default:
      r = parseGenericJson(text)
  }
  if (folderPath.length > 0) {
    r.sessions = r.sessions.map((s) => ({ ...s, folderPath: [...folderPath, ...s.folderPath] }))
  }
  return r
}

// ===== 采集：从已安装客户端 / 从文件 / 从目录 =====

/** 解析一组文件，并按其相对 root 的子目录补上文件夹层级 */
async function parseFiles(files: ImportFileEntry[], root: string): Promise<ParseResult> {
  const sessions: NormSession[] = []
  const warnings: string[] = []
  for (const f of files) {
    let text: string
    try {
      text = await readLocalTextFile(f.path)
    } catch (e) {
      warnings.push(`${f.name}：读取失败（${String(e)}）`)
      continue
    }
    const id = detectFormat(text, f.name)
    const r = await parseByFormat(text, id, f.name, relativeDir(f.path, root))
    sessions.push(...r.sessions)
    warnings.push(...r.warnings)
  }
  return { sessions, warnings }
}

/**
 * FinalShell 需要跨文件解析：folder.json 提供「文件夹 id → 名称」，
 * 会话文件的 parent_id 指向文件夹，不能逐个文件独立解析。
 */
async function parseFinalShellFiles(files: ImportFileEntry[]): Promise<ParseResult> {
  const folderTexts: string[] = []
  const connTexts: string[] = []
  const warnings: string[] = []
  for (const f of files) {
    const base = f.name.toLowerCase()
    if (base !== 'folder.json' && !base.endsWith('connect_config.json')) continue
    try {
      const text = await readLocalTextFile(f.path)
      if (base === 'folder.json') folderTexts.push(text)
      else connTexts.push(text)
    } catch (e) {
      warnings.push(`${f.name}：读取失败（${String(e)}）`)
    }
  }
  const r = parseFinalShell(folderTexts, connTexts)
  return { sessions: r.sessions, warnings: [...warnings, ...r.warnings] }
}

/** 是否像是 FinalShell 的 conn 目录（含 *_connect_config.json） */
function looksLikeFinalShell(files: ImportFileEntry[]): boolean {
  return files.some((f) => /connect_config\.json$/i.test(f.name))
}

/** 从「一键导入」探测到的客户端采集会话 */
export async function collectFromClient(client: DetectedClient): Promise<ParseResult> {
  if (client.kind === 'db') {
    return { sessions: [], warnings: [`${client.name}：${client.note}`] }
  }
  if (client.kind === 'file') {
    const text = await readLocalTextFile(client.path)
    const id = client.id === 'mobaxterm' ? 'mobaxterm' : detectFormat(text, client.path)
    return await parseByFormat(text, id, baseName(client.path))
  }
  // folder：Xshell 只认 .xsh，其它客户端放宽到常见文本/JSON
  const exts = client.id === 'xshell' ? ['xsh'] : ['json', 'ini', 'conf', 'txt', 'xsh']
  const files = await listImportFiles(client.path, exts)
  if (client.id === 'finalshell') return await parseFinalShellFiles(files)
  return await parseFiles(files, client.path)
}

/** 从用户选择的文件导入 */
export async function collectFromFile(path: string): Promise<ParseResult> {
  const text = await readLocalTextFile(path)
  const id = detectFormat(text, path)
  return await parseByFormat(text, id, baseName(path))
}

/**
 * 从「已拿到文本内容」的文件导入。
 * 移动端 WebView 用 `<input type="file">` 选文件时，内容已在 JS 侧，
 * 没有可读的本地路径（SAF 给的是 content:// URI），故直接走这条入口。
 */
export async function collectFromFileText(text: string, fileName: string): Promise<ParseResult> {
  const id = detectFormat(text, fileName)
  return await parseByFormat(text, id, fileName)
}

/** 从用户选择的目录导入（按子目录还原层级） */
export async function collectFromFolder(dir: string): Promise<ParseResult> {
  const files = await listImportFiles(dir, ['json', 'ini', 'conf', 'xsh', 'txt'])
  // 用户直接选了 FinalShell 的 conn 目录时，同样走跨文件解析
  if (looksLikeFinalShell(files)) return await parseFinalShellFiles(files)
  return await parseFiles(files, dir)
}

// ===== 落库 =====

export type ConflictStrategy = 'skip' | 'overwrite' | 'duplicate'

export interface ImportOptions {
  /** 目标命名空间 */
  namespaceId: string
  /** 已存在同名/同地址会话时的处理 */
  conflict: ConflictStrategy
  /** 是否尝试读取私钥文件内容托管（默认 true） */
  adoptKeys?: boolean
}

export interface ImportSummary {
  created: number
  updated: number
  skipped: number
  folders: number
  warnings: string[]
}

/** 把归一化会话写入 host store（批量，只落盘一次） */
export async function importSessions(
  sessions: NormSession[],
  opts: ImportOptions,
): Promise<ImportSummary> {
  const store = useHostStore()
  const summary: ImportSummary = { created: 0, updated: 0, skipped: 0, folders: 0, warnings: [] }
  const folderCache = new Map<string, string | null>()
  /** 待新建的文件夹（id 先生成，才能直接挂到会话的 parentId 上，最终一次性入列） */
  const pendingFolders: Array<Omit<HostItem, 'id' | 'sortOrder'> & { id: string }> = []

  /** 逐级确保文件夹存在，返回最内层文件夹 id（顶层返回 null） */
  function ensureFolder(path: string[]): string | null {
    let parentId: string | null = null
    const acc: string[] = []
    for (const seg of path) {
      acc.push(seg)
      const k = acc.join(' ')
      if (folderCache.has(k)) {
        parentId = folderCache.get(k) ?? null
        continue
      }
      const exist = store.hosts.find(
        (h) =>
          h.namespaceId === opts.namespaceId &&
          h.type === 'folder' &&
          h.parentId === parentId &&
          h.name === seg,
      )
      if (exist) {
        folderCache.set(k, exist.id)
        parentId = exist.id
        continue
      }
      const id = 'folder_' + Date.now() + '_' + Math.random().toString(36).slice(2, 6)
      pendingFolders.push({
        id,
        name: seg,
        addr: '',
        namespaceId: opts.namespaceId,
        parentId,
        type: 'folder',
      })
      folderCache.set(k, id)
      parentId = id
      summary.folders++
    }
    return parentId
  }

  const toCreate: Array<Omit<HostItem, 'id' | 'sortOrder'>> = []

  for (const s of sessions) {
    const folderId = ensureFolder(s.folderPath)
    const { host, port } = splitHostPort(s.host, s.port)
    const addr = joinAddr(host, port)

    // 私钥托管：能读到就存内容，原文件被删除也能连
    if (opts.adoptKeys !== false && s.keyPath && !s.keyContent) {
      try {
        const content = await readLocalTextFile(s.keyPath)
        if (content && /PRIVATE KEY|BEGIN OPENSSH/.test(content)) {
          s.keyContent = content
          s.keyName = s.keyName || baseName(s.keyPath)
        }
      } catch {
        // 私钥不可读（已移动/删除）：保留路径，用户后续可手动修正
      }
    }

    const config: HostItem['config'] = {
      username: s.username || 'root',
      port,
      authType: s.authType,
      ...(s.password ? { password: s.password } : {}),
      ...(s.keyPath ? { keyPath: s.keyPath } : {}),
      ...(s.keyContent ? { keyContent: s.keyContent } : {}),
      ...(s.keyName ? { keyName: s.keyName } : {}),
      ...(s.passphrase ? { passphrase: s.passphrase } : {}),
    }

    const existing = store.hosts.find(
      (h) =>
        h.namespaceId === opts.namespaceId &&
        h.type === 'host' &&
        h.parentId === folderId &&
        (h.addr === addr || h.name === s.name),
    )
    if (existing) {
      if (opts.conflict === 'skip') {
        summary.skipped++
        continue
      }
      if (opts.conflict === 'overwrite') {
        store.updateHost(existing.id, {
          name: s.name,
          addr,
          config: { ...(existing.config ?? {}), ...config },
        })
        summary.updated++
        continue
      }
      // duplicate：继续走新建
    }

    toCreate.push({
      name: s.name,
      addr,
      namespaceId: opts.namespaceId,
      parentId: folderId,
      type: 'host',
      config,
    })
  }

  // 文件夹 + 会话一次性入列，只触发一次落盘
  const created = store.addHostsBulk([...pendingFolders, ...toCreate])
  summary.created = created.filter((i) => i.type === 'host').length
  return summary
}

// ===== 导出 =====

/** 导出会话树到文件；返回是否真的写出（用户取消返回 false） */
export async function exportSessionsToFile(opts: {
  nodes: ExportSourceNode[]
  namespaceName: string
  includePasswords: boolean
  defaultName?: string
}): Promise<boolean> {
  const doc = await buildExportDoc(opts.nodes, opts.namespaceName, opts.includePasswords)
  const name = opts.defaultName || `jc-shell-sessions-${new Date().toISOString().slice(0, 10)}.json`
  const path = await pickSavePath('导出会话', name)
  if (!path) return false
  await saveTextFile(path, JSON.stringify(doc, null, 2))
  return true
}

export * from './types'
export { buildExportDoc, parseJcExport, JC_EXPORT_FORMAT } from './jc'
export type { ExportSourceNode, JcExportDoc } from './jc'
