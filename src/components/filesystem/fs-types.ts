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
 * 文件系统浏览器共享类型
 * @author JC
 */

export interface FileNode {
  name: string
  type: 'dir' | 'file' | 'symlink'
  size: number
  modified: string
  permissions: string
  owner: string
  group: string
  children?: FileNode[]
  /** 子节点是否已展开（树状视图） */
  expanded?: boolean
  /** 子节点是否已加载（避免重复请求） */
  loaded?: boolean
  /** 完整绝对路径 */
  path: string
}

/** 按文件类型/扩展名取图标颜色（全应用统一） */
export function iconColor(item: Pick<FileNode, 'type' | 'name'>): string {
  if (item.type === 'dir') return '#f0b429'
  if (item.type === 'symlink') return '#06b6d4'
  const ext = item.name.split('.').pop()?.toLowerCase()
  if (['sh', 'bash'].includes(ext ?? '')) return '#22c55e'
  if (['conf', 'yml', 'yaml', 'ini'].includes(ext ?? '')) return '#a78bfa'
  if (['log'].includes(ext ?? '')) return '#94a3b8'
  if (['md', 'txt'].includes(ext ?? '')) return '#60a5fa'
  if (['json', 'xml'].includes(ext ?? '')) return '#f59e0b'
  return '#9ca3af'
}
