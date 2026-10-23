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
 * 「换一种验证方式」选择器的单例状态。
 * 生物识别识别失败 / 取消后，用它承接用户的选择：改用指纹、改用人脸、或改用安全密码。
 * 用法与 securityDialog 一致：命令式 API + 响应式单例驱动组件。
 * 作者：JC
 */
import { reactive } from 'vue'
import type { BiometricKind } from './biometric'

/** 'fingerprint' | 'face' = 再试一次该模态；'pin' = 改用安全密码；null = 关闭（等同改用密码） */
export type BiometricMethodPick = BiometricKind | 'pin' | null

interface BiometricMethodState {
  visible: boolean
  /** 可供选择重新尝试的模态（已按「本机具备 + 用户允许」过滤） */
  kinds: BiometricKind[]
  resolve: ((v: BiometricMethodPick) => void) | null
}

export const biometricDlg = reactive<BiometricMethodState>({
  visible: false,
  kinds: [],
  resolve: null,
})

/** 打开选择器：kinds 为空时只剩「使用安全密码」一个有效选项 */
export function openBiometricMethodDialog(opts: { kinds: BiometricKind[] }): Promise<BiometricMethodPick> {
  return new Promise((resolve) => {
    biometricDlg.kinds = opts.kinds
    biometricDlg.visible = true
    biometricDlg.resolve = resolve
  })
}

/** 由 <BiometricMethodDialog /> 在用户操作后调用 */
export function closeBiometricMethodDialog(v: BiometricMethodPick): void {
  const r = biometricDlg.resolve
  biometricDlg.visible = false
  biometricDlg.resolve = null
  r?.(v)
}
