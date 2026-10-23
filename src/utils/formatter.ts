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
 * 文本格式化工具（文件编辑器一键格式化用）
 * 作者：JC
 * - JSON：JSON.parse + 缩进重排
 * - XML/HTML：内置轻量美化器（标签缩进、属性保留、文本节点折行）
 * - YAML：js-yaml 解析后重新 dump（保留注释能力有限，格式化会丢失注释，调用方需提示）
 */
import * as yaml from 'js-yaml'

/** 按扩展名判断是否支持格式化，返回格式标识 */
export function formattableKind(filename: string): 'json' | 'xml' | 'yaml' | null {
  const ext = filename.split('.').pop()?.toLowerCase() ?? ''
  if (ext === 'json' || ext === 'jsonc' || ext === 'ipynb') return 'json'
  if (['xml', 'html', 'htm', 'xhtml', 'svg', 'vue', 'xsl', 'xslt', 'wsdl', 'rss', 'atom'].includes(ext)) return 'xml'
  if (['yml', 'yaml'].includes(ext)) return 'yaml'
  return null
}

/** JSON 格式化（2 空格缩进） */
export function formatJson(text: string): string {
  const value = JSON.parse(text)
  return JSON.stringify(value, null, 2) + '\n'
}

/** YAML 格式化（解析后重排，2 空格缩进；多文档保持分文档输出） */
export function formatYaml(text: string): string {
  const docs: unknown[] = []
  yaml.loadAll(text, (d) => docs.push(d))
  return docs
    .map((d) => yaml.dump(d, { indent: 2, lineWidth: 120, noRefs: true, sortKeys: false }))
    .join('---\n')
}

const VOID_TAGS = new Set([
  'area', 'base', 'br', 'col', 'embed', 'hr', 'img', 'input', 'link', 'meta',
  'param', 'source', 'track', 'wbr', 'command', 'keygen', 'menuitem'
])

interface XmlToken { kind: 'open' | 'close' | 'selfclose' | 'text' | 'comment' | 'decl' | 'cdata' | 'doctype'; raw: string; tag?: string }

function tokenizeMarkup(src: string): XmlToken[] {
  const tokens: XmlToken[] = []
  let i = 0
  const n = src.length
  while (i < n) {
    if (src.startsWith('<!--', i)) {
      const end = src.indexOf('-->', i + 4)
      const stop = end < 0 ? n : end + 3
      tokens.push({ kind: 'comment', raw: src.slice(i, stop) })
      i = stop
    } else if (src.startsWith('<![CDATA[', i)) {
      const end = src.indexOf(']]>', i + 9)
      const stop = end < 0 ? n : end + 3
      tokens.push({ kind: 'cdata', raw: src.slice(i, stop) })
      i = stop
    } else if (src.startsWith('<!DOCTYPE', i) || src.startsWith('<!doctype', i)) {
      const end = src.indexOf('>', i + 2)
      const stop = end < 0 ? n : end + 1
      tokens.push({ kind: 'doctype', raw: src.slice(i, stop) })
      i = stop
    } else if (src.startsWith('<?', i)) {
      const end = src.indexOf('?>', i + 2)
      const stop = end < 0 ? n : end + 2
      tokens.push({ kind: 'decl', raw: src.slice(i, stop) })
      i = stop
    } else if (src[i] === '<') {
      const end = src.indexOf('>', i + 1)
      if (end < 0) { tokens.push({ kind: 'text', raw: src.slice(i) }); break }
      const raw = src.slice(i, end + 1)
      const inner = raw.slice(1, -1).trim()
      if (inner.startsWith('/')) {
        tokens.push({ kind: 'close', raw, tag: inner.slice(1).split(/\s/)[0].toLowerCase() })
      } else {
        const tag = inner.split(/[\s/>]/)[0].toLowerCase()
        const selfclose = inner.endsWith('/') || VOID_TAGS.has(tag)
        tokens.push({ kind: selfclose ? 'selfclose' : 'open', raw, tag })
      }
      i = end + 1
    } else {
      const end = src.indexOf('<', i)
      const stop = end < 0 ? n : end
      tokens.push({ kind: 'text', raw: src.slice(i, stop) })
      i = stop
    }
  }
  return tokens
}

/** XML/HTML 美化（2 空格缩进；内联文本与标签同行时保持紧凑） */
export function formatXml(text: string): string {
  const tokens = tokenizeMarkup(text)
  const out: string[] = []
  let depth = 0
  const pad = () => '  '.repeat(depth)
  for (let idx = 0; idx < tokens.length; idx++) {
    const tk = tokens[idx]
    if (tk.kind === 'text') {
      const t = tk.raw.replace(/\s+/g, ' ').trim()
      if (t) out.push(pad() + t)
      continue
    }
    if (tk.kind === 'open') {
      // <tag>text</tag> 三段紧凑同行
      const next = tokens[idx + 1]
      const after = tokens[idx + 2]
      if (next && after && next.kind === 'text' && after.kind === 'close' && after.tag === tk.tag) {
        const t = next.raw.replace(/\s+/g, ' ').trim()
        if (t && !t.includes('\n')) {
          out.push(pad() + tk.raw + t + after.raw)
          idx += 2
          continue
        }
      }
      out.push(pad() + tk.raw)
      depth++
    } else if (tk.kind === 'close') {
      depth = Math.max(0, depth - 1)
      out.push(pad() + tk.raw)
    } else {
      out.push(pad() + tk.raw)
    }
  }
  return out.join('\n') + '\n'
}

/** 按文件类型一键格式化；不支持/解析失败时抛错（message 面向用户） */
export function formatByFilename(filename: string, text: string): string {
  const kind = formattableKind(filename)
  if (!kind) throw new Error('该文件类型不支持自动格式化（支持 json/xml/html/yml/yaml 等）')
  try {
    if (kind === 'json') return formatJson(text)
    if (kind === 'xml') return formatXml(text)
    return formatYaml(text)
  } catch (e: any) {
    if (kind === 'yaml') throw new Error('YAML 解析失败：' + (e?.reason || e?.message || '语法错误'))
    if (kind === 'json') throw new Error('JSON 解析失败：' + (e?.message || '语法错误'))
    throw e
  }
}
