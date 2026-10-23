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
  <div class="ai-panel h-full flex flex-col bg-[var(--jc-bg-soft)] border-l border-[var(--jc-border)]">
    <div class="flex items-center justify-between px-3 py-2 border-b border-[var(--jc-border)]">
      <div class="flex items-center gap-2">
        <el-icon color="#3b82f6"><MagicStick /></el-icon>
        <span class="font-medium text-sm text-[var(--jc-text)]">AI 助手</span>
      </div>
      <el-button size="small" text @click="store.clear" :disabled="store.sending">
        <el-icon><Delete /></el-icon>
      </el-button>
    </div>

    <div class="flex-1 overflow-y-auto p-3 space-y-3">
      <div v-if="store.messages.length === 0" class="text-center text-[var(--jc-text-muted)] text-sm mt-8">
        <div class="mb-2"><el-icon size="32"><ChatDotRound /></el-icon></div>
        输入描述，让 AI 帮你生成 Shell 命令或解释错误
      </div>
      <div v-for="m in store.messages" :key="m.id"
           :class="['rounded-lg p-3 text-sm whitespace-pre-wrap break-words',
                   m.role === 'user' ? 'bg-blue-600/20 text-blue-100 ml-8' : 'bg-[var(--jc-bg-hover)] text-[var(--jc-text)] mr-8 border border-[var(--jc-border)]']">
        <div class="flex items-center gap-1 mb-1 text-xs opacity-70">
          <el-icon v-if="m.role === 'user'" size="12"><User /></el-icon>
          <el-icon v-else size="12"><MagicStick /></el-icon>
          <span>{{ m.role === 'user' ? '你' : 'AI' }}{{ m.streaming ? ' · 生成中…' : '' }}</span>
        </div>
        <div>{{ m.content }}<span v-if="m.streaming" class="inline-block w-1.5 h-3 align-middle bg-blue-400 ml-0.5 animate-pulse"></span></div>
      </div>
    </div>

    <div class="p-3 border-t border-[var(--jc-border)]">
      <div class="flex gap-2">
        <el-input
          v-model="input"
          type="textarea"
          :rows="2"
          placeholder="例如：列出占用磁盘最大的 10 个目录"
          resize="none"
          :disabled="store.sending"
          @keydown.ctrl.enter.exact.prevent="send"
        />
        <el-button type="primary" :loading="store.sending" @click="send" :disabled="!input.trim()">
          发送<br /><span class="text-xs opacity-70">Ctrl+Enter</span>
        </el-button>
      </div>
      <div class="mt-2 flex gap-2">
        <el-tag size="small" effect="plain" type="info">模型：{{ store.config.model }}</el-tag>
        <el-tag size="small" effect="plain" type="success">提供方：{{ store.config.provider }}</el-tag>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useAiStore } from '@/stores/ai'
import { ElMessage } from 'element-plus'

const store = useAiStore()
const input = ref('')

async function send() {
  const content = input.value.trim()
  if (!content || store.sending) return
  input.value = ''
  store.pushUserMessage(content)
  store.sending = true
  const msgId = store.createAssistantPlaceholder()

  // M1: 模拟流式响应（M3 接入真实 Ollama / OpenAI）
  const demo = '好的，以下是列出占用磁盘最大的 10 个目录的命令：\n\n' +
    '```bash\ndu -h --max-depth=1 / | sort -rh | head -10\n```\n\n' +
    '命令说明：\n• du -h：以人类可读格式显示大小\n• --max-depth=1：只统计一级目录\n• sort -rh：按大小逆序排序\n• head -10：取前 10 条\n\n> 如需解释输出或调整参数，直接告诉我即可。'
  let i = 0
  const tick = () => {
    if (i >= demo.length) {
      store.finishMessage(msgId)
      store.sending = false
      return
    }
    const chunk = demo.slice(i, i + 3)
    i += 3
    store.appendChunk(msgId, chunk)
    setTimeout(tick, 12)
  }
  setTimeout(tick, 100)
  ElMessage.info('当前为 M1 演示模式，M3 将接入真实模型')
}
</script>
