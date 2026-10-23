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

﻿/**
 * 文件拖拽传输控制器
 * 作者：JC
 * - 应用内远程文件拖拽：HTML5 DnD（application/x-jc-file），支持分屏窗格间互拖（local/ssh 任意组合）
 * - 本地 OS 文件拖入：tauri://drag-drop 事件（可拿到真实路径），按放置区坐标命中目标窗格/文件夹
 * 放置动作：同会话 mv/本地移动；跨会话按两端类型选择上传/下载/中转/本地复制；全程 transfer-progress 进度。
 */
import { invoke } from '@/api'
import { useTransferStore } from '@/stores/transfer'
import { friendlyError } from '@/utils/errorHandler'

/** 应用内文件拖拽的自定义 MIME */
export const FILE_DRAG_MIME = 'application/x-jc-file'

/** 应用内拖拽的远程文件载荷 */
export interface RemoteFilePayload {
  sessionId: string
  path: string
  type: 'file' | 'dir'
  name: string
  /** 源会话类型（跨会话传输时决定通道与队列方向） */
  kind?: 'local' | 'ssh'
}

/** OS 文件放置区（终端窗格 / fs 面板注册，drag-drop 事件按坐标命中） */
export interface OsDropZone {
  id: string
  el: HTMLElement
  sessionId: string
  /** 本地终端（拖入走本地复制而非上传） */
  local: boolean
  /** 可选：按放置坐标解析目标目录（fs 面板：命中悬停文件夹 → 该文件夹，否则当前目录） */
  resolveDir?: (x: number, y: number) => string | null
}

const osZones = new Map<string, OsDropZone>()

export function registerOsDropZone(zone: OsDropZone): void {
  osZones.set(zone.id, zone)
}

export function unregisterOsDropZone(id: string): void {
  osZones.delete(id)
}

/** 按窗口内坐标查找命中的放置区（嵌套时取面积最小的最内层） */
export function findOsDropZoneAt(x: number, y: number): OsDropZone | null {
  let best: OsDropZone | null = null
  let bestArea = Infinity
  for (const z of osZones.values()) {
    const r = z.el.getBoundingClientRect()
    if (x >= r.left && x <= r.right && y >= r.top && y <= r.bottom) {
      const area = r.width * r.height
      if (area < bestArea) { bestArea = area; best = z }
    }
  }
  return best
}

function genTransferId(): string {
  return 'tf-' + Date.now() + '-' + Math.random().toString(36).slice(2, 6)
}

/** 路径拼接：分隔符跟随目标目录风格（Windows \ / POSIX /） */
function joinPathSmart(dir: string, name: string): string {
  const sep = dir.includes('\\') ? '\\' : '/'
  const d = dir.replace(/[\\/]+$/, '')
  if (!d) return name
  return d === '/' ? '/' + name : d + sep + name
}

// ===== HTML5 DnD 载荷读写 =====
// 同页面内拖拽时，Chromium 的 dataTransfer.types 跨元素判定并不可靠，
// 易导致目标窗格 dragover 不 preventDefault 而显示"禁止符号"。
// 因此在 dragstart 同步写入模块级标记，dragover/drop 阶段优先据此判断；
// dataTransfer.setData 仍保留，作为兜底与潜在的外部可读。
let draggingPayload: RemoteFilePayload | null = null

export function setRemoteFileDrag(e: DragEvent, payload: RemoteFilePayload): void {
  console.log('[fs-drag] dragstart 源', payload.sessionId, payload.path, payload.type)
  if (!e.dataTransfer) return
  e.dataTransfer.setData(FILE_DRAG_MIME, JSON.stringify(payload))
  e.dataTransfer.effectAllowed = 'copyMove'
  draggingPayload = payload
}

/** 拖拽结束（dragend，源行触发）：清空模块标记，避免残留影响后续判断 */
export function resetRemoteFileDrag(): void {
  draggingPayload = null
}

/** dragover 阶段仅能读 types（判断是否为应用内文件拖拽）。优先走模块标记，更稳 */
export function isRemoteFileDrag(e: DragEvent): boolean {
  if (draggingPayload) return true
  return !!e.dataTransfer && Array.from(e.dataTransfer.types).includes(FILE_DRAG_MIME)
}

export function getRemoteFilePayload(e: DragEvent): RemoteFilePayload | null {
  if (draggingPayload) return draggingPayload
  const raw = e.dataTransfer?.getData(FILE_DRAG_MIME)
  if (!raw) return null
  try {
    return JSON.parse(raw) as RemoteFilePayload
  } catch {
    return null
  }
}

// ===== 传输动作 =====

/** 获取终端会话当前目录（本地/SSH 均走 shell cwd 跟踪，失败回退到家目录或 /） */
export async function getShellCwd(sessionId: string, local: boolean): Promise<string> {
  try {
    const cwd = await invoke<string>('get_shell_cwd', { sessionId })
    const trimmed = cwd.trim().replace(/[\\/]+$/, '')
    if (trimmed) return trimmed
  } catch { /* 读取失败走回退 */ }
  if (local) {
    try {
      return await invoke<string>('get_local_home_dir')
    } catch { /* ignore */ }
  }
  return '/'
}

/**
 * 应用内远程文件放入目标会话目录：
 * - 同会话 → mv（SSH）/ move_local_file（本地）
 * - 跨会话 → 后端 transfer_remote_file（按两端类型自动选择通道，带进度）
 */
export async function transferRemoteFile(
  src: RemoteFilePayload,
  dstSessionId: string,
  dstDir: string,
  dstKind: 'local' | 'ssh' = 'ssh'
): Promise<string> {
  const dstPath = joinPathSmart(dstDir, src.name)
  if (src.sessionId === dstSessionId) {
    if (dstKind === 'local') {
      await invoke<string>('move_local_file', { src: src.path, dst: dstPath })
    } else {
      await invoke<string>('exec_remote_command', {
        dto: { session_id: dstSessionId, command: `mv -f '${src.path.replace(/'/g, "'\\''")}' '${dstPath.replace(/'/g, "'\\''")}'` }
      })
    }
    return dstPath
  }
  const transferId = genTransferId()
  const direction = src.kind === 'local' ? 'upload' : (dstKind === 'local' ? 'download' : 'upload')
  const store = useTransferStore()
  store.begin({ id: transferId, name: src.name, direction, ownerSessionId: dstSessionId, dstPath })
  try {
    await invoke('transfer_remote_file', {
      dto: {
        srcSessionId: src.sessionId,
        dstSessionId: dstSessionId,
        srcPath: src.path,
        dstPath: dstPath,
        transferId: transferId,
        srcType: src.type ?? 'file',
        srcKind: src.kind ?? 'ssh',
        dstKind: dstKind
      }
    })
  } catch (e: any) {
    store.markError(transferId, e?.message || String(e))
    throw e
  }
  return dstPath
}

/** 上传本地 OS 文件到远程会话目录（带进度） */
export async function uploadLocalFile(localPath: string, sessionId: string, dstDir: string): Promise<string> {
  const name = localPath.replace(/\\/g, '/').split('/').pop() || 'file'
  const remotePath = joinPathSmart(dstDir, name)
  const transferId = genTransferId()
  const store = useTransferStore()
  store.begin({ id: transferId, name, direction: 'upload', ownerSessionId: sessionId, dstPath: remotePath })
  try {
    await invoke('upload_file', {
      dto: {
        sessionId: sessionId,
        transferId: transferId,
        localPath: localPath,
        remotePath: remotePath
      }
    })
  } catch (e: any) {
    store.markError(transferId, e?.message || String(e))
    throw e
  }
  return remotePath
}

/**
 * 手机端上传：把 WebView 文件选择器选中的 File 上传到远程会话目录（带进度）。
 * 字节经 base64 走 IPC，Rust 端落临时文件后复用 upload_file 全链路
 * （SFTP 流式 / transfer-progress 进度 / 可取消 / exec 兜底）。
 */
export async function uploadFileBytes(file: File, sessionId: string, dstDir: string): Promise<string> {
  const remotePath = joinPathSmart(dstDir, file.name)
  const transferId = genTransferId()
  const store = useTransferStore()
  store.begin({ id: transferId, name: file.name, direction: 'upload', ownerSessionId: sessionId, dstPath: remotePath })
  try {
    const buf = await file.arrayBuffer()
    const dataBase64 = arrayBufferToBase64(buf)
    await invoke('upload_file_bytes', {
      dto: {
        sessionId: sessionId,
        transferId: transferId,
        remotePath: remotePath,
        fileName: file.name,
        dataBase64: dataBase64
      }
    })
  } catch (e: any) {
    store.markError(transferId, e?.message || String(e))
    throw e
  }
  return remotePath
}

/** 分块 btoa：一次性 String.fromCharCode 大数组会爆调用栈，按 32KB 分块 */
function arrayBufferToBase64(buf: ArrayBuffer): string {
  const bytes = new Uint8Array(buf)
  const chunk = 0x8000
  let out = ''
  for (let i = 0; i < bytes.length; i += chunk) {
    out += String.fromCharCode.apply(null, Array.from(bytes.subarray(i, i + chunk)))
  }
  return btoa(out)
}

/** 复制本地 OS 文件到本地目录（本地终端场景，带进度） */
export async function copyLocalFile(localPath: string, dstDir: string, ownerSessionId: string): Promise<string> {
  const name = localPath.replace(/\\/g, '/').split('/').pop() || 'file'
  const dstPath = joinPathSmart(dstDir, name)
  const transferId = genTransferId()
  const store = useTransferStore()
  store.begin({ id: transferId, name, direction: 'upload', ownerSessionId, dstPath })
  try {
    return await invoke<string>('copy_local_file', { src: localPath, dstDir, transferId })
  } catch (e: any) {
    store.markError(transferId, e?.message || String(e))
    throw e
  }
}

export interface OsDropResult {
  ok: string[]
  failed: { path: string; error: string }[]
}

/**
 * 处理本地 OS 文件拖放到某放置区（支持多文件，逐个传输）：
 * - 目标目录：zone.resolveDir 命中（fs 面板悬停文件夹）优先，否则会话当前目录
 * - SSH 会话 → 上传；本地终端 → 本地复制
 */
export async function handleOsFilesToZone(paths: string[], zone: OsDropZone, x = 0, y = 0): Promise<OsDropResult> {
  const result: OsDropResult = { ok: [], failed: [] }
  if (paths.length === 0) throw new Error('未接收到文件')
  const resolved = zone.resolveDir?.(x, y) ?? null
  const dstDir = resolved ?? await getShellCwd(zone.sessionId, zone.local)
  for (const p of paths) {
    try {
      const dst = zone.local
        ? await copyLocalFile(p, dstDir, zone.sessionId)
        : await uploadLocalFile(p, zone.sessionId, dstDir)
      result.ok.push(dst)
    } catch (e: any) {
      // 错误原文保留给上层友好化展示（friendlyText）；此处不直接展示技术细节
      result.failed.push({ path: p, error: friendlyError(e) })
    }
  }
  return result
}
