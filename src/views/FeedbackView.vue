<!--
  Copyright 2024-2026 OpenJC (https://www.openjc.cc)
  
  Licensed under the Apache License, Version 2.0 (the "License");
  you may not use this file except in compliance with the License.
  You may obtain a copy of the License at
  
      http://www.apache.org/licenses/LICENSE-2.0
  
  Unless required by applicable law or agreed to in writing, software
  distributed under the License is distributed on an "AS IS" BASIS,
  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
  See the License for the specific language governing permissions and
  limitations under the License.
-->

<template>
  <div class="h-full w-full bg-[var(--jc-bg)] text-[var(--jc-text)] flex flex-col overflow-hidden">
    <div class="flex-1 min-h-0 overflow-auto p-6">
      <div class="max-w-2xl mx-auto">
        <h1 class="text-xl font-semibold mb-1">提交反馈</h1>
        <p class="text-sm opacity-70 mb-5">
          遇到问题或有建议？告诉我们，我们会尽快处理。当前产品：
          <span class="font-mono">{{ productCode }}</span>
        </p>

        <el-form :model="form" label-position="top" class="feedback-form">
          <el-form-item label="问题类型" required>
            <el-select v-model="form.type" placeholder="请选择" style="width: 100%">
              <el-option label="功能建议" value="feature" />
              <el-option label="问题反馈" value="bug" />
              <el-option label="使用咨询" value="question" />
              <el-option label="其他" value="other" />
            </el-select>
          </el-form-item>
          <el-form-item label="标题" required>
            <el-input v-model="form.title" maxlength="60" show-word-limit placeholder="一句话描述问题/建议" />
          </el-form-item>
          <el-form-item label="详细描述" required>
            <el-input
              v-model="form.content"
              type="textarea"
              :rows="8"
              maxlength="2000"
              show-word-limit
              placeholder="请详细描述现象、复现步骤、期望结果等"
            />
          </el-form-item>
          <el-form-item label="联系方式（选填）">
            <el-input v-model="form.contact" placeholder="邮箱 / QQ / 手机号，便于我们回访" />
          </el-form-item>
        </el-form>

        <div class="flex items-center gap-2 mt-2">
          <el-button type="primary" :loading="submitting" :disabled="!canSubmit" @click="onSubmit">
            提交反馈
          </el-button>
          <el-button @click="onReset">重置</el-button>
          <el-button v-if="externalUrl" link @click="openExternal">
            <el-icon style="margin-right: 4px"><Link /></el-icon>在主站打开
          </el-button>
        </div>

        <el-alert
          v-if="resultMsg"
          :type="resultOk ? 'success' : 'error'"
          :title="resultMsg"
          :closable="false"
          show-icon
          class="mt-4"
        />

        <div class="text-xs opacity-60 mt-6 leading-6">
          <div>客户端版本：<span class="font-mono">{{ clientVersion }}</span></div>
          <div>运行环境：<span class="font-mono">{{ envInfo }}</span></div>
          <div v-if="externalUrl">主站反馈地址：<span class="font-mono break-all">{{ externalUrl }}</span></div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 独立反馈窗口根组件
 * 作者：JC
 * 在 Tauri 内置 webview 窗口里运行，URL = /feedback?ext=<主站反馈页 URL>
 * 表单提交：优先 POST 到主站 /api/openjc/client/feedback（后端未部署则降级为本地保存 + 主站链接）。
 */
import { ref, computed, onMounted } from 'vue'
import { Link } from '@element-plus/icons-vue'
import { ElMessage } from 'element-plus'
import { invoke, isTauriEnv } from '@/api'
import { useAuthStore } from '@/stores/auth'

interface FeedbackForm {
  type: 'feature' | 'bug' | 'question' | 'other' | ''
  title: string
  content: string
  contact: string
}

const auth = useAuthStore()

const productCode = 'jc-shell'
const clientVersion = ref('0.0.0')
const envInfo = ref('browser')
const externalUrl = ref('')

const form = ref<FeedbackForm>({ type: '', title: '', content: '', contact: '' })
const submitting = ref(false)
const resultMsg = ref('')
const resultOk = ref(false)

const canSubmit = computed(() =>
  !!form.value.type && form.value.title.trim().length > 0 && form.value.content.trim().length > 0
)

/** 解析 URL ?ext= 参数（Rust 端拼的 URL 段编码） */
function applyUrl() {
  try {
    const sp = new URLSearchParams(window.location.search)
    const ext = sp.get('ext')?.trim()
    if (ext) externalUrl.value = decodeURIComponent(ext)
  } catch { /* ignore */ }
}

onMounted(async () => {
  applyUrl()
  // 取版本号：优先从 Vite 注入的全局变量读取
  try {
    if (__APP_VERSION__) clientVersion.value = String(__APP_VERSION__)
  } catch { /* ignore */ }
  envInfo.value = isTauriEnv() ? 'Tauri' : 'Browser'
  if (isTauriEnv()) {
    try { clientVersion.value = await invoke<string>('get_app_version') } catch { /* ignore */ }
  }
})

async function onSubmit() {
  if (!canSubmit.value) return
  submitting.value = true
  resultMsg.value = ''
  try {
    // 1) 优先尝试 POST 到主站
    if (externalUrl.value) {
      try {
        const apiBase = externalUrl.value.replace(/\/feedback.*$/, '')
        const resp = await fetch(`${apiBase}/api/openjc/client/feedback`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            product: productCode,
            type: form.value.type,
            title: form.value.title.trim(),
            content: form.value.content.trim(),
            contact: form.value.contact.trim(),
            clientVersion: clientVersion.value,
            user: auth.state.status === 'authenticated' ? auth.state.user : null,
          }),
        })
        if (resp.ok) {
          resultOk.value = true
          resultMsg.value = '反馈已提交，感谢支持！'
          ElMessage.success('反馈已提交')
          return
        }
      } catch { /* 网络/主站不可达：降级为本地保存 */ }
    }
    // 2) 降级：本地持久化（不阻塞用户），并提示在主站提交
    try {
      const list = JSON.parse(localStorage.getItem('jc_shell_pending_feedback') || '[]')
      list.push({
        ts: new Date().toISOString(),
        product: productCode,
        type: form.value.type,
        title: form.value.title.trim(),
        content: form.value.content.trim(),
        contact: form.value.contact.trim(),
        clientVersion: clientVersion.value,
      })
      localStorage.setItem('jc_shell_pending_feedback', JSON.stringify(list))
    } catch { /* ignore */ }
    resultOk.value = true
    resultMsg.value = '反馈已暂存到本地，可在主站反馈页再次提交以确保送达。'
  } catch (e: any) {
    resultOk.value = false
    resultMsg.value = '提交失败：' + (e?.message || String(e))
  } finally {
    submitting.value = false
  }
}

function onReset() {
  form.value = { type: '', title: '', content: '', contact: '' }
  resultMsg.value = ''
}

/** 打开主站反馈页（系统浏览器） */
async function openExternal() {
  if (!externalUrl.value) return
  try {
    if (isTauriEnv()) {
      const { open: openInOs } = await import('@tauri-apps/plugin-shell')
      await openInOs(externalUrl.value)
    } else {
      window.open(externalUrl.value, '_blank')
    }
  } catch (e) {
    console.warn('[feedback] open external failed', e)
  }
}
</script>

<style scoped>
.feedback-form :deep(.el-form-item__label) {
  color: var(--jc-text-soft);
  font-weight: 500;
}
</style>
