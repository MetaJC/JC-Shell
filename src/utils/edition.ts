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
 * 版本标识与品牌名
 * 作者：JC
 *
 * 由构建期常量 __EDITION__ 区分（见各自工程的 vite.config.ts define）：
 * - jc-shell-community/vite.config.ts → 'community'
 * - 另一构建目标的 vite.config.ts      → 'pro'
 *
 * 构建时，经 appName() 渲染的软件名由本工程的版本决定（本工程为「JC Shell」）。
 * Rust 侧品牌（原生菜单等）由 `pro-edition` cargo feature 控制，
 * 见 src-tauri/src/common/constants.rs 的 APP_NAME。
 */

export type Edition = 'community' | 'pro'

/** 宿主全局标记（由另一构建目标的入口设置） */
interface EditionGlobal {
  __JC_EDITION__?: unknown
}

/** 读取构建期注入的版本标识（未注入时按 community 处理） */
function injectedEdition(): string {
  // 1. 宿主全局标记：dev/build 均可靠。
  //    必须优先于 define —— Vite dev 模式不对「项目外文件」（社区版源码经 alias
  //    引入另一构建目标工程）做 define 替换，此时 __EDITION__ 是未声明标识符。
  const globalEdition = (globalThis as EditionGlobal).__JC_EDITION__
  if (globalEdition === 'pro') return 'pro'
  // 2. 构建期常量 __EDITION__（Vite define；生产构建可靠）
  try {
    if (typeof __EDITION__ === 'string' && __EDITION__) return __EDITION__
  } catch {
    return ''
  }
  return ''
}

/** 当前版本标识 */
export function currentEdition(): Edition {
  return injectedEdition() === 'pro' ? 'pro' : 'community'
}

/** 是否启用扩展版本（本工程默认 false） */
export function isProEdition(): boolean {
  return currentEdition() === 'pro'
}

/** 界面展示用软件名（本工程为 'JC Shell'） */
export function appName(): string {
  return isProEdition() ? 'JC Shell Pro' : 'JC Shell'
}
