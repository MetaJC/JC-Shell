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
 * 统一「打开设置」入口
 * 作者：JC
 *
 * 背景：设置面板有两个宿主 —— 独立设置窗口（Rust open_settings，label=settings）
 * 和主窗口内的 el-dialog。过去不同入口各走一条路（顶部菜单走弹窗、头像菜单与原生菜单
 * 走独立窗口、Ctrl+, 走弹窗），导致形态不一致，也出现过关闭时误伤主窗口的问题。
 * 这里收敛成唯一入口：所有调用方一律走 openSettings()，形态自然一致。
 *
 * 策略：
 *   1) Tauri 环境 → 走 Rust 独立设置窗口（与原生菜单、头像菜单完全一致）
 *   2) 调用失败或非 Tauri 环境 → 降级为主窗口内 el-dialog，保证点了一定有反应
 */
import { invoke, isTauriEnv } from '@/api'
import { useAppStore, type SettingsTab } from '@/stores/app'
import { CAP } from '@/utils/platform'

/** 打开设置面板。tab 为目标分组（general / host / shortcut / about ...） */
export async function openSettings(tab: SettingsTab = 'general'): Promise<void> {
  // 移动端（Android）：Tauri 所有窗口渲染进同一 Activity 视图栈，
  // 独立设置窗口会永久置顶盖住主窗口 → 直接走主窗口内 el-dialog 形态。
  // 桌面端保持原有独立设置窗口逻辑，行为完全不变。
  if (isTauriEnv() && CAP.multiWindow()) {
    try {
      await invoke<void>('open_settings', { tab })
      return
    } catch {
      // Rust 端不可用（命令缺失 / 未重新编译）→ 降级为主窗口内弹窗
    }
  }
  useAppStore().openSettings(tab)
}
