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
 * 跨窗口偏好同步
 * 作者：JC
 * 主窗口与设置窗口是独立的 WebView（各自独立 Pinia 实例），
 * localStorage 只在窗口启动时读取一次，无法天然联动。
 * 这里通过 Tauri 全局事件广播偏好变更，各窗口收到后重读存储并应用到自身，
 * 保证"设置生效范围是整个程序"。
 */
import { emit, listen } from '@tauri-apps/api/event'
import { isTauriEnv } from '@/api'
import type { ConflictInfo, ConflictStrategy } from '@/stores/sync'

/** 偏好变更事件（payload：变更的存储键列表） */
const PREFS_EVENT = 'jc:app-prefs-changed'

/** 广播偏好变更（本窗口自行应用后调用，通知其他窗口跟进） */
export function notifyPrefsChanged(keys: string[]): void {
  if (!isTauriEnv()) return
  emit(PREFS_EVENT, keys).catch(() => {})
}

/** 订阅其他窗口的偏好变更（重复订阅由调用方自行避免） */
export function onPrefsChanged(cb: (keys: string[]) => void): void {
  if (!isTauriEnv()) return
  listen<string[]>(PREFS_EVENT, (e) => cb(e.payload ?? [])).catch(() => {})
}

/**
 * 会话列表(主机配置)变更事件
 * 与偏好不同, 会话列表存在 SQLite: 独立设置窗口里的导入/云同步合并会直接改盘,
 * 主窗口内存中的旧列表若不重读, 下一次 persist 就会把对方刚写入的结果整盘盖掉。
 */
const HOSTS_EVENT = 'jc:app-hosts-changed'

/** 本窗口实例标识: 用来过滤自己广播出去的回声(重读自己刚写的内容没有意义, 还可能打断进行中的合并) */
const WINDOW_INSTANCE = Math.random().toString(36).slice(2)

/** 广播"会话列表已被改写" */
export function notifyHostsChanged(): void {
  if (!isTauriEnv()) return
  emit(HOSTS_EVENT, { inst: WINDOW_INSTANCE }).catch(() => {})
}

/** 订阅其他窗口对会话列表的改写（重复订阅由调用方自行避免） */
export function onHostsChanged(cb: () => void): void {
  if (!isTauriEnv()) return
  listen<{ inst: string }>(HOSTS_EVENT, (e) => {
    if (e.payload?.inst === WINDOW_INSTANCE) return
    cb()
  }).catch(() => {})
}

/**
 * 云同步引擎协调（单引擎架构）。
 * 多 WebView 各自跑一份同步会互相踩基线、让云端 rev 乒乓累加、冲突弹窗刷屏，
 * 因此收敛为单引擎：主窗口=唯一执行者(leader)，设置窗口=follower，
 * follower 的所有同步动作都通过事件交给 leader 执行，
 * leader 再把「冲突发生 / 已裁决」广播回 follower，让弹窗在任意窗口都能出现。
 */
const SYNC_COOR_EVENT = 'jc:app-sync-coor'

export type SyncCoorPayload =
  | { kind: 'request'; pushOnly?: boolean }      // follower → leader：请跑一次同步(pushOnly=以本地为准)
  | { kind: 'conflict'; info: ConflictInfo }     // leader → 所有窗口：已挂起冲突
  | { kind: 'resolved' }                          // leader → 所有窗口：冲突已裁决
  | { kind: 'resolve'; strategy: ConflictStrategy } // follower → leader：用户在弹窗里选了策略

/** 广播一条同步协调消息 */
export function notifySyncCoor(payload: SyncCoorPayload): void {
  if (!isTauriEnv()) return
  emit(SYNC_COOR_EVENT, payload).catch(() => {})
}

/** 订阅同步协调消息（重复订阅由调用方自行避免） */
export function onSyncCoor(cb: (p: SyncCoorPayload) => void): void {
  if (!isTauriEnv()) return
  listen<SyncCoorPayload>(SYNC_COOR_EVENT, (e) => cb(e.payload)).catch(() => {})
}
