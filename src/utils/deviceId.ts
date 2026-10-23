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
 * 设备标识（配置云同步用）
 * 作者：JC
 *
 * 为什么需要它：跨设备合并时，两个条目可能带着几乎相同的修改时间（客户端时钟只精确到毫秒，
 * 还可能有几分钟漂移），必须有第二个稳定依据来定序，否则两台设备会各自收敛出不同结果。
 * 因此这个值必须"同一台机器上长期不变"：生成一次写进 localStorage，之后所有窗口、所有同步流程复用。
 */
const DEVICE_KEY = 'jc_shell_device_id'

let cached = ''

/** 当前设备标识，形如 dev_m2k3x9_ab12cd34 */
export function deviceId(): string {
  if (cached) return cached
  try {
    const saved = localStorage.getItem(DEVICE_KEY)
    if (saved) {
      cached = saved
      return cached
    }
  } catch { /* 存储不可用: 退化为进程内随机值, 只影响本次会话的定序 */ }
  const gen = 'dev_' + Date.now().toString(36) + '_' + Math.random().toString(36).slice(2, 10)
  try { localStorage.setItem(DEVICE_KEY, gen) } catch { /* ignore */ }
  cached = gen
  return cached
}
