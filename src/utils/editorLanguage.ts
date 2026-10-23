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
 * 编辑器语言识别（按文件名 / 后缀）
 * 作者：JC
 *
 * 抽成独立模块：CodeEditor（决定语法高亮语言）与 FileEditorOverlay（「文件格式」下拉的
 * 自动识别展示）必须共用同一份「后缀 → 语言」判定，避免两处各写一套导致下拉显示与高亮不一致。
 */
import { LanguageDescription } from '@codemirror/language'
import { languages } from '@codemirror/language-data'

/** CM6 @codemirror/language-data 没覆盖但常见的配置/数据格式 →
 *  复用最接近的内置语言做兜底高亮（toml/ini/conf → yaml；env/gitignore → shell）。 */
export const EXT_LANG_ALIAS: Record<string, string> = {
  toml: 'yaml',
  ini: 'yaml',
  conf: 'yaml',
  cfg: 'yaml',
  env: 'bash',
  properties: 'yaml',
  log: 'yaml',
  csv: 'markdown',
  tsv: 'markdown',
  list: 'markdown',
  gitignore: 'bash',
  htaccess: 'apache',
  nginxconf: 'nginx',
}

/** 由 LanguageDescription.name 查找指定语言（大小写不敏感、兼容 alias） */
export function findLanguageByName(name: string): LanguageDescription | null {
  if (!name) return null
  const lower = name.toLowerCase()
  for (const d of languages) {
    if (d.name.toLowerCase() === lower) return d
    if (d.alias && d.alias.some(a => a.toLowerCase() === lower)) return d
  }
  return null
}

/** 按文件名后缀推断语言描述；识别不出返回 null（按纯文本处理）。
 *  优先级：别名单映射（toml/ini/env…）→ CM6 language-data 内置的扩展名/特殊文件名匹配。 */
export function detectLanguageDescription(filename: string): LanguageDescription | null {
  if (!filename) return null
  const lower = filename.toLowerCase()
  const dot = lower.lastIndexOf('.')
  const ext = dot >= 0 ? lower.slice(dot + 1) : ''
  if (ext && EXT_LANG_ALIAS[ext]) {
    const alias = lower.slice(0, dot + 1) + EXT_LANG_ALIAS[ext]
    const aliasDesc = LanguageDescription.matchFilename(languages, alias)
    if (aliasDesc) return aliasDesc
  }
  // 特殊文件名（无扩展名，如 Dockerfile、Makefile）由 matchFilename 内置覆盖
  return LanguageDescription.matchFilename(languages, filename)
}

/** 由文件名推断默认语言名（给「文件格式」下拉/状态栏展示用）；识别不出返回空串 */
export function detectLanguageName(filename: string): string {
  return detectLanguageDescription(filename)?.name ?? ''
}
