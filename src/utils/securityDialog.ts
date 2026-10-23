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
 * 安全密码弹窗的单例状态。
 * 用响应式单例驱动 <SecurityDialog />（挂在 MainWindow 下，始终存活），
 * 提供命令式 API：openSetPassword / openVerify，避免每个调用点各挂一个弹窗。
 * 作者：JC
 */
import { reactive } from 'vue'
import type { SecurityScopes } from './securityPassword'

export interface SecDlgVerifyOpts {
  message: string
  /** 是否在验证弹窗内显示「抹除会话数据并关闭」按钮（关闭安全密码时用） */
  showWipe?: boolean
  title?: string
}

export interface SecDlgResult {
  action: 'ok' | 'wipe' | 'cancel'
  /** 用户输入的 4 位 PIN（仅 action==='ok' 时有效） */
  pin: string
}

interface SecDlgState {
  visible: boolean
  mode: 'set' | 'verify'
  title: string
  message: string
  /** 醒目风险提示（开启 / 关闭场景各有） */
  riskWarning: string
  showWipe: boolean
  scopes: SecurityScopes
  resolve: ((r: SecDlgResult) => void) | null
}

export const secDlg = reactive<SecDlgState>({
  visible: false,
  mode: 'set',
  title: '',
  message: '',
  riskWarning: '',
  showWipe: false,
  scopes: { view_password: true, startup: false, edit_delete: false, export_data: false, export_password: true },
  resolve: null,
})

const SET_RISK = '开启后，只能通过验证密码或抹除数据来关闭安全密码，请谨慎操作。'
const VERIFY_RISK = '验证安全密码以继续。连续 5 次错误将锁定 10 分钟。'

export function openSetPassword(scopes: SecurityScopes): Promise<{ pin: string } | null> {
  return new Promise((resolve) => {
    secDlg.mode = 'set'
    secDlg.title = '设置安全密码'
    secDlg.message = '请设置 4 位数字安全密码，用于保护敏感操作。'
    secDlg.riskWarning = SET_RISK
    secDlg.scopes = scopes
    secDlg.visible = true
    secDlg.resolve = (r) => resolve(r.action === 'ok' ? { pin: r.pin } : null)
  })
}

export function openVerify(opts: SecDlgVerifyOpts): Promise<SecDlgResult> {
  return new Promise((resolve) => {
    secDlg.mode = 'verify'
    secDlg.title = opts.title || '安全验证'
    secDlg.message = opts.message
    secDlg.riskWarning = VERIFY_RISK
    secDlg.showWipe = !!opts.showWipe
    secDlg.visible = true
    secDlg.resolve = resolve
  })
}

/** 由 <SecurityDialog /> 在用户操作后调用，关闭弹窗并回传结果 */
export function closeSecDlg(result: SecDlgResult) {
  const r = secDlg.resolve
  secDlg.visible = false
  secDlg.resolve = null
  r?.(result)
}
