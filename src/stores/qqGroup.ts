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
 * QQ群 Pinia Store
 * 作者：JC
 * 拉取当前产品（默认 jc-shell）的QQ群列表，deleted=1 表示已下线。
 *
 * 错误处理：统一走 handleApiError（友好提示 + 静默上报），不在 store 里暴露技术细节。
 */
import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@/api'
import { handleApiError } from '@/utils/errorHandler'

export interface QqGroupItem {
  id: number
  qqNumber: string
  groupName?: string
  sortOrder?: number
  deleted: number
  qrCodeUrl?: string
  projectId?: number
  projectCode?: string
  projectName?: string
}

export const useQqGroupStore = defineStore('qqGroup', () => {
  const groups = ref<QqGroupItem[]>([])
  const loading = ref(false)
  const error = ref('')

  async function fetchGroups(productCode = 'jc-shell') {
    loading.value = true
    error.value = ''
    try {
      const data = await invoke<QqGroupItem[]>('get_qq_groups', { productCode })
      // 倒序：最新群（id 最大）排最上方
      const list = Array.isArray(data) ? [...data] : []
      list.sort((a, b) => b.id - a.id)
      groups.value = list
    } catch (e) {
      // 统一处理：友好提示（不弹 ElMessage，弹窗内部展示），同时静默上报到主站
      const friendly = handleApiError(e, 'stores/qqGroup.fetchGroups', {
        url: `get_qq_groups?productCode=${productCode}`,
        showToast: false,
      })
      error.value = friendly
    } finally {
      loading.value = false
    }
  }

  return { groups, loading, error, fetchGroups }
})
