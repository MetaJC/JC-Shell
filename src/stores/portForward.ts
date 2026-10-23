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
 * 端口转发（SSH 隧道）规则 Store
 * 作者：JC
 *
 * 三种模式（与架构图一致，数据流向统一为「从左到右」）：
 * - local   本地模式：本机监听端口 → 经 SSH 服务器 → 访问服务器侧的目标主机:端口
 * - remote  远程模式：SSH 服务器监听端口 → 经隧道 → 访问本机侧的目标主机:端口
 * - dynamic 动态模式：本机起 SOCKS5 代理 → 经 SSH 服务器 → 访问任意目标网络
 *
 * 规则持久化到本地设置库（app_setting，键 ui_port_forward），
 * 不落云同步（含主机地址等本机信息）。
 */
import { defineStore } from 'pinia'
import { ref } from 'vue'
import { ElMessage } from 'element-plus'
import { invoke, isTauriEnv } from '@/api'
import { useI18nStore } from '@/stores/i18n'
import { useSettingsStore } from '@/stores/settings'
import { resolveSshProxy } from '@/utils/sshProxy'

export type ForwardMode = 'local' | 'remote' | 'dynamic'

/**
 * 远程模式「测试连通性」时，用户为两台远程客户端各自配置的 SSH 连接信息。
 * 客户端机器本身是一台可被 SSH 连上的主机；配置它与「SSH 服务器」表单字段一一对应。
 */
export interface RemoteClientSsh {
  sshHost: string
  sshPort: number
  sshUser: string
  sshAuthType: 'password' | 'key'
  sshPassword: string
  sshKeyPath: string
  sshKeyContent: string
  sshKeyName: string
  sshPassphrase: string
}

/** 右侧那台远程客户端（连接「用户配置的监听地址」）的配置，连同当时绑定的监听地址：
 * 监听地址一旦改变，这份配置即作废、需重新配置。 */
export interface RemoteClientRight {
  addr: string
  cfg: RemoteClientSsh
}

export interface ForwardRule {
  id: string
  /** 规则名称（列表展示） */
  name: string
  mode: ForwardMode
  /** 是否已启动隧道 */
  enabled: boolean
  /** SSH 服务器（本机发起 SSH 连接的目标，即「中间服务器 / 跳板机」） */
  sshHost: string
  sshPort: number
  sshUser: string
  /** SSH 服务器认证方式：密码 / 私钥 */
  sshAuthType: 'password' | 'key'
  /** 密码认证 */
  sshPassword: string
  /** 私钥认证：path 为文件路径（兼容旧数据），content 为托管的私钥原文，name 为原始文件名，passphrase 为私钥口令 */
  sshKeyPath: string
  sshKeyContent: string
  sshKeyName: string
  sshPassphrase: string
  /** 本地监听绑定的地址（本地/动态模式）：默认 127.0.0.1，填 0.0.0.0 可对外暴露 */
  bindAddr: string
  /** 本地模式：本机监听端口；动态模式：SOCKS 监听端口 */
  localPort: number
  /** 远程模式：在 SSH 服务器上开放的端口 */
  remotePort: number
  /** 本地模式：服务器侧的目标主机 / 端口 */
  remoteTargetHost: string
  remoteTargetPort: number
  /** 远程模式：本机侧的目标主机 / 端口 */
  localTargetHost: string
  localTargetPort: number
  /**
   * 远程模式「测试连通性」用：左侧那台远程客户端（连接非监听地址）的 SSH 配置。
   * 与监听地址无关，配置一次即持久化。
   */
  remoteClientLeft: RemoteClientSsh | null
  /**
   * 远程模式「测试连通性」用：右侧那台远程客户端（连接用户配置的监听地址）的 SSH 配置，
   * 连同配置时绑定的监听地址。监听地址改变即须重新配置。
   */
  remoteClientRight: RemoteClientRight | null
  createdAt: number
  updatedAt: number
}

/** 图示中的节点：客户端 → Internet/隧道 → SSH 服务器 → 目标（防火墙只是「被挡住」的装饰元素） */
export type ForwardNodeId = 'client' | 'internet' | 'ssh' | 'target'

const SETTING_KEY = 'ui_port_forward'

/** 各模式的文案键（i18n 前缀 portForward.mode.） */
export const FORWARD_MODES: ForwardMode[] = ['local', 'remote', 'dynamic']

export function defaultRule(mode: ForwardMode): ForwardRule {
  const now = Date.now()
  return {
    id: `fwd_${now}_${Math.random().toString(36).slice(2, 7)}`,
    name: '',
    mode,
    enabled: false,
    sshHost: '',
    sshPort: 22,
    sshUser: 'root',
    sshAuthType: 'password',
    sshPassword: '',
    sshKeyPath: '',
    sshKeyContent: '',
    sshKeyName: '',
    sshPassphrase: '',
    bindAddr: '127.0.0.1',
    // 端口与目标主机一律留空，由用户填写（只默认绑定地址）
    localPort: 0,
    remotePort: 0,
    remoteTargetHost: '',
    remoteTargetPort: 0,
    localTargetHost: '',
    localTargetPort: 0,
    remoteClientLeft: null,
    remoteClientRight: null,
    createdAt: now,
    updatedAt: now,
  }
}

/** 规则摘要（列表第二行展示，随模式不同） */
export function ruleSummary(r: ForwardRule): string {
  if (r.mode === 'local') return `${r.localPort} → ${r.remoteTargetHost}:${r.remoteTargetPort}`
  if (r.mode === 'remote') return `${r.remotePort} → ${r.localTargetHost}:${r.localTargetPort}`
  return `SOCKS5 127.0.0.1:${r.localPort}`
}

export interface ForwardRuleDraft extends ForwardRule {
  /** 生成隧道名称用（SSH 目标） */
}

/** 一个「消息段」的测试结果：段序号 + 是否通过 + 失败原因 */
export interface ForwardTestStep {
  leg: number
  ok: boolean
  reason: string
}

/**
 * 连通性自检：消息沿隧道逐段前进，到每个节点时校验该节点的配置。
 * 段 0 → 到达 Internet/隧道：校验起始端（本机监听 / SOCKS）
 * 段 1 → 到达 SSH 服务器：校验 SSH 地址与登录名
 * 段 2 → 到达目标主机：校验目标主机 / 端口
 * 段 3 → 到达目标服务（仅本地模式）：最终服务能否访问
 *
 * 说明：目前是「配置校验 + 可达性推导」，真实探测（TCP connect / SSH 认证）
 * 只需替换本函数（或改调后端探针命令），动画与结果结构无需改动。
 */
export function buildTestSteps(r: ForwardRule): ForwardTestStep[] {
  const portOk = (p: number) => Number.isInteger(p) && p >= 0 && p <= 65535
  const hostOk = (h: string) => !!h && h.trim().length > 0

  // 段 0：起始端配置（本地监听端口 / SOCKS 端口 / 服务器监听端口）
  const startOk = r.mode === 'remote'
    ? portOk(r.remotePort)
    : portOk(r.localPort)
  const startErr = r.mode === 'remote' ? 'portForward.err.remotePort' : 'portForward.err.localPort'

  // 段 1：SSH 服务器（含认证方式：密码须填密码；密钥须有私钥内容或路径）
  const sshAuthOk = r.sshAuthType === 'key'
    ? !!(r.sshKeyContent || r.sshKeyPath)
    : !!r.sshPassword
  const sshAuthErr = r.sshAuthType === 'key'
    ? 'portForward.err.sshKey'
    : 'portForward.err.sshPassword'
  const sshOk = hostOk(r.sshHost) && portOk(r.sshPort) && hostOk(r.sshUser) && sshAuthOk
  const sshErr = !hostOk(r.sshHost)
    ? 'portForward.err.sshHost'
    : (!portOk(r.sshPort)
      ? 'portForward.err.sshPort'
      : (!hostOk(r.sshUser)
        ? 'portForward.err.sshUser'
        : sshAuthErr))

  // 段 2：目标
  let targetOk = true
  let targetErr = ''
  if (r.mode === 'local') {
    targetOk = hostOk(r.remoteTargetHost) && portOk(r.remoteTargetPort)
    targetErr = hostOk(r.remoteTargetHost) ? 'portForward.err.remoteTargetPort' : 'portForward.err.remoteTargetHost'
  } else if (r.mode === 'remote') {
    targetOk = hostOk(r.localTargetHost) && portOk(r.localTargetPort)
    targetErr = 'portForward.err.localTarget'
  }

  const steps: ForwardTestStep[] = [
    { leg: 0, ok: startOk, reason: startOk ? '' : startErr },
    { leg: 1, ok: sshOk, reason: sshOk ? '' : sshErr },
    { leg: 2, ok: targetOk, reason: targetOk ? '' : targetErr },
  ]
  // 本地模式还有最后一段「目标主机 → 目标服务」：消息要一直走到最终服务，
  // 由该服务是否可达（目标主机 + 端口是否填全）决定成败。
  if (r.mode === 'local') {
    steps.push({ leg: 3, ok: targetOk, reason: targetOk ? '' : targetErr })
  }
  return steps
}

/** 规则是否可保存（只做必填校验，连通性由测试按钮负责） */
export function validateRule(r: ForwardRule): string {
  const steps = buildTestSteps(r)
  const bad = steps.find(s => !s.ok)
  return bad ? bad.reason : ''
}

/**
 * 真实探测：TCP 连接 SSH 服务器。
 * 端口转发里本机能直连的只有 SSH 服务器，因此用它验证所填地址是否真实可达
 * （不是「非空」这种假校验）。返回 null 表示可达；否则返回失败原因的 i18n key。
 * 纯浏览器（非 Tauri）环境无法探测，返回 null 以免误判。
 */
export async function probeSshReachable(r: ForwardRule, timeoutMs = 5000): Promise<string | null> {
  if (!isTauriEnv()) return null
  const host = (r.sshHost || '').trim()
  if (!host) return 'portForward.err.sshHost'
  if (!Number.isInteger(r.sshPort) || r.sshPort < 1 || r.sshPort > 65535) return 'portForward.err.sshPort'
  try {
    await invoke('tcp_probe', { host, port: r.sshPort, timeoutMs })
    return null
  } catch (e) {
    console.warn('[portForward] SSH 服务器连通性探测失败:', e)
    return 'portForward.err.sshUnreachable'
  }
}

/**
 * 规则 → 后端 DTO（字段名与 Rust 的 PortForwardRuleDto 对齐）。
 * 私钥优先把「托管内容」落盘成绝对路径再交给后端，用户删掉原文件也能连。
 */
export async function toBackendRule(r: ForwardRule): Promise<Record<string, unknown>> {
  let keyPath = r.sshKeyPath
  if (r.sshAuthType === 'key' && r.sshKeyContent) {
    keyPath = await invoke<string>('materialize_key', { content: r.sshKeyContent })
  }
  return {
    id: r.id,
    mode: r.mode,
    bind_addr: r.bindAddr,
    local_port: r.localPort,
    ssh_host: r.sshHost,
    ssh_port: r.sshPort,
    ssh_user: r.sshUser,
    ssh_auth_type: r.sshAuthType === 'key' ? 'key' : 'password',
    ssh_password: r.sshPassword,
    ssh_key_path: keyPath,
    ssh_passphrase: r.sshPassphrase,
    remote_target_host: r.remoteTargetHost,
    remote_target_port: r.remoteTargetPort,
    remote_port: r.remotePort,
    local_target_host: r.localTargetHost,
    local_target_port: r.localTargetPort,
  }
}

/** 右侧远程客户端配置是否仍有效：已配置且其绑定的监听地址与当前一致 */
export function remoteClientRightValid(r: ForwardRule): boolean {
  return !!r.remoteClientRight
    && r.remoteClientRight.addr.trim().toLowerCase() === (r.bindAddr || '').trim().toLowerCase()
}

/** 远程模式是否已完成两台远程客户端配置（可走真实测试而非模拟） */
export function remoteClientConfigured(r: ForwardRule): boolean {
  return !!r.remoteClientLeft && remoteClientRightValid(r)
}

/** 单台远程客户端真实探测的结果（与后端 RemoteClientProbeResult 对齐） */
export interface RemoteClientProbeResult {
  sshOk: boolean
  targetOk: boolean
  sshErr: string | null
  targetErr: string | null
}

/** 后端真实返回字段名（snake_case）。Tauri 只转换入参、不转换出参，前端据此接收后转 camelCase */
interface RemoteClientProbeResponse {
  ssh_ok: boolean
  target_ok: boolean
  ssh_err: string | null
  target_err: string | null
}

/**
 * 把后端 SSH 报错归类为友好提示的 i18n key。
 * 原始报错（堆栈、英文错误串、内部字段名）绝不直接抛给用户，只给分类后的友好文案。
 */
export function sshErrorKey(raw: string | null | undefined): string {
  const s = (raw || '').toLowerCase()
  // Windows 的系统错误文案是本地化中文（如「目标计算机积极拒绝」），匹配不到英文
  // 关键词，因此优先按「os error N」代码归类：
  // 10060 连接超时 / 10061 10053 10054 拒绝·重置 / 10065 10051 10064 10050 不可达 / 11001-11004 DNS
  if (/os error (10060|10038)/.test(s)) return 'portForward.ssh.reason.timeout'
  if (/os error (10061|10053|10054)/.test(s)) return 'portForward.ssh.reason.refused'
  if (/os error (10050|10051|10064|10065)/.test(s)) return 'portForward.ssh.reason.unreachable'
  if (/os error (11001|11002|11003|11004)/.test(s)) return 'portForward.ssh.reason.dns'
  if (/auth|password|passwd|permission denied|publickey|keyboard|密钥|密码|认证/.test(s)) {
    return 'portForward.ssh.reason.auth'
  }
  if (/timed?\s*-?\s*out|timeout|超时|没有正确答复/.test(s)) return 'portForward.ssh.reason.timeout'
  if (/refused|reset by peer|积极拒绝/.test(s)) return 'portForward.ssh.reason.refused'
  if (/unreachable|no route|network|网络不可达|找不到网络路径|网络不可用/.test(s)) {
    return 'portForward.ssh.reason.unreachable'
  }
  if (/resolve|name or service not known|getaddrinfo|nodename|dns 解析失败|解析失败/.test(s)) {
    return 'portForward.ssh.reason.dns'
  }
  if (/handshake|kex|protocol|not.*ssh|invalid.*format/.test(s)) {
    return 'portForward.ssh.reason.handshake'
  }
  return 'portForward.ssh.reason.other'
}

/**
 * 「通用-网络代理」设置 → 后端 SSH 建连参数（proxyAddr/proxyKind）。
 * 与终端会话连接共用同一份代理解析：否则配置了代理的环境下，
 * 会出现「终端（走代理）能连、端口转发（直连）连不上」的不一致。
 */
export async function proxyInvokeArgs(): Promise<{ proxyAddr: string | null; proxyKind: string | null }> {
  const proxy = await resolveSshProxy(useSettingsStore().settings)
  return { proxyAddr: proxy.addr, proxyKind: proxy.kind }
}

/**
 * 真实探测一台远程客户端：SSH 建连 + 认证，再从它向 bindAddr:remotePort 开通道。
 * 纯浏览器环境无法探测，直接返回成功以免误判。
 */
export async function probeRemoteClient(
  cfg: RemoteClientSsh,
  bindAddr: string,
  remotePort: number,
): Promise<RemoteClientProbeResult> {
  if (!isTauriEnv()) return { sshOk: true, targetOk: true, sshErr: null, targetErr: null }
  let keyPath = cfg.sshKeyPath
  if (cfg.sshAuthType === 'key' && cfg.sshKeyContent) {
    keyPath = await invoke<string>('materialize_key', { content: cfg.sshKeyContent })
  }
  const raw = await invoke<RemoteClientProbeResponse>('remote_client_probe', {
    client: {
      // 注意：Tauri 只对「顶层参数名」做 camelCase → snake_case 转换，
      // 嵌套对象内部字段必须与 Rust 结构体（RemoteClientDto）的 snake_case 字段一一对应
      ssh_host: cfg.sshHost,
      ssh_port: cfg.sshPort,
      ssh_user: cfg.sshUser,
      ssh_auth_type: cfg.sshAuthType,
      ssh_password: cfg.sshPassword,
      ssh_key_path: keyPath,
      ssh_passphrase: cfg.sshPassphrase,
    },
    bindAddr,
    remotePort,
    ...(await proxyInvokeArgs()),
  })
  // 后端出参按 Rust 字段名（snake_case）返回，Tauri 不转换；先收原始字段再映射成前端使用的 camelCase
  return {
    sshOk: raw.ssh_ok,
    targetOk: raw.target_ok,
    sshErr: raw.ssh_err,
    targetErr: raw.target_err,
  }
}

export const usePortForwardStore = defineStore('portForward', () => {
  const rules = ref<ForwardRule[]>([])
  const loaded = ref(false)

  function snapshot(): ForwardRule[] {
    return rules.value.map(r => ({ ...r }))
  }

  async function load(): Promise<void> {
    if (!isTauriEnv()) { loaded.value = true; return }
    try {
      const json = await invoke<string | null>('get_setting', { key: SETTING_KEY })
      if (json) {
        const arr = JSON.parse(json) as ForwardRule[]
        // 与默认值合并，补齐旧数据缺失的新字段（如 bindAddr）；
        // 隧道不跨进程存活，启动时一律回到「未开启」。
        if (Array.isArray(arr)) {
          rules.value = arr.map(r => ({ ...defaultRule(r.mode), ...r, enabled: false }))
        }
      }
    } catch (e) {
      console.warn('[portForward] load failed:', e)
    } finally {
      loaded.value = true
    }
  }

  async function persist(): Promise<void> {
    if (!isTauriEnv()) return
    try {
      await invoke('set_setting', { key: SETTING_KEY, value: JSON.stringify(snapshot()) })
    } catch (e) {
      console.warn('[portForward] persist failed:', e)
    }
  }

  function add(rule: ForwardRule): void {
    rules.value.unshift({ ...rule, createdAt: Date.now(), updatedAt: Date.now() })
    void persist()
  }

  /** 会影响隧道本身的字段：只改名字 / 时间戳不需要重建隧道 */
  const TUNNEL_KEYS: (keyof ForwardRule)[] = [
    'mode', 'sshHost', 'sshPort', 'sshUser', 'sshAuthType', 'sshPassword',
    'sshKeyPath', 'sshKeyContent', 'sshPassphrase',
    'bindAddr', 'localPort', 'remotePort',
    'remoteTargetHost', 'remoteTargetPort', 'localTargetHost', 'localTargetPort',
  ]
  function tunnelConfigChanged(a: ForwardRule, b: ForwardRule): boolean {
    return TUNNEL_KEYS.some(k => a[k] !== b[k])
  }

  /**
   * 按当前规则重建隧道（先拆后建）。
   * 编辑一条「已开启」的规则时必须在保存时重建，否则新配置要等用户手动关掉再打开才生效。
   */
  async function restartTunnel(id: string): Promise<void> {
    const r = rules.value.find(x => x.id === id)
    if (!r) return
    try {
      await invoke('port_forward_stop', { id })
    } catch (e) {
      console.warn('[portForward] 重建隧道前停止失败:', e)
    }
    try {
      await invoke<number>('port_forward_start', { rule: await toBackendRule(r), ...(await proxyInvokeArgs()) })
      ElMessage.success(useI18nStore().t('portForward.tunnelRestarted'))
    } catch (e) {
      // 新配置起不来：如实把开关落回「未开启」，避免界面显示已开启、实际没有隧道
      r.enabled = false
      ElMessage.error(`${useI18nStore().t('portForward.tunnelRestartFail')}：${(e as Error)?.message ?? e}`)
    }
    r.updatedAt = Date.now()
    void persist()
  }

  /**
   * 编辑保存：写盘；若该规则已开启且隧道配置发生变化，立即按新配置重建 —— 保存即生效，
   * 用户不必再「关闭再启用」。开关状态以 store 里的实际状态为准（弹窗里的草稿可能过时）。
   */
  async function update(rule: ForwardRule): Promise<void> {
    const idx = rules.value.findIndex(r => r.id === rule.id)
    if (idx < 0) return
    const prev = rules.value[idx]
    const enabled = prev.enabled
    rules.value[idx] = { ...rule, enabled, updatedAt: Date.now() }
    void persist()
    if (enabled && tunnelConfigChanged(prev, rule)) await restartTunnel(rule.id)
  }

  function remove(id: string): void {
    rules.value = rules.value.filter(r => r.id !== id)
    void persist()
  }

  /** 仅持久化规则的「测试目标」（地址/端口），不重建隧道 —— 测试目标只用于连通性探测，
   *  输入即落盘，避免用户配置后未点「保存」、关闭弹窗/窗口导致丢失 */
  function updateTestTarget(
    rule: Pick<ForwardRule, 'id' | 'remoteTargetHost' | 'remoteTargetPort'>,
  ): void {
    const idx = rules.value.findIndex(r => r.id === rule.id)
    if (idx < 0) return
    rules.value[idx] = {
      ...rules.value[idx],
      remoteTargetHost: rule.remoteTargetHost,
      remoteTargetPort: rule.remoteTargetPort,
      updatedAt: Date.now(),
    }
    void persist()
  }

  function duplicate(id: string): void {
    const src = rules.value.find(r => r.id === id)
    if (!src) return
    const copy = defaultRule(src.mode)
    Object.assign(copy, src, {
      id: copy.id,
      name: `${src.name} 副本`,
      enabled: false,
      createdAt: Date.now(),
      updatedAt: Date.now(),
    })
    rules.value.unshift(copy)
    void persist()
  }

  /**
   * 启停单条：真实调用后端建立/拆除隧道。
   * 只有后端成功返回才把开关置位（失败保持原状并提示原因），
   * 避免出现「界面显示已开启、实际没有隧道」的假状态。
   */
  async function setEnabled(id: string, enabled: boolean): Promise<void> {
    const r = rules.value.find(x => x.id === id)
    if (!r || r.enabled === enabled) return

    if (enabled) {
      const err = validateRule(r)
      if (err) {
        ElMessage.warning(useI18nStore().t(err))
        return
      }
      try {
        await invoke<number>('port_forward_start', { rule: await toBackendRule(r), ...(await proxyInvokeArgs()) })
        r.enabled = true
      } catch (e) {
        ElMessage.error(`隧道启动失败：${(e as Error)?.message ?? e}`)
        return
      }
    } else {
      try {
        await invoke('port_forward_stop', { id })
      } catch (e) {
        console.warn('[portForward] 停止隧道失败:', e)
      }
      r.enabled = false
    }
    r.updatedAt = Date.now()
    void persist()
  }

  /** 批量启停 */
  async function setEnabledMany(ids: string[], enabled: boolean): Promise<void> {
    for (const id of ids) {
      await setEnabled(id, enabled)
    }
  }

  /** 批量删除 */
  function removeMany(ids: string[]): void {
    const set = new Set(ids)
    rules.value = rules.value.filter(r => !set.has(r.id))
    void persist()
  }

  return {
    rules, loaded,
    load, persist, add, update, remove, duplicate,
    updateTestTarget,
    setEnabled, setEnabledMany, removeMany,
  }
})
