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
 * 终端字号工具
 * 作者：JC
 *
 * 桌面端：渲染字号 = 设置值。
 * 移动端：屏幕窄，同样字号可视列数太少，故整体缩小一档（MOBILE_FONT_OFFSET）。
 * 这个「设置值 ↔ 实际渲染值」的换算同时被两处使用，必须共用一份：
 *   1. TerminalView —— 把字号应用到 xterm；
 *   2. 终端快捷设置面板 —— 滑杆显示与实际渲染一致的字号，拖动时换算回设置值。
 */
import { isMobileWebview } from './platform'

/** 终端字号上下限（含 Ctrl+滚轮缩放） */
export const FONT_MIN = 8
export const FONT_MAX = 32

/** 实际渲染字号的下限：移动端快捷面板最小挡位是 6，这里不能卡在 8，
 *  否则 6 / 7 两挡会被夹到同一值、拖动无变化 */
export const RENDER_FONT_MIN = 6

/** 移动端渲染字号相对设置值的下移量（14 → 10） */
export const MOBILE_FONT_OFFSET = 4

/** 设置值 → 实际渲染字号（移动端整体缩小一档，仍受 RENDER_FONT_MIN 约束） */
export function effFontSize(size: number): number {
  return isMobileWebview() ? Math.max(RENDER_FONT_MIN, size - MOBILE_FONT_OFFSET) : size
}

/** 实际渲染字号 → 设置值（effFontSize 的反向换算，供快捷设置面板滑杆使用） */
export function settingsFontSize(effSize: number): number {
  return isMobileWebview() ? effSize + MOBILE_FONT_OFFSET : effSize
}
