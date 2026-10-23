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
 * 「打开会话」的跨层通道（provide / inject）
 * 作者：JC
 *
 * 会话列表在桌面端位于终端页的左停靠面板、在移动端是一个独立页面，
 * 但两者的「点会话 → 连接」都必须冒泡到 MainWindow（由 TerminalWorkspace 真正开标签）。
 *
 * 页面注册表（components/pages/index.ts）里的页面组件是动态渲染的、不带 props/emits 管道，
 * 与其为它单独造一套事件转发，不如用 provide/inject 把这条通道直接递下去。
 */
import type { InjectionKey } from 'vue'
import type { HostItem } from '@/stores/host'

/** 连接某个会话（由 MainWindow 的 DockLayout 提供） */
export type OpenHostFn = (host: HostItem) => void

export const OPEN_HOST_KEY: InjectionKey<OpenHostFn> = Symbol('jc:openHost')
