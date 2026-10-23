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
 * 跨软件会话导入/导出：归一化中间结构
 * 作者：JC
 *
 * 各客户端格式差异大，统一先解析成 NormSession，再由 importSessions 落到 host store。
 * 这样新增一个客户端只需写一个 parser，不必改动落库与 UI。
 */

export type ImportClientId =
  | 'xshell'
  | 'mobaxterm'
  | 'electerm'
  | 'termius'
  | 'finalshell'
  | 'jc'
  | 'unknown'

/** 归一化会话（解析器产出物） */
export interface NormSession {
  name: string
  host: string
  port: number
  username: string
  authType: 'password' | 'key'
  /** 仅 jc-shell 自有格式会带密码；外部客户端一律不迁移（各家都是密文/钥匙串） */
  password?: string
  /** 私钥原始路径（导入时若可读则读入 keyContent 托管） */
  keyPath?: string
  /** 私钥内容：托管后即使原文件被删除也能连 */
  keyContent?: string
  /** 私钥原始文件名，仅用于展示 */
  keyName?: string
  passphrase?: string
  /** 所属文件夹层级，自上而下 */
  folderPath: string[]
  source: ImportClientId
  /** 提示信息，如「密码未迁移」 */
  note?: string
}

export interface ParseResult {
  sessions: NormSession[]
  warnings: string[]
}

/** Rust 侧探测到的已安装客户端 */
export interface DetectedClient {
  id: string
  name: string
  kind: string
  path: string
  note: string
}

/** Rust 侧列出的目录文件 */
export interface ImportFileEntry {
  name: string
  path: string
}

export const DEFAULT_SSH_PORT = 22

/** 取第一个非空字段（各客户端字段命名不统一，按候选名依次尝试） */
export function pickField(obj: Record<string, unknown>, keys: string[]): string {
  for (const k of keys) {
    const v = obj[k]
    if (typeof v === 'string' && v.trim()) return v.trim()
    if (typeof v === 'number') return String(v)
  }
  return ''
}

/** 名字清洗：去掉路径分隔符，避免把文件夹名塞进层级造成错层 */
export function cleanName(s: string, fallback = '未命名会话'): string {
  const t = (s || '').replace(/[\\/]+/g, '-').trim()
  return t || fallback
}

/** 解析端口：非法/缺失用默认 22 */
export function parsePort(v: unknown): number {
  const n = typeof v === 'number' ? v : parseInt(String(v ?? ''), 10)
  return Number.isFinite(n) && n > 0 && n < 65536 ? n : DEFAULT_SSH_PORT
}

/** 主机地址是否可用（非空且不含空白） */
export function validHost(h: string): boolean {
  const s = (h || '').trim()
  return s.length > 0 && !/\s/.test(s)
}

/**
 * 拆分 "host:port"。
 * 部分客户端把端口拼在主机里（如 1.2.3.4:2222 / [::1]:22），这里统一拆开。
 */
export function splitHostPort(input: string, fallbackPort: number): { host: string; port: number } {
  const s = (input || '').trim()
  const m = s.match(/^(.*):(\d+)$/)
  if (m) {
    const h = m[1].replace(/^\[|\]$/g, '')
    const p = parseInt(m[2], 10)
    if (h && Number.isFinite(p) && p > 0 && p < 65536) return { host: h, port: p }
  }
  return { host: s, port: fallbackPort }
}

/** 拼回 addr（host:port） */
export function joinAddr(host: string, port: number): string {
  // IPv6 需要方括号，否则与端口冒号冲突
  return host.includes(':') ? `[${host}]:${port}` : `${host}:${port}`
}
