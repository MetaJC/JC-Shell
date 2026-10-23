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

import { createRouter, createWebHashHistory, RouteRecordRaw } from 'vue-router'
import MainWindow from '@/views/MainWindow.vue'
import TerminalWindow from '@/views/TerminalWindow.vue'
import SettingsView from '@/views/SettingsView.vue'
import FeedbackView from '@/views/FeedbackView.vue'
import TabGhostView from '@/views/TabGhostView.vue'

const routes: RouteRecordRaw[] = [
  {
    path: '/',
    name: 'Main',
    component: MainWindow
  },
  {
    // 独立终端窗口（Tab 拖出后接管，query take=<tabId> 等元数据）
    path: '/terminal-window',
    name: 'TerminalWindow',
    component: TerminalWindow
  },
  {
    // 独立设置窗口
    path: '/settings',
    name: 'Settings',
    component: SettingsView
  },
  {
    // 独立反馈窗口
    path: '/feedback',
    name: 'Feedback',
    component: FeedbackView
  },
  {
    // Tab 拖拽虚影窗口（原生小窗口，位置由 Rust 轮询全局光标驱动）
    path: '/tab-ghost',
    name: 'TabGhost',
    component: TabGhostView
  }
]

const router = createRouter({
  history: createWebHashHistory(),
  routes
})

export default router
