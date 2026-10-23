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
 * 远程文件编辑器 Store
 * 作者：JC
 * 管理多 Tab 打开的文件：读取（字节 → 按编码解码）、保存（按编码写回服务器/本机）、
 * 脏标记跟踪、编码切换重解码。编辑器窗口（FileEditorOverlay）据此渲染。
 */
import { defineStore } from 'pinia'
import { ref, computed, reactive } from 'vue'
import { invoke } from '@/api'
import { friendlyError } from '@/utils/errorHandler'

/** 编辑器读取文件超时（30s）：SSH 大文件/网络抖动避免 UI 长时间卡 loading */
const READ_FILE_TIMEOUT_MS = 30000

/** 编辑器单文件上限（与后端 read_remote_file 默认一致） */
export const EDITOR_MAX_BYTES = 16 * 1024 * 1024

/** 支持的文件编码（WHATWG 标签；读取用 TextDecoder，写回由后端 encoding_rs 完成） */
export const EDITOR_ENCODINGS: { label: string; value: string }[] = [
  { label: 'UTF-8', value: 'utf-8' },
  { label: 'GBK (简体中文)', value: 'gbk' },
  { label: 'GB18030', value: 'gb18030' },
  { label: 'Big5 (繁体中文)', value: 'big5' },
  { label: 'Shift_JIS (日文)', value: 'shift_jis' },
  { label: 'EUC-KR (韩文)', value: 'euc-kr' },
  { label: 'ISO-8859-1 (拉丁)', value: 'iso-8859-1' },
  { label: 'Windows-1252', value: 'windows-1252' },
  { label: 'UTF-16 LE', value: 'utf-16le' },
  { label: 'UTF-16 BE', value: 'utf-16be' },
]

export interface EditorTab {
  id: string
  sessionId: string
  kind: 'local' | 'ssh'
  path: string
  name: string
  encoding: string
  content: string
  /** 上次保存/加载时的内容（脏判定基准） */
  savedContent: string
  /** 原始字节：编码切换时重新解码用（保存成功后清空，避免陈旧） */
  rawBytes: Uint8Array | null
  loading: boolean
  saving: boolean
  error: string
  /** 疑似二进制（含 NUL 字节），仅提示不拦截 */
  binary: boolean
}

function decodeBytes(bytes: Uint8Array, encoding: string): string {
  try {
    return new TextDecoder(encoding).decode(bytes)
  } catch {
    return new TextDecoder('utf-8').decode(bytes)
  }
}

export const useEditorStore = defineStore('editor', () => {
  const visible = ref(false)
  const tabs = ref<EditorTab[]>([])
  const activeId = ref('')

  const activeTab = computed(() => tabs.value.find(t => t.id === activeId.value) ?? null)
  const dirtyCount = computed(() => tabs.value.filter(t => t.content !== t.savedContent).length)

  function isDirty(tab: EditorTab): boolean {
    return tab.content !== tab.savedContent
  }

  /** 打开文件（已打开则激活对应 Tab）；返回错误信息或空串 */
  async function open(sessionId: string, kind: 'local' | 'ssh', path: string, name: string): Promise<string> {
    const id = `${sessionId}:${path}`
    const existing = tabs.value.find(t => t.id === id)
    if (existing) {
      activeId.value = id
      visible.value = true
      return ''
    }
    const tab = reactive({
      id, sessionId, kind, path, name,
      encoding: 'utf-8', content: '', savedContent: '', rawBytes: null,
      loading: true, saving: false, error: '', binary: false
    }) as EditorTab
    tabs.value.push(tab)
    activeId.value = id
    visible.value = true
    try {
      const bytes = await invoke<number[]>('read_remote_file', {
        sessionId, path, maxBytes: EDITOR_MAX_BYTES
      }, { timeoutMs: READ_FILE_TIMEOUT_MS })
      const u8 = new Uint8Array(bytes)
      tab.rawBytes = u8
      // 前 8KB 含 NUL → 疑似二进制
      const probe = u8.subarray(0, Math.min(8192, u8.length))
      tab.binary = probe.includes(0)
      // UTF-8 BOM 去除
      let text = decodeBytes(u8, tab.encoding)
      if (text.charCodeAt(0) === 0xFEFF) text = text.slice(1)
      tab.content = text
      tab.savedContent = text
    } catch (e: any) {
      tab.error = friendlyError(e)
    } finally {
      tab.loading = false
    }
    return tab.error
  }

  /** 保存到文件所在服务器/本机；返回错误信息或空串 */
  async function save(tab: EditorTab): Promise<string> {
    if (tab.saving) return ''
    tab.saving = true
    tab.error = ''
    try {
      await invoke('write_remote_file', {
        sessionId: tab.sessionId,
        path: tab.path,
        content: tab.content,
        encoding: tab.encoding
      })
      tab.savedContent = tab.content
      tab.rawBytes = null
      return ''
    } catch (e: any) {
      tab.error = friendlyError(e)
      return tab.error
    } finally {
      tab.saving = false
    }
  }

  /** 切换编码：未修改时直接按原始字节重解码；有未保存修改需调用方确认后强制重载 */
  function changeEncoding(tab: EditorTab, encoding: string): boolean {
    if (!EDITOR_ENCODINGS.some(e => e.value === encoding)) return false
    tab.encoding = encoding
    if (!isDirty(tab) && tab.rawBytes) {
      let text = decodeBytes(tab.rawBytes, encoding)
      if (text.charCodeAt(0) === 0xFEFF && encoding.startsWith('utf-8')) text = text.slice(1)
      tab.content = text
      tab.savedContent = text
      return true
    }
    // 无原始字节（已保存过）或仅重设保存编码：内容不动，保存时按新编码写回
    return true
  }

  /** 重新从服务器加载（放弃未保存修改） */
  async function reload(tab: EditorTab): Promise<void> {
    tab.loading = true
    tab.error = ''
    try {
      const bytes = await invoke<number[]>('read_remote_file', {
        sessionId: tab.sessionId, path: tab.path, maxBytes: EDITOR_MAX_BYTES
      }, { timeoutMs: READ_FILE_TIMEOUT_MS })
      const u8 = new Uint8Array(bytes)
      tab.rawBytes = u8
      tab.binary = u8.subarray(0, Math.min(8192, u8.length)).includes(0)
      let text = decodeBytes(u8, tab.encoding)
      if (text.charCodeAt(0) === 0xFEFF) text = text.slice(1)
      tab.content = text
      tab.savedContent = text
    } catch (e: any) {
      tab.error = friendlyError(e)
    } finally {
      tab.loading = false
    }
  }

  /** 关闭 Tab；有未保存修改时返回 false（调用方弹确认后 forceClose） */
  function close(id: string): boolean {
    const tab = tabs.value.find(t => t.id === id)
    if (!tab) return true
    if (isDirty(tab)) return false
    forceClose(id)
    return true
  }

  function forceClose(id: string) {
    const idx = tabs.value.findIndex(t => t.id === id)
    if (idx < 0) return
    tabs.value.splice(idx, 1)
    if (activeId.value === id) {
      activeId.value = tabs.value[Math.min(idx, tabs.value.length - 1)]?.id ?? ''
    }
    if (tabs.value.length === 0) visible.value = false
  }

  return {
    visible, tabs, activeId, activeTab, dirtyCount,
    isDirty, open, save, changeEncoding, reload, close, forceClose
  }
})
