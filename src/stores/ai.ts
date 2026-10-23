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
import { ref } from 'vue'

export interface AiMessage {
  id: string
  role: 'user' | 'assistant' | 'system'
  content: string
  streaming?: boolean
  timestamp: number
}

export interface AiModelConfig {
  provider: 'ollama' | 'openai-compat'
  baseUrl: string
  model: string
  apiKey?: string
}

export const useAiStore = defineStore('ai', () => {
  const messages = ref<AiMessage[]>([])
  const sending = ref(false)
  const config = ref<AiModelConfig>({
    provider: 'ollama',
    baseUrl: 'http://127.0.0.1:11434/v1',
    model: 'qwen3:8b'
  })

  function pushUserMessage(content: string): string {
    const id = 'msg-' + Date.now()
    messages.value.push({
      id,
      role: 'user',
      content,
      timestamp: Date.now()
    })
    return id
  }

  function createAssistantPlaceholder(): string {
    const id = 'msg-' + Date.now() + '-ai'
    messages.value.push({
      id,
      role: 'assistant',
      content: '',
      streaming: true,
      timestamp: Date.now()
    })
    return id
  }

  function appendChunk(msgId: string, chunk: string) {
    const msg = messages.value.find(m => m.id === msgId)
    if (msg) msg.content += chunk
  }

  function finishMessage(msgId: string) {
    const msg = messages.value.find(m => m.id === msgId)
    if (msg) msg.streaming = false
  }

  function clear() {
    messages.value = []
  }

  return { messages, sending, config, pushUserMessage, createAssistantPlaceholder, appendChunk, finishMessage, clear }
})
