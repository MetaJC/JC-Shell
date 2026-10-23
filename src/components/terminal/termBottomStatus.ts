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
 * 终端底行占用状态（跨组件共享，模块级响应式）
 * 作者：JC
 * 终端铺满整个面板，左下角系统状态条悬浮在面板最底边（只够压住视口最底一行）。
 * TerminalView 检测该行是否有可见文字，TerminalWorkspace 据此把状态条降到 25% 不透明度 ——
 * 底行有文字时让内容透出来，底行为空时自动恢复 100%。
 */
import { reactive } from 'vue'

/** tabId -> 视口最底一行是否有可见文字 */
export const termBottomOccupied = reactive<Record<string, boolean>>({})

export function setTermBottomOccupied(tabId: string, occupied: boolean): void {
  termBottomOccupied[tabId] = occupied
}

export function clearTermBottomOccupied(tabId: string): void {
  delete termBottomOccupied[tabId]
}
