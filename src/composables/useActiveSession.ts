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
 * 当前激活终端标签的「会话上下文」
 * 作者：JC
 *
 * 设备信息面板 / 文件系统面板（SFTP）都依赖这份上下文去加载数据，
 * 也就是产品语义上的「跟随当前选中的终端」。
 *
 * 抽成 composable 的目的：桌面端（停靠面板）与移动端（独立页面）共用同一份判定逻辑，
 * 保证「跟随选中终端」只有一处实现，不会两边各写一套后逐渐跑偏。
 */
import { computed } from 'vue'
import { useTerminalStore } from '@/stores/terminal'

export function useActiveSession() {
  const termStore = useTerminalStore()

  /** 当前激活的终端标签 */
  const activeTab = computed(() => termStore.tabs.find(t => t.id === termStore.activeTabId))

  /** 是否可作为数据源：必须已连接；本地会话没有 host 也算（面板据此决定加载/空态） */
  const connected = computed(() =>
    !!activeTab.value?.connected && (!!activeTab.value?.host || activeTab.value?.kind === 'local')
  )

  /** 展示用主机名：addr 形如 user@host:port → 取 host */
  const hostname = computed(() => {
    const addr = activeTab.value?.host
    if (!addr) return ''
    return addr.split('@')[1]?.split(':')[0] ?? addr
  })

  /** 后端会话 ID：文件系统 / 设备信息面板靠它拉数据 */
  const sessionId = computed(() => activeTab.value?.sessionId)

  /** 会话类型（local / ssh）：决定文件系统的数据读取链路 */
  const kind = computed(() => activeTab.value?.kind)

  return { activeTab, connected, hostname, sessionId, kind }
}
