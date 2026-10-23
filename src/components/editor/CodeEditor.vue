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
  <div ref="hostRef" class="code-editor-host h-full w-full overflow-hidden"></div>
</template>

<script setup lang="ts">
/**
 * CodeMirror 6 封装（文件编辑器核心）
 * 作者：JC
 * 功能：语法高亮（language-data 按扩展名识别）、行号/自动换行开关（Compartment 动态重配置）、
 * 查找替换（@codemirror/search 面板）、可配置快捷键（保存/查找/替换/格式化，来自全局设置）。
 */
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue'
import { EditorView, keymap, lineNumbers, highlightActiveLine, highlightActiveLineGutter, drawSelection, rectangularSelection, highlightSpecialChars, placeholder } from '@codemirror/view'
import { EditorState, Compartment, Prec } from '@codemirror/state'
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands'
import { searchKeymap, openSearchPanel, highlightSelectionMatches } from '@codemirror/search'
import { bracketMatching, indentOnInput, syntaxHighlighting, foldGutter, codeFolding, HighlightStyle } from '@codemirror/language'
import { closeBrackets, closeBracketsKeymap, autocompletion, completionKeymap } from '@codemirror/autocomplete'
import { tags } from '@lezer/highlight'
import { useI18nStore } from '@/stores/i18n'
import { useAppStore } from '@/stores/app'
import { findLanguageByName, detectLanguageDescription, detectLanguageName } from '@/utils/editorLanguage'

interface Props {
  value: string
  filename: string
  wordWrap: boolean
  showLineNumbers: boolean
  /** 可配置快捷键（设置页编辑器分组）：Ctrl+S 形式 */
  shortcuts: Record<string, string>
  /** 强制指定语言名（按 LanguageDescription.name，如 "JSON" / "JavaScript"）；为空走自动检测 */
  forcedLanguage?: string
}
const props = defineProps<Props>()
const emit = defineEmits<{
  (e: 'update:value', v: string): void
  (e: 'save'): void
  (e: 'format'): void
  (e: 'cursor', pos: { line: number; col: number }): void
}>()

const hostRef = ref<HTMLElement | null>(null)
let view: EditorView | null = null
/** 外部 setValue 期间抑制 update:value 回环 */
let applyingExternal = false

const wrapComp = new Compartment()
const gutterComp = new Compartment()
const langComp = new Compartment()
const keysComp = new Compartment()
/** CM6 搜索/替换面板内置文案（绑定到 phrases facet，按用户语言切换） */
const phrasesComp = new Compartment()

/** 当前是否深色：跟随应用主题（浅色/深色/护眼），用于 CodeMirror 的 dark 标记 */
const appStore = useAppStore()
const isDarkEditor = computed(() => appStore.effectiveTheme === 'dark')
/** 主题用 Compartment 承载：应用主题切换时可动态重配置 */
const themeComp = new Compartment()

/** 编辑器主题规格：颜色全部走 --jc-* 变量（浅色/深色/护眼自动跟随，不写死）。
 *  注意：CodeMirror 基础主题用 background-image 渐变画面板按钮/输入框，只改 background-color 盖不住，
 *  必须显式 backgroundImage:'none' —— 否则浅色主题下搜索/替换按钮会露出深色渐变（表现为一坨黑块）。 */
const editorThemeSpec: Parameters<typeof EditorView.theme>[0] = {
  '&': { height: '100%', fontSize: '13px', backgroundColor: 'var(--jc-bg)', color: 'var(--jc-text)' },
  '.cm-scroller': { fontFamily: "'JetBrains Mono', Consolas, 'Courier New', monospace", lineHeight: '1.5' },
  '.cm-content': { caretColor: 'var(--jc-accent)', padding: '4px 0' },
  '.cm-cursor, .cm-dropCursor': { borderLeftColor: 'var(--jc-accent)' },
  '&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection': { backgroundColor: 'rgba(59,130,246,0.28) !important' },
  '.cm-activeLine': { backgroundColor: 'rgba(148,163,184,0.08)' },
  '.cm-activeLineGutter': { backgroundColor: 'rgba(148,163,184,0.12)' },
  '.cm-gutters': { backgroundColor: 'var(--jc-bg-soft)', backgroundImage: 'none', color: 'var(--jc-text-muted)', border: 'none', borderRight: '1px solid var(--jc-border)' },
  '.cm-foldGutter span': { cursor: 'pointer' },
  '.cm-searchMatch': { backgroundColor: 'rgba(245,158,11,0.30)', outline: '1px solid rgba(245,158,11,0.4)' },
  '.cm-searchMatch-selected': { backgroundColor: 'rgba(245,158,11,0.55)' },
  // 搜索/替换面板：放大字号、内边距与按钮尺寸，便于阅读
  '.cm-panels': { backgroundColor: 'var(--jc-bg-soft)', backgroundImage: 'none', color: 'var(--jc-text)', borderBottom: '1px solid var(--jc-border)', padding: '6px 8px', fontSize: '13px' },
  '.cm-panels.cm-search': { padding: '8px 10px' },
  '.cm-panels input, .cm-panels button, .cm-panels label': { color: 'var(--jc-text)', fontSize: '13px' },
  '.cm-panels input': { backgroundColor: 'var(--jc-bg)', backgroundImage: 'none', border: '1px solid var(--jc-border)', borderRadius: '4px', padding: '4px 8px', height: '28px', lineHeight: '20px', minWidth: '180px' },
  '.cm-panels input[type=checkbox]': { minWidth: 'auto', width: '14px', height: '14px', padding: '0', margin: '0 6px 0 0' },
  // 关键：清掉基础主题的按钮渐变，颜色跟随主题变量（浅色/深色都不再出现黑块）
  '.cm-panels button': { backgroundColor: 'var(--jc-bg-hover)', backgroundImage: 'none', boxShadow: 'none', border: '1px solid var(--jc-border)', borderRadius: '4px', cursor: 'pointer', padding: '4px 10px', height: '28px', lineHeight: '18px', margin: '0 4px' },
  '.cm-panels button:hover, .cm-panels button:focus': { backgroundColor: 'var(--jc-accent)', backgroundImage: 'none', boxShadow: 'none', color: '#fff', borderColor: 'var(--jc-accent)' },
  '.cm-panels button:active': { backgroundImage: 'none', boxShadow: 'none' },
  '.cm-panels label': { display: 'inline-flex', alignItems: 'center', margin: '0 10px 0 4px', userSelect: 'none', cursor: 'pointer' },
  '.cm-panels [name=close]': { fontSize: '16px', width: '28px', height: '28px', borderRadius: '4px', backgroundImage: 'none' },
  '.cm-tooltip': { backgroundColor: 'var(--jc-bg-soft)', backgroundImage: 'none', border: '1px solid var(--jc-border)', color: 'var(--jc-text)' },
  '.cm-tooltip-autocomplete ul li[aria-selected]': { backgroundColor: 'rgba(59,130,246,0.3)' },
}

/** 构建主题扩展：dark 标记跟随当前应用主题 */
function buildEditorTheme() {
  return EditorView.theme(editorThemeSpec, { dark: isDarkEditor.value })
}

/** 语法高亮配色（暗色主题） */
const darkHighlightStyle = HighlightStyle.define([
  { tag: tags.keyword, color: '#c792ea' },
  { tag: [tags.string, tags.special(tags.string)], color: '#a5d6a7' },
  { tag: [tags.number, tags.bool, tags.null], color: '#f78c6c' },
  { tag: [tags.propertyName, tags.attributeName], color: '#82aaff' },
  { tag: [tags.variableName], color: 'var(--jc-text)' },
  { tag: [tags.function(tags.variableName), tags.function(tags.propertyName)], color: '#ffcb6b' },
  { tag: [tags.typeName, tags.className, tags.tagName], color: '#4ec9b0' },
  { tag: tags.comment, color: '#6b7280', fontStyle: 'italic' },
  { tag: [tags.operator, tags.punctuation], color: '#9ca3af' },
  { tag: [tags.heading, tags.strong], color: '#82aaff', fontWeight: 'bold' },
  { tag: tags.emphasis, fontStyle: 'italic' },
  { tag: tags.link, color: '#60a5fa', textDecoration: 'underline' },
  { tag: tags.invalid, color: '#f87171' },
])

/** 语法高亮配色（浅色 / 护眼主题）：用深色系令牌，保证浅底上的对比度
 *  （暗色那套的浅绿/浅蓝在浅底上几乎看不清，必须按主题切换，不能共用一套写死配色） */
const lightHighlightStyle = HighlightStyle.define([
  { tag: tags.keyword, color: '#a626a4' },
  { tag: [tags.string, tags.special(tags.string)], color: '#3f8f3f' },
  { tag: [tags.number, tags.bool, tags.null], color: '#986801' },
  { tag: [tags.propertyName, tags.attributeName], color: '#2f6fd0' },
  { tag: [tags.variableName], color: 'var(--jc-text)' },
  { tag: [tags.function(tags.variableName), tags.function(tags.propertyName)], color: '#b06f00' },
  { tag: [tags.typeName, tags.className, tags.tagName], color: '#017a9b' },
  { tag: tags.comment, color: '#8a8a8e', fontStyle: 'italic' },
  { tag: [tags.operator, tags.punctuation], color: '#5a5a5f' },
  { tag: [tags.heading, tags.strong], color: '#2f6fd0', fontWeight: 'bold' },
  { tag: tags.emphasis, fontStyle: 'italic' },
  { tag: tags.link, color: '#2f6fd0', textDecoration: 'underline' },
  { tag: tags.invalid, color: '#d13b30' },
])

/** 语法高亮用 Compartment 承载：随应用主题在深/浅两套配色间切换 */
const highlightComp = new Compartment()

/** "Ctrl+Shift+F" → CodeMirror 键位 "Mod-Shift-f" */
function comboToCmKey(combo: string): string | null {
  if (!combo) return null
  const parts = combo.split('+').map(s => s.trim()).filter(Boolean)
  if (parts.length === 0) return null
  let key = parts[parts.length - 1]
  const mods = parts.slice(0, -1)
  const cmMods: string[] = []
  if (mods.includes('Ctrl') || mods.includes('Meta')) cmMods.push('Mod')
  if (mods.includes('Alt')) cmMods.push('Alt')
  if (mods.includes('Shift')) cmMods.push('Shift')
  if (key.length === 1) key = key.toLowerCase()
  return [...cmMods, key].join('-')
}

/** 由设置构建编辑器快捷键 keymap（优先于默认键位） */
function buildKeymap(sc: Record<string, string>) {
  const entries: { key: string; run: () => boolean }[] = []
  const kSave = comboToCmKey(sc.editorSave)
  const kFind = comboToCmKey(sc.editorFind)
  const kReplace = comboToCmKey(sc.editorReplace)
  const kFormat = comboToCmKey(sc.editorFormat)
  if (kSave) entries.push({ key: kSave, run: () => { emit('save'); return true } })
  if (kFind) entries.push({ key: kFind, run: () => { openSearch(); return true } })
  if (kReplace) entries.push({ key: kReplace, run: () => { openReplace(); return true } })
  if (kFormat) entries.push({ key: kFormat, run: () => { emit('format'); return true } })
  return Prec.high(keymap.of(entries))
}

async function resolveLanguage(filename: string, forcedName?: string) {
  // 0) 强制指定优先：按 LanguageDescription.name / alias 精确匹配
  if (forcedName) {
    const d = findLanguageByName(forcedName)
    if (d) {
      try { return [await d.load()] } catch { /* fallthrough to auto */ }
    }
  }
  // 1) 按文件名/后缀自动识别（含 toml/ini/env 等别名兜底与特殊文件名）
  const desc = detectLanguageDescription(filename)
  if (!desc) return []
  try {
    return [await desc.load()]
  } catch {
    return []
  }
}

/** CodeMirror 6 搜索/替换面板内置英文文案 → i18n key 映射（已迁移到 cm.search.*） */
const CM_PHRASE_KEYS: { en: string; key: string }[] = [
  // 输入框占位/无障碍标签用的是首字母大写的 "Find" / "Replace"，与按钮的小写 key 不同，需单独映射
  { en: 'Find', key: 'cm.search.find' },
  { en: 'Replace', key: 'cm.search.replace' },
  { en: 'next', key: 'cm.search.next' },
  { en: 'previous', key: 'cm.search.previous' },
  { en: 'all', key: 'cm.search.all' },
  { en: 'match case', key: 'cm.search.matchCase' },
  { en: 'regexp', key: 'cm.search.regexp' },
  { en: 'by word', key: 'cm.search.byWord' },
  { en: 'replace', key: 'cm.search.replace' },
  { en: 'replace all', key: 'cm.search.replaceAll' },
  { en: 'close', key: 'cm.search.close' },
  { en: 'current match', key: 'cm.search.currentMatch' },
  { en: 'on line', key: 'cm.search.onLine' },
  { en: 'find next?', key: 'cm.search.next?' },
  { en: 'find previous?', key: 'cm.search.previous?' },
  { en: 'replace?', key: 'cm.search.replaceNext?' },
  { en: 'replace all?', key: 'cm.search.replaceAll?' },
]

/** 从当前 i18n 字典构造 phrases facet 内容（CM6 用 view.state.phrase(key) 查表） */
function buildPhrases(): Record<string, string> {
  const t = useI18nStore().t
  const out: Record<string, string> = {}
  for (const { en, key } of CM_PHRASE_KEYS) {
    // CM6 默认面板用英文 key 查表，故这里以英文为键，中文为值；
    // 若当前语言为英文则保留英文原文，避免重复
    const v = t(key)
    out[en] = v === key ? en : v
  }
  return out
}

function currentDoc(): string {
  return view ? view.state.doc.toString() : ''
}

function emitCursor() {
  if (!view) return
  const head = view.state.selection.main.head
  const line = view.state.doc.lineAt(head)
  emit('cursor', { line: line.number, col: head - line.from + 1 })
}

function openSearch() {
  if (!view) return
  openSearchPanel(view)
  view.focus()
}

/** 打开查找面板并展开替换行（CM6 查找/替换同一面板，替换默认折叠） */
function openReplace() {
  if (!view) return
  openSearchPanel(view)
  view.focus()
  requestAnimationFrame(() => {
    const panel = hostRef.value?.querySelector('.cm-panel.cm-search')
    const toggle = panel?.querySelector('button[name="toggleReplace"]') as HTMLButtonElement | null
    if (toggle && toggle.getAttribute('aria-expanded') !== 'true') toggle.click()
    const inputs = panel?.querySelectorAll('input')
    // 聚焦替换输入框（第二个）
    if (inputs && inputs.length > 1) (inputs[1] as HTMLInputElement).focus()
  })
}

onMounted(async () => {
  if (!hostRef.value) return
  const lang = await resolveLanguage(props.filename, props.forcedLanguage)
  view = new EditorView({
    parent: hostRef.value,
    state: EditorState.create({
      doc: props.value,
      extensions: [
        keysComp.of(buildKeymap(props.shortcuts)),
        highlightSpecialChars(),
        history(),
        drawSelection(),
        rectangularSelection(),
        EditorState.allowMultipleSelections.of(true),
        indentOnInput(),
        bracketMatching(),
        closeBrackets(),
        autocompletion(),
        gutterComp.of(props.showLineNumbers ? [lineNumbers(), highlightActiveLineGutter(), codeFolding(), foldGutter()] : []),
        highlightActiveLine(),
        highlightSelectionMatches(),
        wrapComp.of(props.wordWrap ? EditorView.lineWrapping : []),
        langComp.of(lang),
        // CM6 搜索/替换面板文案（按用户语言切换，phrase facet）
        phrasesComp.of(EditorState.phrases.of(buildPhrases())),
        highlightComp.of(syntaxHighlighting(isDarkEditor.value ? darkHighlightStyle : lightHighlightStyle, { fallback: true })),
        themeComp.of(buildEditorTheme()),
        placeholder(''),
        EditorView.updateListener.of((u) => {
          if (u.docChanged && !applyingExternal) emit('update:value', u.state.doc.toString())
          if (u.selectionSet || u.docChanged) emitCursor()
        }),
        keymap.of([
          ...closeBracketsKeymap,
          ...defaultKeymap,
          ...searchKeymap,
          ...historyKeymap,
          ...completionKeymap,
          indentWithTab,
        ]),
      ],
    }),
  })
  emitCursor()
})

watch(() => props.value, (v) => {
  if (!view) return
  if (v === currentDoc()) return
  applyingExternal = true
  try {
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: v } })
  } finally {
    applyingExternal = false
  }
})

watch(() => props.wordWrap, (on) => {
  view?.dispatch({ effects: wrapComp.reconfigure(on ? EditorView.lineWrapping : []) })
})
watch(() => props.showLineNumbers, (on) => {
  view?.dispatch({ effects: gutterComp.reconfigure(on ? [lineNumbers(), highlightActiveLineGutter(), codeFolding(), foldGutter()] : []) })
})
watch(() => props.shortcuts, (sc) => {
  view?.dispatch({ effects: keysComp.reconfigure(buildKeymap(sc)) })
}, { deep: true })
watch(() => props.filename, async (name) => {
  const lang = await resolveLanguage(name, props.forcedLanguage)
  view?.dispatch({ effects: langComp.reconfigure(lang) })
})
/** 工具栏"文件格式"下拉切换：重新解析高亮语言 */
watch(() => props.forcedLanguage, async (forced) => {
  const lang = await resolveLanguage(props.filename, forced)
  view?.dispatch({ effects: langComp.reconfigure(lang) })
})
/** 用户在头像菜单切换语言时，实时刷新搜索面板文案 */
watch(() => useI18nStore().locale, () => {
  view?.dispatch({ effects: phrasesComp.reconfigure(EditorState.phrases.of(buildPhrases())) })
})
/** 应用主题切换（浅色/深色/护眼）时：重配置 CodeMirror 的 dark 标记（避免浅色下露出深色基础样式）
 *  并切换语法高亮配色（浅底用深色系令牌，保证对比度） */
watch(isDarkEditor, (dark) => {
  view?.dispatch({
    effects: [
      themeComp.reconfigure(EditorView.theme(editorThemeSpec, { dark })),
      highlightComp.reconfigure(syntaxHighlighting(dark ? darkHighlightStyle : lightHighlightStyle, { fallback: true })),
    ]
  })
})

onBeforeUnmount(() => {
  view?.destroy()
  view = null
})

defineExpose({ openSearch, openReplace, focus: () => view?.focus(), detectLanguageName })
</script>

<style scoped>
.code-editor-host :deep(.cm-editor) { height: 100%; }
</style>
