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
 * 客户端启动弹窗 Store
 * 作者：JC
 *
 * 弹窗内容由管理员在后台配置（Markdown 编辑 + 预览），后台保存时把渲染后的
 * HTML 一并落库，客户端直接展示 HTML，不做二次解析，保证"后台预览 = 客户端展示"。
 *
 * 弹窗时机（后台配置）：
 *   OFF            不弹窗（默认）
 *   EVERY_STARTUP  每次启动都弹
 *   FIRST_ONLY     仅首次启动弹：用 localStorage 记录已看过的 updatedAt，
 *                  内容更新（updatedAt 变化）后视为新弹窗，再弹一次。
 *
 * 拉取失败时一律不弹窗，避免网络异常打断启动流程。
 */
import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@/api'
import { handleApiError } from '@/utils/errorHandler'

export interface LaunchDialogInfo {
  id: number | null
  title: string | null
  /** 渲染后 HTML，直接展示 */
  contentHtml: string | null
  /** OFF=不弹窗 EVERY_STARTUP=每次启动 FIRST_ONLY=仅首次启动 */
  frequency: string | null
  /** LARGE=大弹窗 SMALL=小弹窗(头像上方) */
  dialogSize: string | null
  /** 内容更新时间（字符串），用于判断是否为新弹窗 */
  updatedAt: string | null
}

/** 已看过的弹窗标识（仅首次启动模式使用），与 update store 的"已忽略版本"同款持久化方式 */
const SEEN_KEY = 'jc_shell_launch_dialog_seen'

export const FREQ_OFF = 'OFF'
export const FREQ_EVERY_STARTUP = 'EVERY_STARTUP'
export const FREQ_FIRST_ONLY = 'FIRST_ONLY'

export const SIZE_LARGE = 'LARGE'
export const SIZE_SMALL = 'SMALL'

export const useLaunchDialogStore = defineStore('launchDialog', () => {
  const info = ref<LaunchDialogInfo | null>(null)
  const loading = ref(false)
  const error = ref('')
  const visible = ref(false)

  function readSeen(): string | null {
    try {
      return localStorage.getItem(SEEN_KEY)
    } catch {
      return null
    }
  }

  function writeSeen(v: string) {
    try {
      localStorage.setItem(SEEN_KEY, v)
    } catch {
      /* 隐身模式等场景忽略 */
    }
  }

  /** 当前弹窗的"内容指纹"：优先用 updatedAt，缺失时退回内容本身 */
  function fingerprint(d: LaunchDialogInfo): string {
    return d.updatedAt || d.contentHtml || ''
  }

  /**
   * 拉取弹窗配置。
   * @returns 是否需要自动弹出（调用方据此决定是否打开弹窗）
   */
  async function fetchDialog(): Promise<boolean> {
    loading.value = true
    error.value = ''
    try {
      const data = await invoke<LaunchDialogInfo | null>('get_launch_dialog')
      info.value = data ?? null
      if (!data || !data.contentHtml) return false

      const freq = (data.frequency || FREQ_OFF).trim()
      if (freq === FREQ_OFF) return false
      if (freq === FREQ_EVERY_STARTUP) return true
      if (freq === FREQ_FIRST_ONLY) {
        return readSeen() !== fingerprint(data)
      }
      // 未知频率值：不弹
      return false
    } catch (e) {
      // 拉取失败：静默不弹窗（网络异常不应打断启动）
      const friendly = handleApiError(e, 'stores/launchDialog.fetchDialog', {
        url: 'get_launch_dialog',
        showToast: false,
      })
      error.value = friendly
      return false
    } finally {
      loading.value = false
    }
  }

  /** 标记已读（仅首次启动模式需要持久化） */
  function markSeen() {
    const d = info.value
    if (!d) return
    if ((d.frequency || FREQ_OFF).trim() !== FREQ_FIRST_ONLY) return
    writeSeen(fingerprint(d))
  }

  /** 是否有可展示的内容（决定红框处的入口按钮是否显示） */
  function hasContent(): boolean {
    return !!info.value?.contentHtml
  }

  function openDialog() {
    visible.value = true
  }

  function closeDialog() {
    visible.value = false
    markSeen()
  }

  return {
    info,
    loading,
    error,
    visible,
    fetchDialog,
    openDialog,
    closeDialog,
    markSeen,
    hasContent,
  }
})
