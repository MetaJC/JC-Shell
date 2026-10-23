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
 * 会话导入 / 导出弹窗的单例状态
 * 作者：JC
 *
 * 两个入口都要能打开它：
 *   1. 顶部菜单「工具 → 导入会话 / 导出会话」（在 MainWindow 层处理菜单 action）；
 *   2. 会话列表面板工具栏的两个按钮。
 * 两者不在同一组件树分支上，故用响应式单例驱动，
 * 由 MainWindow 统一挂载 <SessionTransferDialog />（与 securityDialog 同一套路）。
 */
import { reactive } from 'vue'

interface TransferDlgState {
  visible: boolean
  /** 'import' = 导入（一键/从文件）；'export' = 导出 */
  view: 'import' | 'export'
}

export const transferDlg = reactive<TransferDlgState>({
  visible: false,
  view: 'import',
})

/** 打开「导入会话」弹窗（一键导入 / 从文件导入） */
export function openImportSessions(): void {
  transferDlg.view = 'import'
  transferDlg.visible = true
}

/** 打开「导出会话」弹窗 */
export function openExportSessions(): void {
  transferDlg.view = 'export'
  transferDlg.visible = true
}
