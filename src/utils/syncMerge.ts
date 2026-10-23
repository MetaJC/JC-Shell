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
 * 配置云同步的合并算法（纯函数，不做任何 IO）
 * 作者：JC
 *
 * 为什么单独成文件：合并一旦写错就是"用户精心整理的会话列表被抹掉"，必须在没有网络、
 * 没有 pinia、没有 SQLite 的环境下可重复推演。这里全部是输入 -> 输出的纯计算。
 *
 * 三条收敛性底线（多端同步最容易翻车的地方）：
 * 1. 定序必须与"我是哪一端"无关：同一对差异，两台设备算出的胜者要一样，否则会各收敛出一半；
 * 2. 合并要幂等：同一批文档重放 N 次结果一致；
 * 3. 绝不做整盘覆盖：条目取并集，删除靠墓碑传播，判不出胜负就保留双方并标记冲突。
 */
/** 条目变更戳；updatedAt=0 表示"历史数据，无时间记录"，合并时让位于有真实时间的一方 */
export interface EntryMeta {
  updatedAt: number
  deviceId: string
}

/** 删除墓碑：没有它，跨设备合并会把已删条目"复活" */
export interface Tombstone {
  /** 形如 host:xxx / ns:xxx，由调用方决定键空间 */
  id: string
  deletedAt: number
  deviceId: string
}

/**
 * 时钟容差：客户端时钟可能因休眠/未同步漂移几分钟，差值小于该值视为"同时修改"，
 * 改由 deviceId 定序。没有这一步，两台机器会因 1ms 抖动给出相反的胜者判定。
 */
export const CLOCK_TOLERANCE_MS = 2000

/** 一个条目键空间：key = 形如 host:xxx / ns:xxx，value = 条目内容（本模块不关心结构） */
export interface Side {
  entries: Record<string, unknown>
  meta: Record<string, EntryMeta>
  tombstones: Tombstone[]
}

export interface MergeStats {
  /** 本次合并新增的条目数 */
  added: number
  /** 采纳了对端修改的条目数 */
  updated: number
  /** 采纳了对端删除的条目数 */
  removed: number
  /** 双方都改过、按 LWW 裁决的条目数（需要提示用户核对） */
  conflicts: number
}

export interface MergeResult {
  entries: Record<string, unknown>
  meta: Record<string, EntryMeta>
  tombstones: Tombstone[]
  stats: MergeStats
}

/** 稳定序列化：键顺序无关，否则内容相同也会被判定为"改过"，导致每次同步全量打戳 */
export function stableStringify(value: unknown): string {
  return JSON.stringify(value, (_k, v) => {
    if (v && typeof v === 'object' && !Array.isArray(v)) {
      const obj = v as Record<string, unknown>
      const sorted: Record<string, unknown> = {}
      for (const key of Object.keys(obj).sort()) sorted[key] = obj[key]
      return sorted
    }
    return v
  })
}

/** 本地判等用的快速摘要(FNV-1a)：只用于"内容有没有变"，不承担安全含义，所以不必用 SHA */
export function localDigest(value: unknown): string {
  const s = stableStringify(value)
  let h = 0x811c9dc5
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i)
    h = Math.imul(h, 0x01000193) >>> 0
  }
  return h.toString(16).padStart(8, '0') + ':' + s.length
}

/** 空 Side（首次同步/尚无共同祖先时用它代替 base） */
export function emptySide(): Side {
  return { entries: {}, meta: {}, tombstones: [] }
}

/**
 * 两个变更戳谁更新。
 * @returns 'a' | 'b' | 'same'；时间差在容差内时比 deviceId 字典序 —— 两端拿到的都是同一对
 *          (本地戳, 对端戳)，按固定规则裁决，因此结论必然一致。
 */
export function newerMeta(a?: EntryMeta, b?: EntryMeta): 'a' | 'b' | 'same' {
  const ta = a?.updatedAt ?? 0
  const tb = b?.updatedAt ?? 0
  if (ta === 0 && tb === 0) return 'same'
  if (Math.abs(ta - tb) > CLOCK_TOLERANCE_MS) return ta > tb ? 'a' : 'b'
  const da = a?.deviceId ?? ''
  const db = b?.deviceId ?? ''
  if (da === db) return ta >= tb ? 'a' : 'b'
  return da > db ? 'a' : 'b'
}

/** 墓碑按 TTL 清理并合并：同一键多次删除只保留最早那次时间，保证"删除"这件事在全网有唯一时刻 */
export function mergeTombstones(lists: Tombstone[][], ttlMs: number): Map<string, Tombstone> {
  const floor = Date.now() - ttlMs
  const out = new Map<string, Tombstone>()
  for (const list of lists) {
    for (const t of list || []) {
      if (!t || typeof t.id !== 'string' || t.deletedAt <= floor) continue
      const prev = out.get(t.id)
      if (!prev || t.deletedAt < prev.deletedAt) out.set(t.id, { ...t })
    }
  }
  return out
}

/** 条目内容是否只是"顺序变了"（数组顺序不影响语义时可用） */
function sameContent(a: unknown, b: unknown): boolean {
  if (a === b) return true
  if (a === undefined || b === undefined || a === null || b === null) return false
  return stableStringify(a) === stableStringify(b)
}

/**
 * 条目级三方合并（OR-Set 语义：增删都靠并集 + 墓碑传播）。
 * @param base 上次同步的共同祖先；null 表示没有共同祖先（首次同步、离线导入），
 *             此时"两边都有且内容不同"按 LWW 裁决，其余一律取并集，不会丢条目
 * @param fingerprint 可选：判断"两端各自新建的其实是同一个东西"，返回相同指纹即视为重复
 */
export function mergeSides(
  base: Side | null,
  local: Side,
  remote: Side,
  opts: { tombstoneTtlMs: number; fingerprint?: (val: unknown) => string | null },
): MergeResult {
  const stats: MergeStats = { added: 0, updated: 0, removed: 0, conflicts: 0 }
  const entries: Record<string, unknown> = {}
  const meta: Record<string, EntryMeta> = {}
  const tombstones = mergeTombstones(
    [local.tombstones || [], remote.tombstones || [], base?.tombstones || []],
    opts.tombstoneTtlMs,
  )
  const baseEntries = base?.entries ?? {}

  const keys = new Set([
    ...Object.keys(baseEntries),
    ...Object.keys(local.entries),
    ...Object.keys(remote.entries),
  ])
  /** 本轮"两端各自新增"的键，稍后按指纹去重 */
  const bothAdded: string[] = []

  for (const key of keys) {
    const b = baseEntries[key]
    const l = local.entries[key]
    const r = remote.entries[key]
    const lm = local.meta[key]
    const rm = remote.meta[key]

    if (l === undefined && r === undefined) {
      // 两端都没有了：只要曾经存在过就留下墓碑，否则对端下次合并会把它当"新增"复活
      if (b !== undefined && !tombstones.has(key)) {
        tombstones.set(key, { id: key, deletedAt: Date.now(), deviceId: local.meta[key]?.deviceId ?? '' })
      }
      continue
    }

    if (l !== undefined && r === undefined) {
      if (b === undefined && !tombstones.has(key)) {
        // 本地独有（对端从未有过）→ 直接带上
        entries[key] = l
        meta[key] = lm ?? { updatedAt: 0, deviceId: '' }
        stats.added++
        continue
      }
      if (b !== undefined) {
        // 对端删了它：本地是否在删除之后又改过？改过则复活，否则接受删除
        const tomb = tombstones.get(key)
        if (tomb && (lm?.updatedAt ?? 0) > tomb.deletedAt) {
          entries[key] = l
          meta[key] = lm ?? { updatedAt: 0, deviceId: '' }
          tombstones.delete(key)
          stats.conflicts++
        } else {
          stats.removed++
        }
        continue
      }
      // 在 base 里不存在但有墓碑 → 本地重建了同名条目：按新增处理
      entries[key] = l
      meta[key] = lm ?? { updatedAt: 0, deviceId: '' }
      tombstones.delete(key)
      stats.added++
      continue
    }

    if (l === undefined && r !== undefined) {
      if (b === undefined && !tombstones.has(key)) {
        entries[key] = r
        meta[key] = rm ?? { updatedAt: 0, deviceId: '' }
        stats.added++
        continue
      }
      if (b !== undefined) {
        const tomb = tombstones.get(key)
        if (tomb && (rm?.updatedAt ?? 0) > tomb.deletedAt) {
          entries[key] = r
          meta[key] = rm ?? { updatedAt: 0, deviceId: '' }
          tombstones.delete(key)
          stats.conflicts++
        } else {
          stats.removed++
        }
        continue
      }
      entries[key] = r
      meta[key] = rm ?? { updatedAt: 0, deviceId: '' }
      tombstones.delete(key)
      stats.added++
      continue
    }

    // 两端都在
    if (b === undefined) bothAdded.push(key)
    if (sameContent(l, r)) {
      entries[key] = l
      // 内容一致也要把戳推进到较新的那个，否则下次比较还会重复判冲突
      meta[key] = pickMeta(lm, rm) ?? { updatedAt: 0, deviceId: '' }
      continue
    }
    if (b !== undefined && sameContent(l, b)) {
      entries[key] = r
      meta[key] = rm ?? { updatedAt: 0, deviceId: '' }
      stats.updated++
      continue
    }
    if (b !== undefined && sameContent(r, b)) {
      entries[key] = l
      meta[key] = lm ?? { updatedAt: 0, deviceId: '' }
      continue
    }
    // 双方都改过：LWW 裁决（无共同祖先时也走这里）
    stats.conflicts++
    const winner = newerMeta(lm, rm)
    if (winner === 'a') {
      entries[key] = l
      meta[key] = lm ?? { updatedAt: 0, deviceId: '' }
    } else {
      entries[key] = r
      meta[key] = rm ?? { updatedAt: 0, deviceId: '' }
    }
  }

  // 两端各自新建了"同一个东西"（id 不同但指纹相同）：条目一个都不丢，输的那条改名让用户自己取舍。
  // 定序规则必须只看内容(id 字典序)，不能看"我是哪端"，否则两台机器会各自重命名不同的一条。
  if (opts.fingerprint) {
    const byFp = new Map<string, string[]>()
    for (const key of bothAdded) {
      const fp = opts.fingerprint(entries[key])
      if (!fp) continue
      const arr = byFp.get(fp) ?? []
      arr.push(key)
      byFp.set(fp, arr)
    }
    for (const group of byFp.values()) {
      if (group.length < 2) continue
      const sorted = [...group].sort()
      for (const key of sorted.slice(1)) {
        const val = entries[key] as Record<string, unknown> | undefined
        if (val && typeof val === 'object' && typeof val.name === 'string' && !val.name.endsWith(' (冲突副本)')) {
          entries[key] = { ...val, name: val.name + ' (冲突副本)' }
          meta[key] = { ...(meta[key] ?? { updatedAt: 0, deviceId: '' }) }
        }
        stats.conflicts++
      }
    }
  }

  return { entries, meta, tombstones: [...tombstones.values()], stats }
}

/** 取较新的戳（内容已一致时用，避免同一差异被反复判成冲突） */
function pickMeta(a?: EntryMeta, b?: EntryMeta): EntryMeta | undefined {
  if (!a) return b
  if (!b) return a
  return newerMeta(a, b) === 'a' ? a : b
}

/**
 * 扁平文档（偏好、界面布局）的字段级合并。
 * <p>与条目级合并的区别：这类数据是"一组互不相干的开关"，一个字段变了不该连带别字段，
 * 所以按字段各判各的；skipKeys 里的设备级字段整体排除在比较之外（由调用方保留本机值）。</p>
 */
export function mergeFlat(
  base: Record<string, unknown> | null,
  local: Record<string, unknown>,
  remote: Record<string, unknown>,
  localStamp: EntryMeta,
  remoteStamp: EntryMeta,
  skipKeys: string[],
): { values: Record<string, unknown>; stats: MergeStats } {
  const skip = new Set(skipKeys)
  const stats: MergeStats = { added: 0, updated: 0, removed: 0, conflicts: 0 }
  const values: Record<string, unknown> = {}
  const keys = new Set([...Object.keys(local), ...Object.keys(remote)])
  for (const key of keys) {
    if (skip.has(key)) continue
    const l = local[key]
    const r = remote[key]
    if (l === undefined) {
      // 本地没有该字段(老版本配置) → 用云端补齐
      values[key] = r
      continue
    }
    if (r === undefined) continue
    if (sameContent(l, r)) {
      values[key] = l
      continue
    }
    const b = base?.[key]
    if (b !== undefined && sameContent(l, b)) {
      values[key] = r
      stats.updated++
      continue
    }
    if (b !== undefined && sameContent(r, b)) {
      values[key] = l
      continue
    }
    stats.conflicts++
    values[key] = newerMeta(localStamp, remoteStamp) === 'a' ? l : r
  }
  return { values, stats }
}

/**
 * 用来源文档覆盖目标文档（用户在离线导入时显式选"直接覆盖"）。
 * <p>只有一条规则要守：这是用户主动选择，所以来源的条目与墓碑原样过去；
 * 但目标独有的条目要转成墓碑带过去，否则覆盖后它们会在下一次合并时又冒出来。</p>
 */
export function overwriteSide(target: Side, source: Side, tombstoneTtlMs: number): MergeResult {
  const stats: MergeStats = { added: 0, updated: 0, removed: 0, conflicts: 0 }
  const tombstones = mergeTombstones([source.tombstones || []], tombstoneTtlMs)
  const entries: Record<string, unknown> = {}
  const meta: Record<string, EntryMeta> = {}
  const now = Date.now()
  for (const [key, val] of Object.entries(source.entries)) {
    entries[key] = val
    meta[key] = source.meta[key] ?? { updatedAt: 0, deviceId: '' }
    tombstones.delete(key)
    if (target.entries[key] === undefined) stats.added++
    else if (!sameContent(target.entries[key], val)) stats.updated++
  }
  for (const key of Object.keys(target.entries)) {
    if (source.entries[key] === undefined) {
      tombstones.set(key, { id: key, deletedAt: now, deviceId: source.meta[key]?.deviceId ?? '' })
      stats.removed++
    }
  }
  return { entries, meta, tombstones: [...tombstones.values()], stats }
}
