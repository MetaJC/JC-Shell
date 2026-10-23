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
 * SSH 代理解析：把「通用-网络代理」设置翻译成 connect_ssh 需要的 proxy_addr / proxy_kind。
 *
 * 会话连接（TerminalWorkspace）与「测试连接」探测（HostListPanel）共用同一份实现 ——
 * 否则测试结果可能与真实连接不一致（例如测试忽略了代理，测出来能连、实际连不上）。
 */
import { invoke } from '@/api'
import type { AppSettings } from '@/stores/settings'

export interface SshProxy {
  /** 代理地址 host:port；直连时 null */
  addr: string | null
  /** 代理类型：socks5 / http；直连时 null */
  kind: string | null
}

export async function resolveSshProxy(s: AppSettings): Promise<SshProxy> {
  if (s.proxyMode === 'direct') return { addr: null, kind: null }
  if (s.proxyMode === 'system') {
    try {
      const info = await invoke<{ addr: string; kind: string } | null>('get_system_proxy')
      return info ? { addr: info.addr, kind: info.kind } : { addr: null, kind: null }
    } catch {
      return { addr: null, kind: null }
    }
  }
  const h = (s.manualProxyHost || '').trim()
  if (!h || !s.manualProxyPort) return { addr: null, kind: null }
  // 手动配置默认走 HTTP CONNECT（兼容 Clash 等混合端口代理）
  return { addr: `${h}:${s.manualProxyPort}`, kind: 'http' }
}
