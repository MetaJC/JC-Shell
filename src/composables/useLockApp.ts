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
 * 锁定应用（统一安全密码）
 * 作者：JC
 *
 * 桌面端（全局快捷键）与移动端（「我的」页）共用同一入口，
 * 未设置安全密码时提示而不是直接锁上 —— 否则锁屏后无法解开。
 */
import { ElMessage } from 'element-plus'
import { useAppStore } from '@/stores/app'
import { securityStatus } from '@/utils/securityPassword'

export async function lockAppIfPossible(): Promise<void> {
  try {
    const st = await securityStatus()
    if (!st.has_password) {
      ElMessage.warning('尚未设置安全密码，请先在「设置 → 安全」中开启后再锁定')
      return
    }
    useAppStore().locked = true
  } catch { /* 后端未就绪忽略 */ }
}
