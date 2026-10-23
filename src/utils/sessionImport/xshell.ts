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
 * Xshell 会话文件（.xsh）解析
 * 作者：JC
 *
 * .xsh 是 INI 文本（常见 UTF-16LE 编码，已由 Rust 侧 readLocalTextFile 解码成 UTF-8）。
 * 关键字段（不同版本命名略有差异，故按 key 大小写不敏感收集后按候选名取值）：
 *   [CONNECTION]      Host= / Port= / Protocol=
 *   [CONNECTION:SSH]  UserName= / Method=Password|PublicKey / UserKey= / Passphrase=
 * 密码不迁移：Xshell 存的是密文（绑定本机/主密码），取出来也无法使用。
 */
import type { NormSession, ParseResult } from './types'
import { DEFAULT_SSH_PORT, parsePort, pickField, validHost } from './types'

/** 非 SSH 协议（Xshell 也支持 telnet/serial/rlogin 等），导入时跳过 */
const NON_SSH_PROTOCOLS = ['telnet', 'serial', 'rlogin', 'local']

export function parseXshell(text: string, displayName: string): ParseResult {
  const warnings: string[] = []
  const kv: Record<string, string> = {}

  for (const rawLine of text.split(/\r?\n/)) {
    const line = rawLine.trim()
    if (!line || line.startsWith(';') || line.startsWith('#')) continue
    // 段名只用于跳过，字段取值不依赖它（不同版本段名不一致）
    if (line.startsWith('[')) continue
    const eq = line.indexOf('=')
    if (eq <= 0) continue
    const k = line.slice(0, eq).trim().toLowerCase()
    const v = line.slice(eq + 1).trim()
    if (!kv[k]) kv[k] = v // 首次非空优先，避免空值覆盖
  }

  const protocol = (pickField(kv, ['protocol']) || '').toLowerCase()
  if (protocol && NON_SSH_PROTOCOLS.some((p) => protocol.includes(p))) {
    return { sessions: [], warnings: [`${displayName}：非 SSH 会话（${protocol}），已跳过`] }
  }

  const host = pickField(kv, ['host', 'hostname', 'hostaddr', 'ip', 'address', 'server'])
  if (!validHost(host)) {
    return { sessions: [], warnings: [`${displayName}：未找到主机地址，已跳过`] }
  }

  const method = pickField(kv, ['method', 'authmethod', 'authtype']).toLowerCase()
  const userKey = pickField(kv, ['userkey', 'userkeyname', 'privatekey', 'identityfile', 'keyfile'])
  const authType: NormSession['authType'] =
    method.includes('publickey') || method.includes('key') || !!userKey ? 'key' : 'password'

  const portRaw = pickField(kv, ['port'])
  const port = portRaw ? parsePort(portRaw) : DEFAULT_SSH_PORT
  const username = pickField(kv, ['username', 'user', 'login', 'loginname', 'account']) || 'root'

  const session: NormSession = {
    name: displayName || host,
    host,
    port,
    username,
    authType,
    keyPath: userKey || undefined,
    folderPath: [],
    source: 'xshell',
    note: '密码为 Xshell 密文，未迁移，请手动补全',
  }
  return { sessions: [session], warnings }
}
