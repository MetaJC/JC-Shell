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
 * 主机管理 Store
 * 作者：JC
 * 管理命名空间、文件夹、主机列表；支持拖拽排序、父子级关系、持久化。
 * 会话列表按登录账号隔离落盘(host_manager:{owner}), 同一台设备上不同账号各一份。
 * v3 起为云同步补齐元数据：每个条目带变更戳(meta) + 删除墓碑(tombstones)，
 * 两者都由 persist() 与上次落盘内容做差异自动维护，业务代码不必手动打戳。
 */
import { defineStore } from 'pinia'
import { ref, computed, watch } from 'vue'
import { invoke, isTauriEnv } from '@/api'
import { useSettingsStore } from '@/stores/settings'
import { useAuthStore } from '@/stores/auth'
import { encryptText, decryptText, isEncrypted } from '@/utils/crypto'
import { deviceId } from '@/utils/deviceId'
import { stableStringify, type EntryMeta, type Tombstone } from '@/utils/syncMerge'
import { emitConfigChanged } from '@/utils/syncSignals'
import { notifyHostsChanged, onHostsChanged } from '@/utils/crossWindow'

export type { EntryMeta, Tombstone }

export interface Namespace {
  id: string
  name: string
  description?: string
}

export interface HostItem {
  id: string
  name: string
  addr: string
  namespaceId: string
  parentId: string | null
  type: 'host' | 'folder'
  sortOrder: number
  /** 连接状态 */
  connected?: boolean
  /** 主机配置 */
  config?: {
    username?: string
    port?: number
    authType?: 'password' | 'key'
    password?: string
    /** 旧数据/手动指定的私钥路径；已有 keyContent 时优先用托管内容（删掉原文件也能连） */
    keyPath?: string
    /** 导入时托管的私钥内容（PEM / OpenSSH 原文），随会话保存 */
    keyContent?: string
    /** 私钥原始文件名，仅用于界面展示 */
    keyName?: string
    /** 私钥口令（加密私钥必填，可为空） */
    passphrase?: string
  }
}

/** 落盘的会话列表文档(v3) */
export interface HostDoc {
  version: number
  namespaces: Namespace[]
  currentNamespaceId: string
  hosts: HostItem[]
  /** key = 'host:{id}' | 'ns:{id}' */
  meta: Record<string, EntryMeta>
  tombstones: Tombstone[]
}

/** 条目键前缀：主机与命名空间在同步时共用一套合并规则，故放进同一键空间 */
export const EK_HOST = 'host:'
export const EK_NS = 'ns:'

/** 墓碑保留时长：超过后清理。太短会让"离线期间的删除"重新出现在别的设备上 */
export const TOMBSTONE_TTL_MS = 30 * 24 * 3600 * 1000

/** 会话列表落盘键前缀: 按账号隔离, 完整键形如 host_manager:u123 / host_manager:anon */
const SETTING_KEY_PREFIX = 'host_manager'
/** 未登录时的归属域(离线数据不参与云同步, 只能被"导入本地离线配置"并入账号) */
export const ANON_OWNER = 'anon'
/** 旧版本(全局共用一份)的单键: 升级后由第一个加载的账号继承, 随后删除 */
const LEGACY_SETTING_KEY = 'host_manager'
/** 数据版本：v2 → v3 自动迁移(补 meta)；高于本版本的未知格式只展示不落盘 */
export const HOST_DATA_VERSION = 3

/** 目录（分组）最大层级：分组最多嵌套三层，第 3 层分组内只允许放会话 */
export const MAX_FOLDER_DEPTH = 3

const DEFAULT_NAMESPACES: Namespace[] = [
  { id: 'public', name: '公共 (Public)', description: '默认命名空间，所有用户共享' },
  { id: 'dev', name: '开发 (Dev)', description: '开发环境' },
  { id: 'prod', name: '生产 (Prod)', description: '生产环境' }
]

/** 默认命名空间逐个拷: 直接复用常量对象会让 renameNamespace 改到常量本身, 污染后续所有新建文档 */
function defaultNamespaces(): Namespace[] {
  return DEFAULT_NAMESPACES.map(n => ({ ...n }))
}

export const useHostStore = defineStore('host', () => {
  const namespaces = ref<Namespace[]>(defaultNamespaces())
  const currentNamespaceId = ref('public')
  const hosts = ref<HostItem[]>([])
  const activeHostId = ref<string | null>(null)
  const loaded = ref(false)

  // ===== 云同步元数据(不进 UI, 只随文档落盘) =====
  /** 条目变更戳: key = 'host:{id}' | 'ns:{id}' */
  let entryMeta: Record<string, EntryMeta> = {}
  /** 删除墓碑 */
  let tombstones: Tombstone[] = []
  /** 上次落盘时各条目的内容摘要: 与当前内容差异 = 本次需要打戳/记墓碑的条目 */
  let lastDigest: Record<string, string> = {}

  /** 当前命名空间下的主机和文件夹树 */
  const treeData = computed(() => {
    const list = hosts.value.filter(h => h.namespaceId === currentNamespaceId.value)
    return buildTree(list)
  })

  /** 构建树结构 */
  function buildTree(list: HostItem[]): HostItem[] {
    const roots: HostItem[] = []
    const map = new Map<string, HostItem & { children?: HostItem[] }>()

    list.forEach(item => {
      map.set(item.id, { ...item, children: [] })
    })

    map.forEach(node => {
      if (!node.parentId || !map.has(node.parentId)) {
        roots.push(node)
      } else {
        const parent = map.get(node.parentId)
        if (parent) {
          parent.children = parent.children || []
          parent.children.push(node)
        }
      }
    })

    // 排序：按 sortOrder
    const sortTree = (nodes: (HostItem & { children?: HostItem[] })[]) => {
      nodes.sort((a, b) => a.sortOrder - b.sortOrder)
      nodes.forEach(n => { if (n.children) sortTree(n.children) })
    }
    sortTree(roots as any)

    return roots as HostItem[]
  }

  /** 当前命名空间下的扁平列表 */
  const currentNamespaceHosts = computed(() =>
    hosts.value.filter(h => h.namespaceId === currentNamespaceId.value)
  )

  // ===== 命名空间操作 =====

  function selectNamespace(id: string) {
    currentNamespaceId.value = id
    persist()
  }

  function addNamespace(name: string, description?: string) {
    const id = 'ns_' + Date.now() + '_' + Math.random().toString(36).slice(2, 6)
    namespaces.value.push({ id, name, description })
    currentNamespaceId.value = id
    persist()
    return id
  }

  function renameNamespace(id: string, name: string, description?: string) {
    const ns = namespaces.value.find(n => n.id === id)
    if (ns) {
      ns.name = name
      if (description !== undefined) ns.description = description
      persist()
    }
  }

  function deleteNamespace(id: string) {
    if (namespaces.value.length <= 1) {
      throw new Error('至少保留一个命名空间')
    }
    // 删除该命名空间下的所有主机和文件夹
    hosts.value = hosts.value.filter(h => h.namespaceId !== id)
    namespaces.value = namespaces.value.filter(n => n.id !== id)
    if (currentNamespaceId.value === id) {
      currentNamespaceId.value = namespaces.value[0].id
    }
    persist()
  }

  // ===== 主机/文件夹操作 =====

  function addHost(data: Omit<HostItem, 'id' | 'sortOrder'>) {
    const maxOrder = hosts.value
      .filter(h => h.namespaceId === data.namespaceId && h.parentId === data.parentId)
      .reduce((max, h) => Math.max(max, h.sortOrder), -1)
    const id = 'host_' + Date.now() + '_' + Math.random().toString(36).slice(2, 6)
    const item: HostItem = {
      ...data,
      id,
      sortOrder: maxOrder + 1
    }
    hosts.value.push(item)
    persist()
    return item
  }

  // ===== 目录层级约束（最多三层）=====
  // 口径：根下的分组为 1 级，分组内的会话与所在分组合计同一层级（会话不占层级），
  // 因此「第 3 层分组里可以放会话，但不能继续建分组 / 移入分组」。

  /**
   * 节点的分组层级：沿 parentId 上溯统计分组数量（自身是分组则计入）。
   * 传 null（顶级目录）返回 0。
   */
  function nodeLevel(id: string | null): number {
    let level = 0
    let cur = id ? hosts.value.find(h => h.id === id) : undefined
    let guard = 0
    while (cur && guard++ < 64) {
      if (cur.type === 'folder') level++
      const parentId = cur.parentId
      cur = parentId ? hosts.value.find(h => h.id === parentId) : undefined
    }
    return level
  }

  /** 分组的子树高度：叶分组为 1，含一层子分组为 2……会话恒为 1（不占层级） */
  function folderHeight(id: string): number {
    const node = hosts.value.find(h => h.id === id)
    if (!node || node.type !== 'folder') return 1
    let best = 1
    for (const child of hosts.value) {
      if (child.parentId !== id || child.type !== 'folder') continue
      best = Math.max(best, 1 + folderHeight(child.id))
    }
    return best
  }

  /** 该分组内是否还能再建子分组（第 3 层分组返回 false） */
  function canContainFolder(parentId: string | null): boolean {
    return nodeLevel(parentId) + 1 <= MAX_FOLDER_DEPTH
  }

  /** 把 nodeId 移到 targetParentId 下后是否仍不超三层（分组的整棵子树一起算） */
  function canMoveUnder(nodeId: string, targetParentId: string | null): boolean {
    const node = hosts.value.find(h => h.id === nodeId)
    if (!node || node.type !== 'folder') return true
    return nodeLevel(targetParentId) + folderHeight(nodeId) <= MAX_FOLDER_DEPTH
  }

  function addFolder(name: string, parentId: string | null = null, namespaceId?: string) {
    // 层级上限：第 3 层分组内不能再建分组；调用方按 null 返回值给出提示
    if (!canContainFolder(parentId)) return null
    const nsId = namespaceId || currentNamespaceId.value
    const maxOrder = hosts.value
      .filter(h => h.namespaceId === nsId && h.parentId === parentId)
      .reduce((max, h) => Math.max(max, h.sortOrder), -1)
    const id = 'folder_' + Date.now() + '_' + Math.random().toString(36).slice(2, 6)
    const folder: HostItem = {
      id,
      name,
      addr: '',
      namespaceId: nsId,
      parentId,
      type: 'folder',
      sortOrder: maxOrder + 1
    }
    hosts.value.push(folder)
    persist()
    return folder
  }

  /**
   * 批量导入会话（跨软件导入用）。
   * 逐条调用 addHost 会触发 N 次 persist，多次异步落盘相互覆盖导致丢数据，
   * 故这里一次性入列后只落盘一次。
   */
  function addHostsBulk(items: Array<Omit<HostItem, 'id' | 'sortOrder'> & { id?: string }>): HostItem[] {
    const created: HostItem[] = []
    for (const data of items) {
      const maxOrder = hosts.value
        .filter(h => h.namespaceId === data.namespaceId && h.parentId === data.parentId)
        .reduce((max, h) => Math.max(max, h.sortOrder), -1)
      // 允许调用方自带 id：批量导入时需要把新文件夹的 id 直接挂到会话的 parentId 上
      const id = data.id || 'host_' + Date.now() + '_' + Math.random().toString(36).slice(2, 6)
      const item: HostItem = { ...data, id, sortOrder: maxOrder + 1 }
      hosts.value.push(item)
      created.push(item)
    }
    if (created.length > 0) persist()
    return created
  }

  function updateHost(id: string, updates: Partial<HostItem>) {
    const item = hosts.value.find(h => h.id === id)
    if (item) {
      Object.assign(item, updates)
      persist()
    }
  }

  function deleteHost(id: string) {
    // 同时删除子项
    const toDelete = new Set<string>([id])
    let changed = true
    while (changed) {
      changed = false
      hosts.value.forEach(h => {
        if (h.parentId && toDelete.has(h.parentId) && !toDelete.has(h.id)) {
          toDelete.add(h.id)
          changed = true
        }
      })
    }
    hosts.value = hosts.value.filter(h => !toDelete.has(h.id))
    if (activeHostId.value && toDelete.has(activeHostId.value)) {
      activeHostId.value = null
    }
    persist()
  }

  /** 拖拽移动节点（调整顺序或父子关系） */
  function moveNode(nodeId: string, targetParentId: string | null, targetIndex: number) {
    const node = hosts.value.find(h => h.id === nodeId)
    if (!node) return

    // 检查：文件夹的父级必须是文件夹或 null
    if (node.type === 'folder' && targetParentId) {
      const parent = hosts.value.find(h => h.id === targetParentId)
      if (!parent || parent.type !== 'folder') {
        throw new Error('文件夹的父级必须是文件夹')
      }
    }

    // 检查：不能将节点移动到自己的子节点下
    if (targetParentId && isDescendant(nodeId, targetParentId)) {
      throw new Error('不能将节点移动到其自身的子节点下')
    }

    // 检查：移动后不得超过目录层级上限（第 3 层分组内不能再放分组）
    if (!canMoveUnder(nodeId, targetParentId)) {
      throw new Error(`目录层级最多 ${MAX_FOLDER_DEPTH} 层`)
    }

    node.parentId = targetParentId
    node.namespaceId = currentNamespaceId.value

    // 重新排序：获取同级所有节点并更新 sortOrder
    const siblings = hosts.value
      .filter(h => h.namespaceId === node.namespaceId && h.parentId === targetParentId && h.id !== nodeId)
      .sort((a, b) => a.sortOrder - b.sortOrder)

    siblings.splice(targetIndex, 0, node)
    siblings.forEach((s, i) => { s.sortOrder = i })

    persist()
  }

  /** 检查 descendantId 是否为 ancestorId 的子节点 */
  function isDescendant(ancestorId: string, descendantId: string): boolean {
    const desc = hosts.value.find(h => h.id === descendantId)
    if (!desc || !desc.parentId) return false
    if (desc.parentId === ancestorId) return true
    return isDescendant(ancestorId, desc.parentId)
  }

  function setActiveHost(id: string | null) {
    activeHostId.value = id
  }

  // ===== 账号作用域(会话列表按登录账号隔离) =====

  /** 数据归属域: 登录用户 = u{userId}, 未登录 = anon */
  function currentOwner(): string {
    try {
      const uid = useAuthStore().user?.id
      return uid ? `u${uid}` : ANON_OWNER
    } catch {
      // pinia 尚未就绪的极早期调用按匿名域处理
      return ANON_OWNER
    }
  }

  /** 拼落盘键 */
  function scopeKey(owner: string): string {
    return `${SETTING_KEY_PREFIX}:${owner}`
  }

  /** 最近一次 load() 使用的落盘键; null = 还没加载过, 按当前账号即时推导 */
  let loadedKey: string | null = null

  /**
   * 绑定账号切换: 登录/登出/换号后丢弃内存数据, 重新加载新账号自己的会话列表。
   * 由各窗口在 auth.load() 之后调用一次(重复调用无副作用)。
   */
  let stopScopeWatch: (() => void) | null = null
  function bindAccountScope(): void {
    if (stopScopeWatch) return
    const auth = useAuthStore()
    stopScopeWatch = watch(
      () => (auth.user?.id ? `u${auth.user.id}` : ANON_OWNER),
      () => {
        loaded.value = false
        void load()
      }
    )
  }

  /** 旧版全局单键迁移: 复制到当前账号键后删除旧键, 其余账号从空列表开始 */
  async function adoptLegacyData(targetKey: string): Promise<string | null> {
    try {
      const legacy = await invoke<string | null>('get_setting', { key: LEGACY_SETTING_KEY })
      if (!legacy) return null
      await invoke('set_setting', { key: targetKey, value: legacy })
      await invoke('delete_setting', { key: LEGACY_SETTING_KEY })
      console.info('[host] 会话列表已迁移到账号作用域:', targetKey)
      return legacy
    } catch (e) {
      console.warn('[host] 旧数据迁移失败:', e)
      return null
    }
  }

  // ===== 持久化 =====

  /** 运行时字段不参与落盘与差异比较: connected 是连接状态, 它变了不代表配置变了 */
  function persistableHost(h: HostItem): HostItem {
    if (h.connected === undefined) return h
    const { connected, ...rest } = h
    return rest as HostItem
  }

  /** 当前内存内容 -> 条目键到摘要的映射 */
  function entryDigests(): Record<string, string> {
    const map: Record<string, string> = {}
    for (const h of hosts.value) map[EK_HOST + h.id] = stableStringify(persistableHost(h))
    for (const n of namespaces.value) map[EK_NS + n.id] = stableStringify(n)
    return map
  }

  /**
   * 与上次落盘内容做差异, 自动维护变更戳与墓碑。
   * 放在 persist 链路而不是每个 add/update/delete 里: 业务改动点很多(含拖拽、重命名),
   * 漏一处就是永久丢失一次跨设备变更。
   */
  function stampChanges(): void {
    const now = Date.now()
    const current = entryDigests()
    for (const [key, sum] of Object.entries(current)) {
      if (lastDigest[key] === sum) continue
      entryMeta[key] = { updatedAt: now, deviceId: deviceId() }
      // 同 id 重新出现 = 撤销删除, 墓碑不清掉会在合并时又被判为已删
      tombstones = tombstones.filter(t => t.id !== key)
    }
    for (const key of Object.keys(lastDigest)) {
      if (key in current) continue
      if (!tombstones.some(t => t.id === key)) {
        tombstones.push({ id: key, deletedAt: now, deviceId: deviceId() })
      }
    }
    lastDigest = current
  }

  /** 墓碑按 TTL 清理: 过期删除事件不再传播, 避免墓碑无限增长 */
  function pruneTombstones(): void {
    const floor = Date.now() - TOMBSTONE_TTL_MS
    tombstones = tombstones.filter(t => t && t.id && t.deletedAt > floor)
  }

  /** 生成当前状态的完整文档(云同步推送、导出、备份共用) */
  function snapshot(): HostDoc {
    stampChanges()
    pruneTombstones()
    return {
      version: HOST_DATA_VERSION,
      namespaces: namespaces.value.map(n => ({ ...n })),
      currentNamespaceId: currentNamespaceId.value,
      hosts: hosts.value.map(persistableHost),
      meta: { ...entryMeta },
      tombstones: tombstones.map(t => ({ ...t })),
    }
  }

  /**
   * 解析落盘 JSON -> 文档。
   * @returns upgraded=true 表示这是旧版本(v1/v2)升上来的, 调用方应回写一次把 meta 定下来;
   *          高于本版本的未知格式返回 null(只能展示空列表, 绝不回写覆盖用户数据)
   */
  function parseDoc(raw: string): { doc: HostDoc; upgraded: boolean } | null {
    let cfg: any
    try { cfg = JSON.parse(raw) } catch { return null }
    if (!cfg || typeof cfg !== 'object') return null
    const version = Number(cfg.version) || 1
    if (version > HOST_DATA_VERSION) return null
    const list: HostItem[] = Array.isArray(cfg.hosts) ? cfg.hosts : []
    const nsList: Namespace[] = Array.isArray(cfg.namespaces) && cfg.namespaces.length
      ? cfg.namespaces : DEFAULT_NAMESPACES.map(n => ({ ...n }))
    const srcMeta = cfg.meta && typeof cfg.meta === 'object' ? cfg.meta as Record<string, Partial<EntryMeta>> : {}
    const meta: Record<string, EntryMeta> = {}
    const fill = (key: string) => {
      const m = srcMeta[key]
      // v2 及更早没有 meta: 一律标成 updatedAt=0(=未知历史), 合并时让位于有真实时间的一方
      meta[key] = { updatedAt: Number(m?.updatedAt) || 0, deviceId: String(m?.deviceId ?? '') }
    }
    for (const h of list) fill(EK_HOST + h.id)
    for (const n of nsList) fill(EK_NS + n.id)
    const tombs: Tombstone[] = Array.isArray(cfg.tombstones)
      ? cfg.tombstones
        .filter((t: any) => t && typeof t.id === 'string')
        .map((t: any) => ({ id: String(t.id), deletedAt: Number(t.deletedAt) || 0, deviceId: String(t.deviceId ?? '') }))
      : []
    return {
      doc: {
        version: HOST_DATA_VERSION,
        namespaces: nsList,
        currentNamespaceId: cfg.currentNamespaceId || nsList[0].id,
        hosts: list,
        meta,
        tombstones: tombs,
      },
      upgraded: version !== HOST_DATA_VERSION,
    }
  }

  /** 把文档装进内存(不落盘): 条目戳沿用文档自带的, 不重新打戳 */
  function installDoc(doc: HostDoc): void {
    namespaces.value = doc.namespaces
    currentNamespaceId.value = doc.currentNamespaceId
    hosts.value = doc.hosts
    entryMeta = { ...doc.meta }
    tombstones = doc.tombstones.map(t => ({ ...t }))
    lastDigest = entryDigests()
  }

  /** 序列化 + 按本机「凭证加密」开关加密后落盘 */
  async function writeDoc(doc: HostDoc, key?: string): Promise<void> {
    if (!isTauriEnv()) return
    try {
      let value = JSON.stringify(doc)
      if (useSettingsStore().settings.encryptCredentials) {
        value = await encryptText(value)
      }
      await invoke('set_setting', { key: key ?? loadedKey ?? scopeKey(currentOwner()), value })
      // 通知同机其他窗口重读: 它的内存此刻已经是旧数据
      notifyHostsChanged()
    } catch (e) {
      console.warn('[host] persist failed:', e)
    }
  }

  /** 应用一份外部文档(云同步合并结果 / 离线导入): 装内存 + 落盘, 两步必须同一流向, 避免内存与磁盘不一致 */
  async function applyDoc(doc: HostDoc): Promise<void> {
    busyApplying = true
    try {
      installDoc(doc)
      await writeDoc({ ...doc, meta: { ...doc.meta }, hosts: hosts.value.map(persistableHost), namespaces: namespaces.value.map(n => ({ ...n })) })
    } finally {
      busyApplying = false
    }
  }

  /** 正在应用外部文档(云同步合并结果 / 离线导入): 期间不响应其他窗口的重读通知, 免得把自己的结果冲掉 */
  let busyApplying = false

  // 独立设置窗口与主窗口各有一套内存: 哪个窗口把会话列表写进了 SQLite, 另一个窗口都必须重读,
  // 否则它下一次 persist 会把对方刚写入的合并结果整盘盖掉(真正的丢数据)
  onHostsChanged(() => {
    if (busyApplying || !loaded.value) return
    void load()
  })

  /** 读某个归属域的原始文档(离线导入预览用), 不改动当前内存与落盘键 */
  async function peekDoc(owner: string): Promise<HostDoc | null> {
    if (!isTauriEnv()) return null
    try {
      const json = await invoke<string | null>('get_setting', { key: scopeKey(owner) })
      if (!json) return null
      let raw = json
      if (isEncrypted(json)) {
        try { raw = await decryptText(json) } catch { return null }
      }
      if (!raw) return null
      return parseDoc(raw)?.doc ?? null
    } catch (e) {
      console.warn('[host] peekDoc failed:', e)
      return null
    }
  }

  /**
   * 读数据前先确保登录态已从后端读回。
   * 面板组件比窗口 onMounted 更早挂载, 不等待就会把已登录用户的数据当成匿名域(旧数据也会被错迁移)。
   */
  async function ensureOwnerResolved(): Promise<void> {
    try {
      const auth = useAuthStore()
      if (!auth.resolved) await auth.load()
    } catch { /* 浏览器开发模式: 按匿名域处理 */ }
  }

  async function load(): Promise<void> {
    if (!isTauriEnv()) {
      // 非 Tauri 环境（浏览器开发模式）使用 mock 数据演示 UI
      installDoc(emptyDoc(getMockHosts()))
      loaded.value = true
      return
    }
    await ensureOwnerResolved()
    const key = scopeKey(currentOwner())
    // 先记下归属键: 账号切换后即使还没读完, 期间的写操作也应落到新账号而不是旧账号
    loadedKey = key
    try {
      let json = await invoke<string | null>('get_setting', { key })
      if (!json) {
        // 首次升级到按账号隔离: 继承旧的全局单键(仅第一个账号能拿到历史数据)
        json = await adoptLegacyData(key)
      }
      if (!json) {
        // 该账号本机首次使用: 只装空列表, 不预填演示主机, 也不写盘(写盘会生成一个无内容的云端文档占位)
        installDoc(emptyDoc([]))
        loaded.value = true
        return
      }
      // 凭证加密开启时落盘为密文，读取时先解密（解密失败视为无数据）
      let raw = json
      if (isEncrypted(json)) {
        try { raw = await decryptText(json) } catch { raw = '' }
      }
      const parsed = raw ? parseDoc(raw) : null
      if (!parsed) {
        // 解不开 / 版本比本程序还新 / JSON 损坏: 只展示留空, 绝不 persist —— 写回会用空数据覆盖用户已存的会话
        console.warn('[host] 会话数据不可用(版本过新或解析失败)，不覆盖落盘数据')
        installDoc(emptyDoc([]))
        loaded.value = true
        return
      }
      installDoc(parsed.doc)
      // 旧版本升到 v3: 补完 meta 立刻回写一次, 之后的差异比较才有基线
      if (parsed.upgraded) {
        console.info('[host] 会话数据已升级到 v3 (补条目变更戳)')
        await writeDoc(snapshot())
      }
    } catch (e) {
      console.warn('[host] load failed:', e)
      installDoc(emptyDoc([]))
    } finally {
      loaded.value = true
    }
  }

  /**
   * 持久化到 SQLite —— 直写不做防抖。
   * 原因：用户操作后可能立刻卸载/强退，setTimeout 防抖窗口内的 set_setting 调用会丢失，
   * 导致重装后数据没了。Host 数据量小（KB 级），单次写 < 1ms，性能可接受。
   */
  /**
   * 落盘串行化。
   * writeDoc 内部有 await（加密/IPC），并发调用时各请求完成顺序可能与发起顺序相反，
   * 于是"较早发起但较晚完成"的那次会用旧快照覆盖新快照（批量导入时必现丢数据）。
   * 这里把写盘串到一条队列上，并且快照在实际执行时才取，保证最终落盘的一定是最新状态。
   */
  let persistQueue: Promise<unknown> = Promise.resolve()
  async function persist(): Promise<void> {
    if (!isTauriEnv()) return
    const task = persistQueue
      .then(() => writeDoc(snapshot()))
      .catch((e) => console.warn('[host] persist failed:', e))
    persistQueue = task
    emitConfigChanged('hosts')
    return task as Promise<void>
  }

  function reset(): void {
    namespaces.value = DEFAULT_NAMESPACES.map(n => ({ ...n }))
    currentNamespaceId.value = 'public'
    hosts.value = getMockHosts()
    activeHostId.value = null
    persist()
  }

  /** 重置主机列表为 mock 数据 */
  function resetHosts(): void {
    hosts.value = getMockHosts()
    activeHostId.value = null
    persist()
  }

  /** 当前数据实际归属的落盘键(含尚未 load 完的中间态) */
  function ownerKey(): string {
    return loadedKey ?? scopeKey(currentOwner())
  }

  return {
    namespaces, currentNamespaceId, hosts, treeData,
    activeHostId, currentNamespaceHosts, loaded,
    selectNamespace, addNamespace, renameNamespace, deleteNamespace,
    addHost, addHostsBulk, addFolder, updateHost, deleteHost, moveNode, setActiveHost,
    // 目录层级约束（最多三层）相关判定，UI 用来提前拦截与提示
    nodeLevel, canContainFolder, canMoveUnder,
    snapshot, load, persist, reset, resetHosts, bindAccountScope,
    // 云同步与离线导入需要跨归属域读写
    ownerKey, peekDoc, applyDoc
  }
})

/** 空文档: 带默认命名空间, 条目时间戳全为 0(=无历史), 供首次使用与浏览器演示模式复用 */
function emptyDoc(list: HostItem[] = []): HostDoc {
  const ns = DEFAULT_NAMESPACES.map(n => ({ ...n }))
  const meta: Record<string, EntryMeta> = {}
  for (const h of list) meta[EK_HOST + h.id] = { updatedAt: 0, deviceId: '' }
  for (const n of ns) meta[EK_NS + n.id] = { updatedAt: 0, deviceId: '' }
  return {
    version: HOST_DATA_VERSION,
    namespaces: ns,
    currentNamespaceId: ns[0].id,
    hosts: list,
    meta,
    tombstones: [],
  }
}

/** Mock 数据 - 测试阶段统一使用示例地址 192.168.1.10（请勿填入真实内网地址） */
function getMockHosts(): HostItem[] {
  return [
    // 公共命名空间
    { id: 'folder_1', name: 'Web 服务器', addr: '', namespaceId: 'public', parentId: null, type: 'folder', sortOrder: 0 },
    { id: 'host_1', name: 'web-01', addr: 'ubuntu@192.168.1.10:22', namespaceId: 'public', parentId: 'folder_1', type: 'host', sortOrder: 0, config: { username: 'ubuntu', port: 22, authType: 'password' } },
    { id: 'host_2', name: 'web-02', addr: 'root@192.168.1.10:22', namespaceId: 'public', parentId: 'folder_1', type: 'host', sortOrder: 1 },
    { id: 'folder_2', name: '数据库', addr: '', namespaceId: 'public', parentId: null, type: 'folder', sortOrder: 1 },
    { id: 'host_3', name: 'db-01', addr: 'root@192.168.1.10:22', namespaceId: 'public', parentId: 'folder_2', type: 'host', sortOrder: 0 },
    { id: 'host_4', name: 'cache-01', addr: 'root@192.168.1.10:22', namespaceId: 'public', parentId: null, type: 'host', sortOrder: 2 },
    // 开发命名空间
    { id: 'host_5', name: 'dev-app', addr: 'root@192.168.1.10:22', namespaceId: 'dev', parentId: null, type: 'host', sortOrder: 0 },
    { id: 'host_6', name: 'dev-db', addr: 'root@192.168.1.10:22', namespaceId: 'dev', parentId: null, type: 'host', sortOrder: 1 },
    // 生产命名空间
    { id: 'folder_3', name: '核心服务', addr: '', namespaceId: 'prod', parentId: null, type: 'folder', sortOrder: 0 },
    { id: 'host_7', name: 'prod-api', addr: 'root@192.168.1.10:22', namespaceId: 'prod', parentId: 'folder_3', type: 'host', sortOrder: 0 },
    { id: 'host_8', name: 'prod-db', addr: 'root@192.168.1.10:22', namespaceId: 'prod', parentId: 'folder_3', type: 'host', sortOrder: 1 }
  ]
}
