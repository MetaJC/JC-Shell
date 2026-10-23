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
 * 配置变更信号（解耦 store 之间的循环依赖）
 * 作者：JC
 *
 * 谁改了配置只需要喊一声, 同步 store 订阅后自己安排防抖推送。
 * 不用直接 import 同步 store: settings/host 是被同步的对象, 反向依赖会成环。
 */
export type ConfigDocKey = 'hosts' | 'prefs'

type Listener = (doc: ConfigDocKey) => void

const listeners = new Set<Listener>()

/** 广播"某份配置变了" */
export function emitConfigChanged(doc: ConfigDocKey): void {
  for (const fn of [...listeners]) {
    try {
      fn(doc)
    } catch { /* 订阅者异常不能影响业务写盘 */ }
  }
}

/** 订阅配置变更, 返回取消订阅函数 */
export function onConfigChanged(fn: Listener): () => void {
  listeners.add(fn)
  return () => listeners.delete(fn)
}
