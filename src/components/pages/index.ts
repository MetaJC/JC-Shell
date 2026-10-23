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
 * 侧边 Dock 页面注册表
 * 作者：JC
 * 新增一个 Dock 页面只需：写一个页面组件 + 在下表登记一行。
 * 注意：terminal（终端页）不在此表内 —— 它需要承载 DockLayout 的默认插槽，
 * 由 DockLayout 单独以常驻挂载方式渲染，避免切页时中断终端会话。
 */
import type { Component } from 'vue'
import FtpPage from './FtpPage.vue'
import PortForwardPage from '@/components/portforward/PortForwardPage.vue'
// 移动端底部导航的独立页面（会话 / SFTP / 仪表盘 / 我的）
import MobileSessionsPage from './mobile/MobileSessionsPage.vue'
import MobileSftpPage from './mobile/MobileSftpPage.vue'
import MobileDashboardPage from './mobile/MobileDashboardPage.vue'
import MobileProfilePage from './mobile/MobileProfilePage.vue'

export const DOCK_PAGES: Record<string, Component> = {
  ftp: FtpPage,
  portForward: PortForwardPage,
  /**
   * 以下 4 个页面仅由移动端底部导航（MobileTabBar）选中。
   * 桌面端导航只有 terminal / ftp / portForward 三项，因此这些 id 在桌面端不可达，
   * 桌面行为与本次改动前完全一致。
   */
  sessions: MobileSessionsPage,
  sftp: MobileSftpPage,
  dashboard: MobileDashboardPage,
  profile: MobileProfilePage,
}
