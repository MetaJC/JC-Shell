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
  <Teleport to="body">
    <div v-if="store.visible" class="editor-overlay fixed inset-0 z-[1200] flex flex-col bg-[var(--jc-bg)]">
      <!-- 标题栏：应用级编辑器窗口 -->
      <div class="shrink-0 h-9 flex items-center gap-2 px-3 border-b border-[var(--jc-border)] bg-[var(--jc-bg-soft)] select-none">
        <el-icon :size="15" class="text-blue-400"><EditPen /></el-icon>
        <span class="text-xs font-medium text-[var(--jc-text)]">文件编辑器</span>
        <span v-if="store.dirtyCount > 0" class="text-[10px] text-amber-400">{{ store.dirtyCount }} 个文件未保存</span>
        <div class="flex-1"></div>
        <button class="ed-win-btn" title="关闭编辑器" @click="closeOverlay"><el-icon :size="14"><Close /></el-icon></button>
      </div>

      <!-- Tab 栏 -->
      <div class="ed-tabs shrink-0 flex items-end gap-0.5 px-2 pt-1 border-b border-[var(--jc-border)] bg-[var(--jc-bg-soft)] overflow-x-auto scrollbar-hide">
        <div
          v-for="tab in store.tabs"
          :key="tab.id"
          class="ed-tab group flex items-center gap-1.5 px-3 h-7 rounded-t text-xs cursor-pointer shrink-0 border border-b-0 border-transparent max-w-[220px]"
          :class="tab.id === store.activeId
            ? 'bg-[var(--jc-bg)] text-[var(--jc-text)] border-[var(--jc-border)]'
            : 'text-[var(--jc-text-soft)] hover:bg-[var(--jc-bg-hover)]'"
          :title="tab.path"
          @click="store.activeId = tab.id"
          @mousedown.middle.prevent="requestCloseTab(tab.id)"
        >
          <span class="w-1.5 h-1.5 rounded-full shrink-0" :class="store.isDirty(tab) ? 'bg-amber-400' : 'bg-transparent'"></span>
          <span class="truncate">{{ tab.name }}</span>
          <button
            class="ed-tab-close shrink-0 w-4 h-4 rounded flex items-center justify-center opacity-0 group-hover:opacity-100 hover:bg-white/15"
            title="关闭文件"
            @click.stop="requestCloseTab(tab.id)"
          ><el-icon :size="10"><Close /></el-icon></button>
        </div>
      </div>

      <!-- 工具栏 -->
      <div v-if="tab" class="ed-toolbar shrink-0 h-9 flex items-center gap-1 px-2 border-b border-[var(--jc-border)] bg-[var(--jc-bg)] text-xs">
        <button class="ed-btn" :disabled="!store.isDirty(tab) || tab.saving" :title="'保存 (' + sc.editorSave + ')'" @click="saveActive">
          <el-icon :size="13"><DocumentChecked /></el-icon><span>{{ tab.saving ? '保存中…' : '保存' }}</span>
        </button>
        <button class="ed-btn" :disabled="!canFormat" :title="canFormat ? '一键格式化 (' + sc.editorFormat + ')' : '该文件类型不支持格式化'" @click="formatActive">
          <el-icon :size="13"><MagicStick /></el-icon><span>格式化</span>
        </button>
        <span class="w-px h-4 bg-[var(--jc-border)] mx-1"></span>
        <button class="ed-btn" :title="'查找 (' + sc.editorFind + ')'" @click="editorRef?.openSearch()">
          <el-icon :size="13"><Search /></el-icon><span>查找</span>
        </button>
        <button class="ed-btn" :title="'替换 (' + sc.editorReplace + ')'" @click="editorRef?.openReplace()">
          <el-icon :size="13"><Switch /></el-icon><span>替换</span>
        </button>
        <span class="w-px h-4 bg-[var(--jc-border)] mx-1"></span>
        <button class="ed-btn" :class="{ 'ed-btn-on': wordWrap }" title="自动换行" @click="wordWrap = !wordWrap">
          <el-icon :size="13"><Fold /></el-icon><span>换行</span>
        </button>
        <button class="ed-btn" :class="{ 'ed-btn-on': showLineNumbers }" title="显示行号" @click="showLineNumbers = !showLineNumbers">
          <el-icon :size="13"><List /></el-icon><span>行号</span>
        </button>
        <span class="w-px h-4 bg-[var(--jc-border)] mx-1"></span>
        <el-select
          :model-value="forcedLanguage"
          size="small"
          filterable
          :filter-method="onLangFilter"
          :placeholder="t('editor.language.label')"
          style="width: 180px"
          :title="t('editor.language.label')"
          @change="(v: string) => onForcedLanguageChange(v)"
        >
          <el-option
            :key="LANG_AUTO"
            :label="autoLanguageLabel"
            :value="LANG_AUTO"
          />
          <el-option
            v-for="lang in languageOptions"
            :key="lang.value"
            :label="lang.label"
            :value="lang.value"
          />
        </el-select>
        <el-select
          :model-value="tab.encoding"
          size="small"
          style="width: 150px"
          title="文件编码"
          @change="(v: string) => onEncodingChange(v)"
        >
          <el-option v-for="enc in EDITOR_ENCODINGS" :key="enc.value" :label="enc.label" :value="enc.value" />
        </el-select>
        <button class="ed-btn" title="从服务器重新加载（放弃未保存修改）" @click="reloadActive">
          <el-icon :size="13"><Refresh /></el-icon><span>重载</span>
        </button>
      </div>

      <!-- 编辑区 -->
      <div class="flex-1 min-h-0 relative">
        <div v-if="!tab" class="h-full flex items-center justify-center text-[var(--jc-text-muted)] text-sm">无打开的文件</div>
        <template v-else>
          <div v-if="tab.loading" class="h-full flex items-center justify-center text-[var(--jc-text-muted)] text-sm">正在读取远程文件…</div>
          <div v-else-if="tab.error && !tab.content" class="h-full flex flex-col items-center justify-center gap-2 text-sm">
            <span class="text-red-400">读取失败：{{ tab.error }}</span>
          </div>
          <template v-else>
            <div v-if="tab.binary" class="absolute top-0 inset-x-0 z-10 px-3 py-1 text-[11px] bg-amber-500/15 text-amber-400 border-b border-amber-500/30">
              该文件疑似二进制文件，编辑并保存可能导致文件损坏
            </div>
            <CodeEditor
              ref="editorRef"
              :value="tab.content"
              :filename="tab.name"
              :word-wrap="wordWrap"
              :show-line-numbers="showLineNumbers"
              :shortcuts="sc"
              :forced-language="forcedLanguage === LANG_AUTO ? '' : forcedLanguage"
              @update:value="(v: string) => { if (tab) tab.content = v }"
              @save="saveActive"
              @format="formatActive"
              @cursor="onCursor"
            />
          </template>
        </template>
      </div>

      <!-- 状态栏 -->
      <div v-if="tab" class="shrink-0 h-6 px-3 flex items-center gap-4 text-[10px] text-[var(--jc-text-muted)] border-t border-[var(--jc-border)] bg-[var(--jc-bg-soft)]">
        <span class="truncate flex-1" :title="tab.path">{{ tab.path }}</span>
        <span v-if="tab.error" class="text-red-400 truncate max-w-[300px]" :title="tab.error">{{ tab.error }}</span>
        <span>行 {{ cursor.line }}, 列 {{ cursor.col }}</span>
        <span v-if="detectedLanguage">语言 {{ detectedLanguage }}</span>
        <span>{{ encodingLabel }}</span>
        <span :class="store.isDirty(tab) ? 'text-amber-400' : 'text-green-500'">{{ store.isDirty(tab) ? '未保存' : '已保存' }}</span>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * 文件编辑器窗口（全屏 Overlay，多 Tab）
 * 作者：JC
 * 保存直接写回文件所在服务器/本机（write_remote_file），实现"本地编辑远程文件"。
 * 快捷键（保存/查找/替换/格式化）在软件设置 → 快捷键 → 编辑器分组中配置。
 */
import { ref, computed, watch } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { handleApiError, friendlyText } from '@/utils/errorHandler'
import { EditPen, Close, DocumentChecked, MagicStick, Search, Switch, Fold, List, Refresh } from '@element-plus/icons-vue'
import CodeEditor from './CodeEditor.vue'
import { useEditorStore, EDITOR_ENCODINGS } from '@/stores/editor'
import { useSettingsStore } from '@/stores/settings'
import { useI18nStore } from '@/stores/i18n'
import { languages } from '@codemirror/language-data'
import { formatByFilename, formattableKind } from '@/utils/formatter'
import { detectLanguageName } from '@/utils/editorLanguage'

const store = useEditorStore()
const settingsStore = useSettingsStore()
const t = useI18nStore().t

const editorRef = ref<InstanceType<typeof CodeEditor> | null>(null)
const cursor = ref({ line: 1, col: 1 })

/** 编辑器显示偏好（全局，本地持久化） */
const wordWrap = ref(localStorage.getItem('jc_editor_wrap') !== '0')
const showLineNumbers = ref(localStorage.getItem('jc_editor_lineno') !== '0')
watch(wordWrap, v => localStorage.setItem('jc_editor_wrap', v ? '1' : '0'))
watch(showLineNumbers, v => localStorage.setItem('jc_editor_lineno', v ? '1' : '0'))

const tab = computed(() => store.activeTab)
const sc = computed(() => settingsStore.settings.shortcuts)

const canFormat = computed(() => !!tab.value && !!formattableKind(tab.value.name))
const encodingLabel = computed(() => EDITOR_ENCODINGS.find(e => e.value === tab.value?.encoding)?.label ?? tab.value?.encoding ?? '')

/** "文件格式"下拉：自动检测哨兵值（空串 / 特殊标记） */
const LANG_AUTO = '__auto__'
/** 用户当前选择的语言；切换 Tab 时重置为自动检测 */
const forcedLanguage = ref<string>(LANG_AUTO)
/** 当前文件按后缀自动识别出的语言名（供下拉「自动」项与状态栏展示；识别不出为空串） */
const detectedLanguage = computed(() => (tab.value ? detectLanguageName(tab.value.name) : ''))
/** 「自动」项标签：识别出语言时附带显示，让用户直观看到"按后缀识别成了什么" */
const autoLanguageLabel = computed(() =>
  detectedLanguage.value ? `${t('editor.language.auto')}（${detectedLanguage.value}）` : t('editor.language.auto')
)
/** 全部语言元数据（一次性构建，按显示名排序） */
const allLangOptions = [...languages]
  .map(d => ({
    name: d.name,
    ext: d.extensions ? d.extensions.slice(0, 4).join(', ') : '',
    alias: d.alias ? d.alias.slice(0, 2).join(', ') : '',
  }))
  .sort((a, b) => a.name.localeCompare(b.name))
/** 搜索框当前关键词（驱动 el-select 自定义过滤） */
const langFilter = ref('')
/** 全部选项的 label 形式（受 langFilter 影响；空字符串展示全部） */
const languageOptions = computed(() => {
  const q = langFilter.value.trim().toLowerCase()
  const base = allLangOptions.map(o => ({
    value: o.name,
    label: o.ext ? `${o.name}  (${o.ext})` : o.name,
    keywords: `${o.name} ${o.ext} ${o.alias}`.toLowerCase(),
  }))
  if (!q) return base
  return base.filter(o => o.keywords.includes(q))
})
/** el-select filter-method 回调：把查询写到 langFilter 触发 computed 重算 */
function onLangFilter(query: string) {
  langFilter.value = query ?? ''
}

function onForcedLanguageChange(v: string) {
  forcedLanguage.value = v || LANG_AUTO
}

/** 切换 Tab 时重置下拉为自动检测（让用户在新文件重新选择） */
watch(() => store.activeId, () => {
  forcedLanguage.value = LANG_AUTO
  langFilter.value = ''
})

function onCursor(pos: { line: number; col: number }) {
  cursor.value = pos
}

async function saveActive() {
  const t = tab.value
  if (!t || t.loading) return
  const err = await store.save(t)
  if (err) handleApiError(err, 'components/FileEditorOverlay.saveActive', { prefix: '保存失败' })
  else ElMessage.success(`已保存到${t.kind === 'local' ? '本机' : '服务器'}`)
}

function formatActive() {
  const t = tab.value
  if (!t) return
  try {
    const formatted = formatByFilename(t.name, t.content)
    if (formatted === t.content) {
      ElMessage.info('内容已是规范格式')
      return
    }
    if (formattableKind(t.name) === 'yaml') ElMessage.warning('YAML 格式化会丢失注释，请确认后保存')
    t.content = formatted
  } catch (e: any) {
    handleApiError(e, 'components/FileEditorOverlay.formatActive', { prefix: '格式化失败' })
  }
}

async function onEncodingChange(encoding: string) {
  const t = tab.value
  if (!t) return
  if (store.isDirty(t) && t.rawBytes) {
    // 有未保存修改：重解码会覆盖修改，提示选择
    try {
      await ElMessageBox.confirm(
        '切换编码将按新编码重新解析文件内容，未保存的修改会丢失。也可以先保存再切换。',
        '切换编码',
        { confirmButtonText: '放弃修改并切换', cancelButtonText: '仅改保存编码', type: 'warning', distinguishCancelAndClose: true }
      )
      // 确认：按原始字节重解码（changeEncoding 会用 rawBytes 重解码并复位脏标记基准）
      store.changeEncoding(t, encoding)
      t.savedContent = t.content
    } catch (action) {
      if (action === 'cancel') {
        // 仅改保存编码（内容不动）
        t.encoding = encoding
      }
      // close：不做任何事
    }
    return
  }
  store.changeEncoding(t, encoding)
}

async function reloadActive() {
  const t = tab.value
  if (!t) return
  if (store.isDirty(t)) {
    try {
      await ElMessageBox.confirm('重新加载将放弃未保存的修改，确定？', '重新加载', { type: 'warning' })
    } catch { return }
  }
  await store.reload(t)
}

async function requestCloseTab(id: string) {
  const t = store.tabs.find(x => x.id === id)
  if (!t) return
  if (store.isDirty(t)) {
    try {
      const action = await ElMessageBox.confirm(`「${t.name}」有未保存的修改，是否保存？`, '关闭文件', {
        confirmButtonText: '保存并关闭', cancelButtonText: '不保存', type: 'warning', distinguishCancelAndClose: true
      })
      void action
      const err = await store.save(t)
      if (err) {
        handleApiError(err, 'components/FileEditorOverlay.closeWithSave', { prefix: '保存失败' })
        return
      }
      store.forceClose(id)
    } catch (action) {
      if (action === 'cancel') store.forceClose(id)
      // close：中止
    }
    return
  }
  store.forceClose(id)
}

async function closeOverlay() {
  const dirty = store.tabs.filter(t => store.isDirty(t))
  if (dirty.length > 0) {
    try {
      await ElMessageBox.confirm(
        `有 ${dirty.length} 个文件未保存（${dirty.map(t => t.name).join('、')}），确定全部关闭？`,
        '关闭编辑器',
        { confirmButtonText: '全部丢弃并关闭', cancelButtonText: '继续编辑', type: 'warning' }
      )
    } catch { return }
    dirty.forEach(t => store.forceClose(t.id))
  }
  store.visible = false
}
</script>

<style scoped>
.scrollbar-hide::-webkit-scrollbar { display: none; }
.scrollbar-hide { -ms-overflow-style: none; scrollbar-width: none; }
.ed-btn {
  display: inline-flex; align-items: center; gap: 4px;
  height: 26px; padding: 0 8px; border-radius: 4px;
  color: var(--jc-text-soft); transition: background .15s, color .15s;
}
.ed-btn:hover:not(:disabled) { background: var(--jc-bg-hover); color: var(--jc-text); }
.ed-btn:disabled { opacity: .4; cursor: not-allowed; }
.ed-btn-on { background: rgba(59, 130, 246, .2); color: #60a5fa; }
.ed-win-btn {
  display: flex; align-items: center; justify-content: center;
  width: 24px; height: 24px; border-radius: 4px;
  color: var(--jc-text-soft); transition: background .15s;
}
.ed-win-btn:hover { background: rgba(239, 68, 68, .25); color: #f87171; }
</style>
