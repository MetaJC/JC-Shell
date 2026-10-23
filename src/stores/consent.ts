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
 * 法律协议同意 Store
 * 作者：JC
 *
 * 职责（仅做数据支撑，不做弹窗拦截）：
 *  - 拉取后台法律协议配置（版本号 + 链接 + 须重新同意的最低版本），供登录/注册表单判断。
 *  - 登录/注册表单里自带「已阅读并同意」复选框：未勾选则阻止提交并提示（见
 *    MobilePhoneLogin / PC 登录入口），不另弹全屏门禁（应用不强制登录）。
 *  - 用户勾选同意后：暂存待提交版本；登录/注册成功后由 auth store 调 submitIfPending() 上报落库。
 *
 * 真正的强制拦截由后端在登录时兜底（phone-login 的 enforceAndRecord）。
 */
import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@/api'

export interface LegalAgreementConfig {
  enabled?: boolean | null
  userAgreementVersion?: string | null
  userAgreementUrl?: string | null
  userAgreementRequiredVersion?: string | null
  privacyPolicyVersion?: string | null
  privacyPolicyUrl?: string | null
  privacyPolicyRequiredVersion?: string | null
}

export const useConsentStore = defineStore('consent', () => {
  const config = ref<LegalAgreementConfig | null>(null)
  const loading = ref(false)
  /** 待提交给后端的已同意版本（登录/注册完成后清空） */
  const pending = ref<{ userAgreementVersion: string; privacyPolicyVersion: string } | null>(null)

  async function fetchConfig() {
    loading.value = true
    try {
      const data = await invoke<LegalAgreementConfig | null>('get_legal_agreement')
      config.value = data ?? null
    } catch {
      // 拉取失败：按"无配置"处理；后端登录时会兜底拦截
      config.value = null
    } finally {
      loading.value = false
    }
  }

  /** 用户勾选「已阅读并同意」：暂存待提交版本（登录/注册成功后上报） */
  function markAgreed() {
    const c = config.value
    if (!c) return
    pending.value = {
      userAgreementVersion: c.userAgreementVersion || '',
      privacyPolicyVersion: c.privacyPolicyVersion || '',
    }
  }

  /** 登录/注册完成后由 auth store 调用：把暂存的已同意版本上报后端 */
  async function submitIfPending() {
    if (!pending.value) return
    const p = pending.value
    pending.value = null
    try {
      await invoke('submit_agreement', {
        userAgreementVersion: p.userAgreementVersion,
        privacyPolicyVersion: p.privacyPolicyVersion,
      })
    } catch (e) {
      console.error('[consent] submit_agreement 失败:', e)
    }
  }

  /** 外部（如协议版本变更后）强制重新评估 */
  function reset() {
    pending.value = null
    void fetchConfig()
  }

  return {
    config,
    loading,
    pending,
    fetchConfig,
    markAgreed,
    submitIfPending,
    reset,
  }
})
