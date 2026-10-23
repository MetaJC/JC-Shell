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

import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { invoke } from '@/api'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { friendlyError, friendlyText } from '@/utils/errorHandler'
import { useConsentStore } from '@/stores/consent'

/** 当前用户信息 */
export interface UserInfo {
  id: number
  username: string
  nickname?: string | null
  email?: string | null
  avatar?: string | null
  phone?: string | null
}

/** 登录态 */
export type AuthState =
  | { status: 'anonymous' }
  | { status: 'logging_in'; login_url: string }
  | { status: 'authenticated'; user: UserInfo }

/**
 * 桌面端登录认证 store
 * 作者：JC
 */
export const useAuthStore = defineStore('auth', () => {
  const state = ref<AuthState>({ status: 'anonymous' })
  const loading = ref(false)
  const errorMsg = ref('')
  /** 登录态是否已从 KeyChain/后端读回; 未读回前无法判断数据归属哪个账号 */
  const resolved = ref(false)

  let unlistenStateChanged: UnlistenFn | null = null
  let unlistenLoginError: UnlistenFn | null = null
  let unlistenLoginTimeout: UnlistenFn | null = null

  const isAuthenticated = computed(() => state.value.status === 'authenticated')
  const user = computed(() => (state.value.status === 'authenticated' ? state.value.user : null))
  const isLoggingIn = computed(() => state.value.status === 'logging_in')

  /** 并发去重: 多个窗口/store 同时要登录态时只打一次 /auth/me */
  let pending: Promise<void> | null = null

  /** 启动时拉取当前登录态 (从 KeyChain 读 token + 调后端 /auth/me) */
  async function load() {
    if (pending) return pending
    pending = (async () => {
      try {
        state.value = await invoke<AuthState>('get_auth_state')
      } catch (e: any) {
        // 友好文案只给用户看；技术细节进 console（真机上用 chrome://inspect 排障）
        console.error('[auth] get_auth_state 失败:', e)
        errorMsg.value = friendlyError(e)
      } finally {
        resolved.value = true
      }
    })()
    try {
      await pending
    } finally {
      pending = null
    }
  }

  /** 发起登录: 启动本地回调 + 打开浏览器 */
  async function startLogin() {
    loading.value = true
    errorMsg.value = ''
    try {
      state.value = await invoke<AuthState>('start_login')
    } catch (e: any) {
      // 「网络异常」大多是后端地址在手机上不可达（见 docs/安卓调试与打包.md 排障速查）：
      // 技术细节只进 console，不进 UI（errorHandler 铁律）
      console.error('[auth] start_login 失败:', e)
      errorMsg.value = friendlyError(e)
      loading.value = false
    }
  }

  /** 取消登录: 中断当前浏览器授权流程并恢复未登录状态 */
  async function cancelLogin() {
    try {
      await invoke('cancel_login')
    } catch (e: any) {
      errorMsg.value = friendlyError(e)
    }
    state.value = { status: 'anonymous' }
    loading.value = false
  }

  /** 登出: 吊销 token + 清理 KeyChain */
  async function logout() {
    try {
      await invoke('logout')
      state.value = { status: 'anonymous' }
    } catch (e: any) {
      errorMsg.value = friendlyError(e)
    }
  }

  /** 订阅 Rust 推送的登录事件 (auth_state_changed / auth_login_error / auth_login_timeout) */
  async function subscribeEvents() {
    if (unlistenStateChanged) return
    // 浏览器 fallback / 部分 Tauri 版本可能缺少 transformCallback：订阅失败不抛异常，不阻塞主 UI 渲染
    const safeListen = async <T,>(evt: string, cb: (payload: T) => void): Promise<UnlistenFn | null> => {
      try {
        return await listen<T>(evt, (e) => cb(e.payload))
      } catch {
        return null
      }
    }
    unlistenStateChanged = await safeListen<AuthState>('auth_state_changed', (payload) => {
      state.value = payload
      loading.value = false
      // 登录成功后，若存在待提交的协议同意记录（OAuth 流程在门禁处已勾选），上报后端落库
      if (payload.status === 'authenticated') {
        void useConsentStore().submitIfPending()
      }
    })
    unlistenLoginError = await safeListen<string>('auth_login_error', (payload) => {
      // payload 来自 Rust 登录错误事件，可能夹带技术细节，收敛后再展示
      errorMsg.value = friendlyText(payload)
      loading.value = false
    })
    unlistenLoginTimeout = await safeListen<number>('auth_login_timeout', () => {
      errorMsg.value = '登录超时，请重试'
      loading.value = false
      state.value = { status: 'anonymous' }
    })
  }

  /** 退出时取消订阅 */
  function unsubscribeEvents() {
    unlistenStateChanged?.()
    unlistenLoginError?.()
    unlistenLoginTimeout?.()
    unlistenStateChanged = null
    unlistenLoginError = null
    unlistenLoginTimeout = null
  }

  /** 清除错误提示 */
  function clearError() { errorMsg.value = '' }

  return {
    state, loading, errorMsg, resolved,
    isAuthenticated, user, isLoggingIn,
    load, startLogin, cancelLogin, logout, clearError,
    subscribeEvents, unsubscribeEvents
  }
})