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
 * 配置云同步 Store（仅登录用户）
 * 作者：JC
 *
 * 边界与不变量（改动前请先读完，这里的每条都对应一种丢数据场景）：
 * 1. 只同步登录账号的数据；未登录(anon)的本地离线配置不上云，只能由「导入本地离线配置」并入账号；
 * 2. 先拉后推：任何一次同步都先把云端文档拉下来做三方合并，再把合并结果推上去，
 *    顺序反了就会用"还没见过对端改动"的旧数据覆盖云端；
 * 3. 合并必须落在 SQLite 之后，任何一步失败都不推进 state（宁可多推一次，不可少推一次丢改动）；
 * 4. 自动路径永不做大面积删除（熔断），只有用户在界面上明确选择才放行；
 * 5. 本地写盘(Sync SQLite)与云同步解耦：本地是真相源，云是副本；被强杀最多丢一次上传，不丢本地数据。
 */
import { defineStore } from 'pinia'
import { ref, computed, watch } from 'vue'
import { invoke, isTauriEnv } from '@/api'
import { useAuthStore } from '@/stores/auth'
import { useSettingsStore } from '@/stores/settings'
import { useHostStore, EK_HOST, EK_NS, TOMBSTONE_TTL_MS, ANON_OWNER, HOST_DATA_VERSION } from '@/stores/host'
import type { HostDoc, HostItem, Namespace } from '@/stores/host'
import {
  mergeSides, mergeFlat, overwriteSide, localDigest,
  type Side, type EntryMeta, type MergeStats,
} from '@/utils/syncMerge'
import { onConfigChanged, type ConfigDocKey } from '@/utils/syncSignals'
import { notifySyncCoor, onSyncCoor } from '@/utils/crossWindow'
import { deviceId } from '@/utils/deviceId'
import { friendlyError, friendlyText } from '@/utils/errorHandler'
import { securityGetSync, securitySetSync, type SecurityPolicySync } from '@/utils/securityPassword'

/** 参与云同步的文档键(与服务端白名单一致; layout 暂不纳入, 它是本机窗口几何) */
export type SyncDocKey = 'hosts' | 'prefs'
export const SYNC_DOCS: SyncDocKey[] = ['hosts', 'prefs']

/** 变更防抖: 连续改名/拖动只推一次 */
const DEBOUNCE_MS = 20_000
/** 最长持留: 用户一直在改, 也要保证两分钟内上一次云 */
const MAX_HOLD_MS = 120_000
/** 定时兜底: 拉取其它设备已经推上去的改动 */
const TICK_MS = 10 * 60_000
/** 启动后延迟: 避开首屏渲染与 init_db 的兜底恢复 */
const STARTUP_DELAY_MS = 4000
/** 骤降熔断: 合并结果条目数不足本地一半时, 自动路径拒绝落地 */
const SHRED_RATIO = 0.5
/** 应用远端结果前的本地快照保留份数 */
const BACKUP_KEEP = 5
/** 单轮同步里因并发冲突重试的最大次数(超过就留到下次) */
const MAX_RETRY_PASS = 3

const STATE_KEY_PREFIX = 'sync_state'
const BACKUP_KEY_PREFIX = 'sync_backup'

/** 云端文档(与 Rust SyncDoc 对应) */
interface CloudDoc {
  docKey: string
  rev: number
  schemaVer?: number | null
  entryCount?: number | null
  payload?: string | null
  digest?: string | null
  sourceDevice?: string | null
  updatedAt?: string | null
}

/** 推送结果(冲突不是错误: accepted=false 且带云端最新文档) */
interface PushResult {
  accepted: boolean
  rev?: number | null
  digest?: string | null
  serverTime?: number | null
  doc?: CloudDoc | null
  message?: string | null
}

/** 每份文档的同步基线 */
interface DocState {
  /** 云端当前修订号 */
  rev: number
  /** 上次成功推送内容的本地摘要(与云端 digest 算法无关, 只用于判"本地有没有变") */
  digest: string
  /** 三方合并的共同祖先 */
  base: unknown
  /** 本地内容最后一次真正变化的时间(偏好类文档的字段级 LWW 依据) */
  stamp?: EntryMeta
  syncedAt: number
}

interface StateFile {
  docs: Partial<Record<SyncDocKey, DocState>>
  lastSyncAt: number
  /** 本机该账号是否已处理过"导入离线配置"(asked 之后不再打扰) */
  offlineImport?: { status: 'done' | 'skipped'; at: number }
}

/** 偏好文档外壳 */
interface PrefsDoc {
  version: number
  updatedAt: number
  deviceId: string
  values: Record<string, unknown>
  /** 安全密码策略（开启时随 prefs 同步；未开启为 null/缺失） */
  security?: SecurityPolicySync | null
}

export interface OfflinePreview {
  /** 离线(未登录时)配置的条目数 */
  offlineCount: number
  /** 当前账号配置的条目数 */
  accountCount: number
  /** 合并后的条目数 */
  mergedCount: number
  /** 合并时需要用户核对的冲突数 */
  conflictCount: number
}

/** 熔断详情: 自动合并结果比本地少太多, 不经过用户确认绝不落地 */
export interface BlockedInfo {
  doc: SyncDocKey
  /** 合并后剩余条目数 */
  kept: number
  /** 本地当前条目数 */
  local: number
}

/**
 * 冲突解决策略
 * - merge: 智能合并, 双端取并集后一起推上云(不丢任何一端的东西)
 * - cloud: 云端覆盖本地, 放弃本机改动
 * - local: 以本地为准, 放弃云端版本
 */
export type ConflictStrategy = 'merge' | 'cloud' | 'local'

/** 待用户裁决的冲突: 两端相对共同祖先都改过, 自动选边就会丢数据, 必须让人来定 */
export interface ConflictInfo {
  /** 发生冲突的文档 */
  docs: SyncDocKey[]
  /** 云端当前修订号 */
  remoteRev: number
  /** 本地基线修订号 */
  localRev: number
  /** 冲突发生的时间 */
  detectedAt: number
}

/** 离线导入结果(供 UI 拼提示文案) */
export interface OfflineImportResult {
  ok: boolean
  mode: 'merge' | 'overwrite'
  added: number
  updated: number
  removed: number
}

export const useSyncStore = defineStore('sync', () => {
  const auth = useAuthStore()
  const settingsStore = useSettingsStore()
  const hostStore = useHostStore()

  const busy = ref(false)
  const lastSyncAt = ref(0)
  const lastError = ref('')
  /** 待推送的文档(界面上显示"待同步 N 项") */
  const pending = ref<SyncDocKey[]>([])
  const conflictCount = ref(0)
  /** 非空表示自动同步被熔断拦下, 等待用户在界面上确认(文案由 UI 层负责, 这里只给数据) */
  const blocked = ref<BlockedInfo | null>(null)
  /** 非空表示两端都改过、已挂起同步, 等用户在三个策略里选一个 */
  const conflict = ref<ConflictInfo | null>(null)
  const offlinePromptVisible = ref(false)
  const offlinePreview = ref<OfflinePreview | null>(null)

  const loggedIn = computed(() => !!auth.user?.id)
  const autoSync = computed(() => settingsStore.settings.autoSync === true)
  /** 可以自动同步: 登录 + 开关开 + Tauri 环境 */
  const active = computed(() => loggedIn.value && autoSync.value && isTauriEnv())

  let state: StateFile = { docs: {}, lastSyncAt: 0 }
  /** 已加载的 state 属于哪个归属域; 换号必须重读 */
  let stateOwner = ''
  let running: Promise<void> | null = null
  let again = false
  let debounceTimer: number | null = null
  let holdStartAt = 0
  let tickTimer: number | null = null
  let stopConfigListener: (() => void) | null = null
  let stopAuthWatch: (() => void) | null = null
  /** 正在应用远端结果: 屏蔽由此触发的自身变更信号, 否则每次拉取都会立刻回推一次 */
  let applying = false
  let initialized = false
  /** 本窗口是否同步执行者：多 WebView 只允许一个窗口真正跑同步，其余只转发请求 */
  let isLeader = true

  function docState(key: SyncDocKey): DocState {
    let st = state.docs[key]
    if (!st) {
      st = { rev: 0, digest: '', base: null, syncedAt: 0 }
      state.docs[key] = st
    }
    return st
  }

  /**
   * 出错文案：只返回给用户看的中文友好提示。
   *
   * 注意：lastError 同时被 manualPull/manualPush 用作"成功/失败"判定（依赖非空），
   * 所以这里必须返回非空字符串；真实技术细节由 api/index.ts 的 invoke 包装自动上报，
   * 绝不再回显给用户。
   */
  function errText(e: unknown): string {
    return friendlyError(e)
  }

  // ===== 本地状态存取 =====

  async function loadState(): Promise<void> {
    const owner = auth.user?.id ? `u${auth.user.id}` : ''
    if (!owner) {
      state = { docs: {}, lastSyncAt: 0 }
      stateOwner = ''
      lastSyncAt.value = 0
      return
    }
    if (stateOwner === owner) return
    try {
      const json = await invoke<string | null>('get_setting', { key: `${STATE_KEY_PREFIX}:${owner}` })
      const parsed = json ? (JSON.parse(json) as StateFile) : null
      state = parsed && typeof parsed === 'object' ? { docs: parsed.docs || {}, lastSyncAt: parsed.lastSyncAt || 0, offlineImport: parsed.offlineImport } : { docs: {}, lastSyncAt: 0 }
    } catch {
      state = { docs: {}, lastSyncAt: 0 }
    }
    stateOwner = owner
    lastSyncAt.value = state.lastSyncAt || 0
  }

  async function saveState(): Promise<void> {
    if (!stateOwner) return
    state.lastSyncAt = lastSyncAt.value
    try {
      await invoke('set_setting', { key: `${STATE_KEY_PREFIX}:${stateOwner}`, value: JSON.stringify(state) })
    } catch (e) {
      console.warn('[sync] 状态保存失败:', e)
    }
  }

  /** 应用远端结果前把当前本地内容留一份快照, 滚动保留最近 BACKUP_KEEP 份 */
  async function backupBeforeApply(key: SyncDocKey): Promise<void> {
    if (!isTauriEnv()) return
    try {
      const ts = Date.now()
      const bp = await buildPayload(key)
      await invoke('set_setting', { key: `${BACKUP_KEY_PREFIX}:${key}:${stateOwner || 'anon'}:${ts}`, value: bp.payload })
      const rows = await invoke<{ key: string }[]>('list_all_settings')
      const prefix = `${BACKUP_KEY_PREFIX}:${key}:${stateOwner || 'anon'}:`
      const olds = rows.map(r => r.key).filter(k => k.startsWith(prefix)).sort().slice(0, -BACKUP_KEEP)
      for (const k of olds) {
        try { await invoke('delete_setting', { key: k }) } catch { /* 单条清理失败不影响主流程 */ }
      }
    } catch (e) {
      console.warn('[sync] 本地快照失败:', e)
    }
  }

  // ===== 文档 <-> Side =====

  /**
   * 关闭「同步会话私钥」时，私钥内容只留在本机：既不推送上云，也不参与摘要比较。
   * 摘要必须同样基于剔除后的形态 —— 否则本地（有私钥）与云端（无私钥）永远不相等，
   * 每一轮都会被判成"本地已改"，进而反复触发冲突弹窗。
   */
  function stripSshKeyContent(doc: HostDoc): HostDoc {
    if (settingsStore.settings.syncSshKeys) return doc
    let changed = false
    const hosts = doc.hosts.map(h => {
      if (!h.config?.keyContent) return h
      changed = true
      const { keyContent: _dropped, ...rest } = h.config
      return { ...h, config: rest }
    })
    return changed ? { ...doc, hosts } : doc
  }

  /** 应用远端结果前把本机已有的私钥补回去：云端没有私钥，不代表本机也没有 */
  function restoreSshKeyContent(doc: HostDoc, local: HostDoc): HostDoc {
    if (settingsStore.settings.syncSshKeys) return doc
    const localKeys = new Map<string, NonNullable<HostDoc['hosts'][number]['config']>>()
    for (const h of local.hosts) {
      if (h.config?.keyContent) localKeys.set(h.id, h.config)
    }
    if (!localKeys.size) return doc
    return {
      ...doc,
      hosts: doc.hosts.map(h => {
        const c = localKeys.get(h.id)
        if (!c || h.config?.keyContent) return h
        return {
          ...h,
          config: { ...(h.config || {}), keyContent: c.keyContent, keyName: h.config?.keyName || c.keyName },
        }
      }),
    }
  }

  function hostSideOf(doc: HostDoc): Side {
    const src = stripSshKeyContent(doc)
    const entries: Record<string, unknown> = {}
    for (const h of src.hosts) entries[EK_HOST + h.id] = h
    for (const n of src.namespaces) entries[EK_NS + n.id] = n
    return { entries, meta: doc.meta || {}, tombstones: doc.tombstones || [] }
  }

  /**
   * 合并结果 -> 文档。数组顺序沿用本地(命名空间页签顺序、同级展示顺序都是有意义的),
   * 只把本地没有的新条目按 id 追加, 否则每次合并都会把用户排好的顺序打乱。
   */
  function hostDocOf(side: Side, local: HostDoc): HostDoc {
    const hosts: HostItem[] = []
    const namespaces: Namespace[] = []
    const usedHost = new Set<string>()
    const usedNs = new Set<string>()
    for (const n of local.namespaces) {
      const key = EK_NS + n.id
      const val = side.entries[key]
      if (val) { namespaces.push(val as Namespace); usedNs.add(n.id) }
    }
    for (const h of local.hosts) {
      const key = EK_HOST + h.id
      const val = side.entries[key]
      if (val) { hosts.push(val as HostItem); usedHost.add(h.id) }
    }
    const extraNs = Object.keys(side.entries)
      .filter(k => k.startsWith(EK_NS) && !usedNs.has(k.slice(EK_NS.length)))
      .sort()
      .map(k => side.entries[k] as Namespace)
    const extraHosts = Object.keys(side.entries)
      .filter(k => k.startsWith(EK_HOST) && !usedHost.has(k.slice(EK_HOST.length)))
      .sort()
      .map(k => side.entries[k] as HostItem)
    namespaces.push(...extraNs)
    hosts.push(...extraHosts)
    const currentOk = namespaces.some(n => n.id === local.currentNamespaceId)
    return {
      version: HOST_DATA_VERSION,
      namespaces,
      currentNamespaceId: currentOk ? local.currentNamespaceId : (namespaces[0]?.id ?? 'public'),
      hosts,
      meta: side.meta,
      tombstones: side.tombstones,
    }
  }

  /** 主机条目指纹: 两端各自新建了同一台主机的记录时用来识别"其实是同一个东西" */
  function hostFingerprint(val: unknown): string | null {
    const h = val as HostItem | undefined
    if (!h || typeof h !== 'object' || h.type !== 'host') return null
    const addr = String(h.addr || '').trim()
    if (!addr) return null
    return `${addr}|${String(h.name || '').trim()}|${h.namespaceId || ''}`
  }

  /** 安全密码策略同步的容错封装：凭据库异常时降级为"不同步"，不阻断整体同步 */
  async function safeGetSec(): Promise<SecurityPolicySync | null> {
    try { return await securityGetSync() } catch { return null }
  }
  async function safeSetSec(p: SecurityPolicySync | null): Promise<void> {
    try { await securitySetSync(p) } catch { /* 凭据库异常忽略，下次同步再对齐 */ }
  }

  /** 本机当前内容 -> 待推送文档(应用远端前的本地备份、强制推送共用) */
  async function buildPayload(key: SyncDocKey): Promise<{ payload: string; schemaVer: number; entryCount: number }> {
    if (key === 'hosts') {
      const doc = stripSshKeyContent(hostStore.snapshot())
      return { payload: JSON.stringify(doc), schemaVer: HOST_DATA_VERSION, entryCount: doc.hosts.length + doc.namespaces.length }
    }
    const values = settingsStore.syncableValues()
    const mut: PrefsDoc = { version: 1, updatedAt: Date.now(), deviceId: deviceId(), values }
    // 安全密码策略随 prefs 上云（开启时）
    const sec = await safeGetSec()
    if (sec) mut.security = sec
    return { payload: JSON.stringify(mut), schemaVer: 1, entryCount: Object.keys(values).length }
  }

  /**
   * 本地内容摘要。
   * 口径必须与 syncHosts/syncPrefs 写进 st.digest 的完全一致(hosts 取整份文档, prefs 只取字段表),
   * 否则会误判"本地已同步"而永久漏掉一次改动。
   */
  function localDigestOf(key: SyncDocKey): string {
    return key === 'hosts'
      ? localDigest(stripSshKeyContent(hostStore.snapshot()))
      : localDigest(settingsStore.syncableValues())
  }

  function parseHostDoc(payload: string): HostDoc | null {
    try {
      const cfg = JSON.parse(payload)
      if (!cfg || typeof cfg !== 'object') return null
      // 版本护栏: 云端是更高版本写的结构时, 本机按旧结构去解释再落盘就会把数据改坏,
      // 宁可不动本地数据并提示升级客户端, 也不能"老版本覆盖新版本"
      if (Number(cfg.version) > HOST_DATA_VERSION) return null
      const list: HostItem[] = Array.isArray(cfg.hosts) ? cfg.hosts : []
      const ns: Namespace[] = Array.isArray(cfg.namespaces) ? cfg.namespaces : []
      const meta: Record<string, EntryMeta> = cfg.meta && typeof cfg.meta === 'object' ? cfg.meta : {}
      const fill = (key: string) => {
        if (!meta[key]) meta[key] = { updatedAt: 0, deviceId: '' }
      }
      for (const h of list) fill(EK_HOST + h.id)
      for (const n of ns) fill(EK_NS + n.id)
      return {
        version: HOST_DATA_VERSION,
        namespaces: ns,
        currentNamespaceId: cfg.currentNamespaceId || 'public',
        hosts: list,
        meta,
        tombstones: Array.isArray(cfg.tombstones) ? cfg.tombstones : [],
      }
    } catch {
      return null
    }
  }

  function parsePrefsDoc(payload: string): PrefsDoc | null {
    try {
      const cfg = JSON.parse(payload) as PrefsDoc
      if (!cfg || typeof cfg !== 'object' || !cfg.values || typeof cfg.values !== 'object') return null
      return {
        version: Number(cfg.version) || 1,
        updatedAt: Number(cfg.updatedAt) || 0,
        deviceId: String(cfg.deviceId ?? ''),
        values: cfg.values,
        security: cfg.security === undefined ? null : cfg.security,
      }
    } catch {
      return null
    }
  }

  // ===== 单份文档的一轮"拉-合-推" =====

  async function syncHosts(cloud: CloudDoc | null, force: boolean): Promise<boolean> {
    const st = docState('hosts')
    if (blocked.value?.doc === 'hosts') blocked.value = null
    const localDoc = hostStore.snapshot()
    const localSide = hostSideOf(localDoc)

    // 版本护栏: 云端有正文却解不出来(多半是比本机更高的结构版本), 绝不能当成"云端为空"
    // 继续往下走 —— 那样会把本机旧版本整份推上去覆盖云端, 正是要防的"老版本覆盖新版本"。
    let remoteSide: Side | null = null
    if (cloud?.payload) {
      const parsed = parseHostDoc(cloud.payload)
      if (!parsed) {
        lastError.value = '云端配置来自更高版本的 JC Shell，已暂停同步以避免覆盖，请升级客户端后重试'
        return false
      }
      remoteSide = hostSideOf(parsed)
    }
    // 修订号护栏: 云端 rev 落后于本地基线说明拿到了过期快照, 同样不能据此回推
    if (cloud && st.rev > 0 && cloud.rev < st.rev) {
      lastError.value = '云端配置版本落后于本机，已跳过本次同步'
      return false
    }
    const remoteRev = cloud ? cloud.rev : 0
    let mergedSide: Side
    let stats: MergeStats = { added: 0, updated: 0, removed: 0, conflicts: 0 }

    if (remoteSide) {
      const base = st.base && typeof st.base === 'object' && (st.base as Side).entries ? (st.base as Side) : null
      const res = mergeSides(base, localSide, remoteSide, { tombstoneTtlMs: TOMBSTONE_TTL_MS, fingerprint: hostFingerprint })
      mergedSide = { entries: res.entries, meta: res.meta, tombstones: res.tombstones }
      stats = res.stats
    } else {
      mergedSide = localSide
    }
    conflictCount.value += stats.conflicts

    // 熔断: 自动路径不接受"条目大面积消失"(云端某次写坏或被别的设备清空时, 不该顺着抹掉本地)
    const localN = Object.keys(localSide.entries).length
    const mergedN = Object.keys(mergedSide.entries).length
    if (!force && localN > 0 && mergedN < localN * SHRED_RATIO) {
      blocked.value = { doc: 'hosts', kept: mergedN, local: localN }
      return false
    }

    // mergedDoc 是"推送形态"：关闭私钥同步时其中不含 keyContent
    const mergedDoc = hostDocOf(mergedSide, localDoc)
    const mergedSum = localDigest(mergedDoc)
    // 落到本机的版本要把本机私钥补回去 —— 云端没有私钥，不代表本机也没有
    const docToApply = restoreSshKeyContent(mergedDoc, localDoc)
    if (mergedSum !== localDigest(stripSshKeyContent(localDoc))) {
      await backupBeforeApply('hosts')
      applying = true
      try { await hostStore.applyDoc(docToApply) } finally { applying = false }
    }

    if (mergedSum === st.digest && remoteRev === st.rev) return true
    const payload = JSON.stringify(mergedDoc)
    const ok = await pushDoc('hosts', payload, remoteRev || null, HOST_DATA_VERSION,
      mergedDoc.hosts.length + mergedDoc.namespaces.length, () => ({ base: hostSideOf(mergedDoc), digest: localDigestOf('hosts') }))
    return ok
  }

  async function syncPrefs(cloud: CloudDoc | null, force: boolean): Promise<boolean> {
    const st = docState('prefs')
    if (blocked.value?.doc === 'prefs') blocked.value = null
    const localValues = settingsStore.syncableValues()
    const localSum = localDigest(localValues)
    const remoteDoc = cloud && cloud.payload ? parsePrefsDoc(cloud.payload) : null
    const remoteRev = cloud ? cloud.rev : 0
    const localChanged = localSum !== st.digest
    let values = localValues
    // 本地没动就沿用上次发布的时间戳(不伪造"我刚改过"), 动了才打当前时间去参与字段级 LWW
    let stamp: EntryMeta = !localChanged && st.stamp ? st.stamp : { updatedAt: Date.now(), deviceId: deviceId() }

    if (remoteDoc) {
      if (!localChanged) {
        // 本地一点没改: 直接接受云端, 不做无意义的字段比较, 也不产生冲突计数
        values = remoteDoc.values
        stamp = { updatedAt: remoteDoc.updatedAt || Date.now(), deviceId: remoteDoc.deviceId || st.stamp?.deviceId || '' }
      } else {
        const base = (st.base && typeof st.base === 'object') ? (st.base as Record<string, unknown>) : null
        const res = mergeFlat(base, localValues, remoteDoc.values,
          st.stamp ?? { updatedAt: 0, deviceId: '' },
          { updatedAt: remoteDoc.updatedAt || 0, deviceId: remoteDoc.deviceId || '' },
          [])
        values = { ...localValues, ...res.values }
        conflictCount.value += res.stats.conflicts
      }
    }
    if (!force && Object.keys(values).length === 0 && Object.keys(localValues).length > 0) {
      blocked.value = { doc: 'prefs', kept: 0, local: Object.keys(localValues).length }
      return false
    }
    const sum = localDigest(values)
    if (sum !== localDigest(localValues)) {
      await backupBeforeApply('prefs')
      applying = true
      try { settingsStore.applySyncedValues(values) } finally { applying = false }
    }
    // ===== 安全密码策略同步：last-write-wins by updatedAt，胜出策略落本机凭据库并随文档上云 =====
    const localSec = await safeGetSec()
    const remoteSec = remoteDoc?.security ?? null
    let outSec: SecurityPolicySync | null = localSec
    if (remoteSec && (!localSec || remoteSec.updated_at >= localSec.updated_at)) {
      outSec = remoteSec
    }
    await safeSetSec(outSec)
    if (sum === st.digest && remoteRev === st.rev) return true
    const payload = JSON.stringify({ version: 1, updatedAt: stamp.updatedAt, deviceId: stamp.deviceId, values, security: outSec } as PrefsDoc)
    return pushDoc('prefs', payload, remoteRev || null, 1, Object.keys(values).length,
      () => ({ base: values, digest: localDigestOf('prefs'), stamp }))
  }

  /**
   * 推送并把 state 推进到"已同步"。
   * @param applyOnAccepted 成功后要落进 state 的基线(祖先快照 + 本地摘要)
   */
  async function pushDoc(
    key: SyncDocKey,
    payload: string,
    baseRev: number | null,
    schemaVer: number,
    entryCount: number,
    applyOnAccepted: () => Partial<DocState>,
  ): Promise<boolean> {
    let res: PushResult
    try {
      res = await invoke<PushResult>('sync_push_doc', {
        dto: { docKey: key, baseRev, schemaVer, entryCount, payload, deviceId: deviceId() },
      })
    } catch (e) {
      lastError.value = errText(e)
      return false
    }
    const st = docState(key)
    if (res?.accepted) {
      Object.assign(st, applyOnAccepted(), { rev: Number(res.rev) || st.rev + 1, syncedAt: Number(res.serverTime) || Date.now() })
      pending.value = pending.value.filter(k => k !== key)
      lastError.value = ''
      return true
    }
    // 冲突: 本轮先不推, 把云端最新版本记进 state, 由外层再走一遍"拉-合-推"
    if (res?.doc) {
      st.rev = Number(res.doc.rev) || st.rev
      again = true
    }
    // res.message 来自服务端，可能夹带技术细节，收敛后再展示给用户
    lastError.value = res?.message
      ? friendlyText(res.message)
      : '云端配置已被其它设备更新，正在重新合并'
    return false
  }

  // ===== 冲突裁决 =====

  /**
   * 挂起一份文档的同步, 等用户裁决。
   * <p>两端都改过时"自动选一边"没有正确答案 —— 本地优先会丢云端改动, 云端优先会丢本地改动,
   * 所以这里什么都不做, 只把冲突摆到界面上。</p>
   */
  function queueConflict(doc: SyncDocKey, remoteRev: number, localRev: number): void {
    if (conflict.value) {
      if (!conflict.value.docs.includes(doc)) conflict.value.docs = [...conflict.value.docs, doc]
      return
    }
    conflict.value = { docs: [doc], remoteRev, localRev, detectedAt: Date.now() }
    // 广播给所有窗口(含 follower 的设置窗口)，让裁决弹窗在任意窗口都能弹出
    notifySyncCoor({ kind: 'conflict', info: conflict.value })
    console.warn(
      `[sync] 检测到版本冲突 doc=${doc} 本机基线 rev=${localRev} 云端 rev=${remoteRev}，已挂起等待用户裁决`
    )
  }

  /** 云端覆盖本地: 放弃本机改动, 整份套用云端文档(不做合并, 也不回推) */
  async function applyCloudOverLocal(key: SyncDocKey, cloud: CloudDoc | null): Promise<void> {
    const st = docState(key)
    // 状态行不带正文, 需要时再取一次完整文档
    let doc = cloud && cloud.payload ? cloud : null
    if (!doc?.payload) {
      try {
        doc = await invoke<CloudDoc | null>('sync_pull_doc', { docKey: key })
      } catch (e) {
        lastError.value = errText(e)
        return
      }
    }
    if (!doc?.payload) return // 云端确实没有这份文档: 没有可覆盖的内容, 保持本地不动

    if (key === 'hosts') {
      const parsed = parseHostDoc(doc.payload)
      if (!parsed) {
        lastError.value = '云端配置来自更高版本的 JC Shell，已暂停同步以避免覆盖，请升级客户端后重试'
        return
      }
      const localBefore = hostStore.snapshot()
      await backupBeforeApply('hosts')
      applying = true
      try { await hostStore.applyDoc(restoreSshKeyContent(parsed, localBefore)) } finally { applying = false }
      // 用落地后的真实本地摘要记账：保证下一轮 localDigestOf 与基线完全一致，
      // 否则「应用云端内容」会因摘要口径差一截而被误判成"本地又改了"，引发反复冲突
      st.digest = localDigestOf('hosts')
      st.base = hostSideOf(parsed)
    } else {
      const parsed = parsePrefsDoc(doc.payload)
      if (!parsed) return
      await backupBeforeApply('prefs')
      applying = true
      try { settingsStore.applySyncedValues(parsed.values) } finally { applying = false }
      if (parsed.security) await safeSetSec(parsed.security)
      const vals = settingsStore.syncableValues()
      st.digest = localDigestOf('prefs')
      st.base = vals
      st.stamp = { updatedAt: parsed.updatedAt, deviceId: parsed.deviceId }
    }
    st.rev = Number(doc.rev) || st.rev
    st.syncedAt = Date.now()
    pending.value = pending.value.filter(k => k !== key)
  }

  /**
   * 用户对挂起的冲突做出选择。
   * - merge 智能合并: 双端取并集, 合并结果推回云端(任何一端的东西都不丢)
   * - cloud 云端覆盖本地: 放弃本机配置
   * - local 以本地为准: 放弃云端版本
   */
  async function resolveConflict(strategy: ConflictStrategy): Promise<void> {
    if (!conflict.value) return
    // follower 窗口没有执行引擎：把用户的选择转交给 leader，由它真正跑同步
    if (!isLeader) {
      notifySyncCoor({ kind: 'resolve', strategy })
      conflict.value = null
      return
    }
    conflict.value = null
    // 通知所有窗口（含 follower）冲突已裁决，让它们的弹窗跟着关闭
    notifySyncCoor({ kind: 'resolved' })
    if (strategy === 'cloud') {
      await syncOnce({ force: true, strategy: 'cloud', skipConflict: true })
      return
    }
    if (strategy === 'local') {
      await syncOnce({ force: true, pushOnly: true, skipConflict: true })
      return
    }
    await syncOnce({ force: true, skipConflict: true })
  }

  // ===== 一轮完整同步 =====

  async function runPass(
    force: boolean,
    pushOnly: boolean,
    strategy: 'cloud' | null,
    skipConflict: boolean,
  ): Promise<void> {
    await loadState()
    let states: CloudDoc[] = []
    // 状态行不带 payload, 代价极小, 因此 pushOnly(强制推送) 也必须取:
    // 它是推送所需的 baseRev 唯一来源。少了这一步, pushOnly 永远带 baseRev=null,
    // 而服务端对已存在的文档用 rev.equals(baseRev) 判定, null 恒不相等 -> 一律回冲突,
    // 重试 N 轮仍是冲突, 「推送本地配置到云端」将永不生效。
    try {
      states = await invoke<CloudDoc[]>('sync_pull_states')
    } catch (e) {
      lastError.value = errText(e)
      return
    }
    const byKey = new Map(states.map(s => [s.docKey, s]))
    for (const key of SYNC_DOCS) {
      const st = docState(key)
      // pushOnly 是"以本机为准": 不看云端正文, 但仍要拿云端 rev 当并发基线
      const cloud = byKey.get(key) ?? null
      // 云端状态行不带正文(payload 为 null), 只有 rev 变了才值得把整份文档拖回来
      let full = cloud
      if (!pushOnly && cloud && !cloud.payload && cloud.rev !== st.rev) {
        try {
          full = await invoke<CloudDoc | null>('sync_pull_doc', { docKey: key })
        } catch (e) {
          lastError.value = errText(e)
          continue
        }
      }
      const localChanged = localDigestOf(key) !== st.digest
      const remoteChanged = !!cloud && cloud.rev !== st.rev

      // 真冲突: 两端相对共同祖先都改过, 自动选任何一边都会抹掉另一边的改动,
      // 因此挂起本轮, 等用户在"智能合并 / 云端覆盖本地 / 以本地为准"里裁决。
      // st.rev>0 才算有共同祖先 —— 首次握手没有祖先, 那是一次性取并集, 不是冲突。
      if (!pushOnly && !strategy && !skipConflict && localChanged && remoteChanged && st.rev > 0) {
        queueConflict(key, cloud?.rev ?? 0, st.rev)
        continue
      }
      // 云端覆盖本地: 放弃本机改动, 整份套用云端内容, 不合并也不回推
      if (strategy === 'cloud') {
        await applyCloudOverLocal(key, cloud)
        continue
      }

      if (!force && !localChanged && (!full || full.rev === st.rev)) continue
      if (pushOnly) {
        // 强制推送(以本机为准): payload / base / digest 必须取自同一份快照,
        // 否则推送期间用户又改了东西, 会把没推上去的改动误记成"已同步"而永久丢失
        const stamp: EntryMeta = { updatedAt: Date.now(), deviceId: deviceId() }
        let payload: string
        let schemaVer: number
        let entryCount: number
        let base: unknown
        let digest: string
        if (key === 'hosts') {
          const doc = hostStore.snapshot()
          const out = stripSshKeyContent(doc)
          payload = JSON.stringify(out)
          schemaVer = HOST_DATA_VERSION
          entryCount = out.hosts.length + out.namespaces.length
          base = hostSideOf(doc)
          digest = localDigest(out)
        } else {
          const vals = settingsStore.syncableValues()
          const sec = await safeGetSec()
          const mut: PrefsDoc = { version: 1, updatedAt: stamp.updatedAt, deviceId: stamp.deviceId, values: vals }
          if (sec) mut.security = sec
          payload = JSON.stringify(mut)
          schemaVer = 1
          entryCount = Object.keys(vals).length
          base = vals
          digest = localDigest(vals)
        }
        // 云端无此文档时必须传 null: 服务端对首建要求 baseRev 为空, 传 0 会被拒
        await pushDoc(key, payload, cloud?.rev || null, schemaVer, entryCount,
          () => ({ base, digest, stamp }))
        continue
      }
      if (key === 'hosts') await syncHosts(full, force)
      else await syncPrefs(full, force)
    }
    lastSyncAt.value = Date.now()
    await saveState()
  }

  /**
   * 同步入口(单飞 + 尾随一轮)。
   * <p>同一时刻只允许一个同步事务: 并发跑会有两个 pass 都基于旧 rev 推送, 其中一个必然被拒,
   * 白耗一次网络还可能把用户短暂带到"看起来丢了改动"的中间态。</p>
   */
  function syncOnce(opts: {
    force?: boolean
    pushOnly?: boolean
    /** 'cloud' = 云端覆盖本地, 不走合并 */
    strategy?: 'cloud'
    /** 跳过冲突拦截(用户在冲突弹窗里做出选择后走这条路) */
    skipConflict?: boolean
  } = {}): Promise<void> {
    if (!isTauriEnv() || !loggedIn.value) return Promise.resolve()
    // 冲突未裁决前不再跑同步: 否则每轮都会重新判一次冲突, 弹窗反复弹且状态来回抖。
    // 必须显式落一条错误, 否则 manualPull/manualPush 会因 lastError 为空而误报"同步完成"。
    if (conflict.value && !opts.skipConflict) {
      lastError.value = '存在未处理的同步冲突，请先选择处理方式'
      return Promise.resolve()
    }
    if (running) {
      again = true
      return running
    }
    const force = !!opts.force
    const pushOnly = !!opts.pushOnly
    const strategy = opts.strategy ?? null
    const skipConflict = !!opts.skipConflict
    running = (async () => {
      busy.value = true
      conflictCount.value = 0
      // 必须先清空: manualPull/manualPush 以 lastError 是否为空判断成败,
      // 残留上一轮的错误会让"本次成功"也被报成失败, 界面上还会一直挂着旧错误。
      lastError.value = ''
      try {
        let pass = 0
        do {
          again = false
          pass++
          try {
            await runPass(force, pushOnly, strategy, skipConflict)
          } catch (e) {
            lastError.value = errText(e)
            console.warn('[sync] 同步失败:', e)
          }
        } while (again && pass < MAX_RETRY_PASS)
      } finally {
        busy.value = false
        running = null
        // 排障线索: 会话列表没同步过来时, 这三行能直接定位是"没推上去"还是"没拉下来"
        console.info(
          `[sync] 一轮同步结束 mode=${pushOnly ? 'push' : 'pull'} force=${force} ` +
          `lastSyncAt=${lastSyncAt.value || 0} error=${lastError.value || '无'}`
        )
      }
    })()
    return running
  }

  // ===== 变更触发(防抖 + 最长持留) =====

  function markDirty(key: ConfigDocKey): void {
    if (applying) return
    if (!pending.value.includes(key)) pending.value = [...pending.value, key]
    schedulePush()
  }

  function schedulePush(): void {
    if (!active.value) return
    const now = Date.now()
    if (!holdStartAt) holdStartAt = now
    // 一直在改也要保证 MAX_HOLD_MS 内上一次云, 否则"边改边同步"会永远推不出去
    const wait = Math.max(0, Math.min(DEBOUNCE_MS, MAX_HOLD_MS - (now - holdStartAt)))
    if (debounceTimer !== null) window.clearTimeout(debounceTimer)
    debounceTimer = window.setTimeout(() => {
      debounceTimer = null
      holdStartAt = 0
      void syncOnce()
    }, wait)
  }

  /** 立刻把待推送内容送上云(切走窗口/关闭前调用) */
  function flushNow(): void {
    if (debounceTimer !== null) { window.clearTimeout(debounceTimer); debounceTimer = null; holdStartAt = 0 }
    if (!pending.value.length) return
    void syncOnce()
  }

  // ===== 开关与初始化 =====

  /** 打开开关: 立即做一次全量握手(先拉后推) */
  async function enableAutoSync(): Promise<void> {
    settingsStore.patchSettings({ autoSync: true })
    // follower 窗口改了开关也要让 leader 真正同步一次（patchSettings 已通过配置变更事件转发）
    if (!isLeader) { notifySyncCoor({ kind: 'request' }); return }
    await syncOnce()
  }

  /** 关闭开关: 只停自动触发, 云端数据保留不删, 手动按钮仍可用 */
  function disableAutoSync(): void {
    settingsStore.patchSettings({ autoSync: false })
    if (debounceTimer !== null) { window.clearTimeout(debounceTimer); debounceTimer = null; holdStartAt = 0 }
  }

  /** 手动强制拉取合并(先合后推, 不会丢本地未同步的改动); 返回是否无错 */
  async function manualPull(): Promise<boolean> {
    // follower 窗口没有执行引擎：请 leader 跑，并把结果反馈回来
    if (!isLeader) { notifySyncCoor({ kind: 'request' }); return true }
    await syncOnce({ force: true })
    return !lastError.value
  }

  /** 手动强制推送(以本机为准覆盖云端: 覆盖前云端仍会归档历史版本); 返回是否无错 */
  async function manualPush(): Promise<boolean> {
    if (!isLeader) { notifySyncCoor({ kind: 'request', pushOnly: true }); return true }
    await syncOnce({ force: true, pushOnly: true })
    return !lastError.value
  }

  // ===== 导入本地离线配置 =====

  /** 预览: 离线(anon)配置有多少条, 与当前账号合并会产生多少冲突 */
  async function previewOfflineImport(): Promise<OfflinePreview | null> {
    if (!isTauriEnv()) return null
    const offline = await hostStore.peekDoc(ANON_OWNER)
    if (!offline) return null
    const localDoc = hostStore.snapshot()
    const offlineHosts = offline.hosts.filter(h => h.type === 'host').length
    if (offlineHosts === 0) return null
    const accountHosts = localDoc.hosts.filter(h => h.type === 'host').length
    const res = mergeSides(null, hostSideOf(localDoc), hostSideOf(offline), { tombstoneTtlMs: TOMBSTONE_TTL_MS, fingerprint: hostFingerprint })
    const mergedHosts = Object.keys(res.entries).filter(k => k.startsWith(EK_HOST)).length
    return { offlineCount: offlineHosts, accountCount: accountHosts, mergedCount: mergedHosts, conflictCount: res.stats.conflicts }
  }

  /**
   * 执行导入。
   * @param mode merge=智能合并(取并集 + 条目级 LWW); overwrite=用离线配置直接覆盖当前账号
   */
  async function runOfflineImport(mode: 'merge' | 'overwrite'): Promise<OfflineImportResult> {
    const offline = await hostStore.peekDoc(ANON_OWNER)
    if (!offline) return { ok: false, mode, added: 0, updated: 0, removed: 0 }
    await loadState()
    const st = docState('hosts')
    const localDoc = hostStore.snapshot()
    const target = hostSideOf(localDoc)
    const source = hostSideOf(offline)
    const res = mode === 'overwrite'
      ? overwriteSide(target, source, TOMBSTONE_TTL_MS)
      : mergeSides(st.base && (st.base as Side).entries ? (st.base as Side) : null, target, source, { tombstoneTtlMs: TOMBSTONE_TTL_MS, fingerprint: hostFingerprint })
    const doc = hostDocOf({ entries: res.entries, meta: res.meta, tombstones: res.tombstones }, localDoc)
    await backupBeforeApply('hosts')
    applying = true
    try {
      await hostStore.applyDoc(doc)
    } finally {
      applying = false
    }
    // 离线数据保留不动: 导入是"复制并合并", 不做删除, 出问题还能重来
    state.offlineImport = { status: 'done', at: Date.now() }
    offlinePromptVisible.value = false
    st.digest = ''  // 强制下一轮把合并结果推上云
    pending.value = [...new Set([...pending.value, 'hosts' as SyncDocKey])]
    if (autoSync.value) await syncOnce({ force: true })
    await saveState()
    return { ok: true, mode, added: res.stats.added, updated: res.stats.updated, removed: res.stats.removed }
  }

  /** 弹窗里选"暂不导入": 本机该账号不再自动提醒, 设置里的入口仍在 */
  function skipOfflineImport(): void {
    offlinePromptVisible.value = false
    state.offlineImport = { status: 'skipped', at: Date.now() }
    void saveState()
  }

  /** 当前设备 + 当前账号是否还需要提醒(首次登录弹窗用) */
  async function shouldPromptOfflineImport(): Promise<boolean> {
    if (!loggedIn.value || !isTauriEnv()) return false
    await loadState()
    if (state.offlineImport) return false
    const info = await previewOfflineImport()
    if (!info) return false
    offlinePreview.value = info
    return true
  }

  /** 手动入口(设置里按钮): 无可导入内容时返回 false, 提示文案由调用方负责 */
  async function openOfflinePrompt(): Promise<boolean> {
    const info = await previewOfflineImport()
    if (!info) return false
    offlinePreview.value = info
    offlinePromptVisible.value = true
    return true
  }

  // ===== 生命周期 =====

  /**
   * 由窗口在 auth.load() 之后调用一次。
   * @param role 'leader' = 真正执行同步(主窗口)；'follower' = 只转发请求/展示弹窗(设置窗口)。
   *
   * 多 WebView 架构下，绝不允许两个窗口同时跑同步：彼此会踩同一份云端 rev、互相覆盖
   * 本地基线状态文件，导致「基线 17 / 云端 61」这种 rev 乒乓累加、冲突弹窗刷屏。
   * 因此收敛为单引擎 —— 只有 leader 订阅变更/定时兜底/跑同步，follower 只会：
   *   1) 本窗口改了配置 → 发 'request' 让 leader 跑；
   *   2) 接收 leader 的 'conflict'/'resolved' 广播 → 让裁决弹窗在本窗口也能出现；
   *   3) 用户在弹窗里选了策略 → 发 'resolve' 给 leader 执行。
   */
  async function init(role: 'leader' | 'follower' = 'leader'): Promise<void> {
    if (!isTauriEnv()) return
    isLeader = role === 'leader'

    // follower：只订阅本窗口的配置变更并转发给 leader，再监听冲突广播即可，不跑任何同步循环
    if (!isLeader) {
      if (!stopConfigListener) stopConfigListener = onConfigChanged(() => notifySyncCoor({ kind: 'request' }))
      onSyncCoor((p) => {
        if (p.kind === 'conflict') conflict.value = p.info
        else if (p.kind === 'resolved') conflict.value = null
      })
      return
    }

    if (!stopConfigListener) stopConfigListener = onConfigChanged(doc => markDirty(doc))
    // 接收 follower 的同步请求与裁决选择
    onSyncCoor((p) => {
      if (p.kind === 'request') void syncOnce(p.pushOnly ? { force: true, pushOnly: true } : {})
      else if (p.kind === 'resolve') void resolveConflict(p.strategy)
    })
    if (!stopAuthWatch) {
      stopAuthWatch = watch(() => (auth.user?.id ? `u${auth.user.id}` : ''), async (_next, prev) => {
        // 换号/登出: 上一账号的基线立即作废, 否则会把别人的 rev 当自己的用
        state = { docs: {}, lastSyncAt: 0 }
        stateOwner = ''
        pending.value = []
        blocked.value = null
        if (!prev && !auth.user?.id) return
        await onAccountReady()
      })
    }
    if (initialized) return
    initialized = true
    window.addEventListener('blur', flushNow)
    window.addEventListener('beforeunload', flushNow)
    if (!tickTimer) {
      tickTimer = window.setInterval(() => {
        if (active.value && document.visibilityState === 'visible') void syncOnce()
      }, TICK_MS)
    }
    await onAccountReady()
  }

  /** 登录态就绪后的统一动作: 拉取合并 + 离线导入提醒 */
  async function onAccountReady(): Promise<void> {
    if (!loggedIn.value) return
    await loadState()
    // 登录是用户主动建立的会话: 无论自动同步开关是否打开, 都先对齐一次云端,
    // 否则"另一台电脑改了配置"要等到下一次定时兜底才会出现在这台机器上。
    // 开关管的是"后续本地改动要不要自动上云", 不是"登录后要不要对齐"。
    // 稍微延后: 首屏渲染与本地数据迁移先完成, 同步不去抢启动时的锁
    window.setTimeout(() => { void syncOnce() }, STARTUP_DELAY_MS)
    if (await shouldPromptOfflineImport()) offlinePromptVisible.value = true
  }

  return {
    busy, lastSyncAt, lastError, pending, conflictCount, blocked, conflict,
    offlinePromptVisible, offlinePreview,
    loggedIn, autoSync, active,
    init, syncOnce, markDirty, flushNow,
    enableAutoSync, disableAutoSync, manualPull, manualPush,
    resolveConflict,
    previewOfflineImport, runOfflineImport, skipOfflineImport, openOfflinePrompt,
  }
})
