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
 * 侧边 Dock 页面导航 Store
 * 作者：JC
 * Dock 按钮即页面 Tab：这里只保存「当前选中的页面 id」。
 * 刻意不做持久化 —— 每次启动都默认回到「终端」页（terminal）。
 * 其它位置想切页时直接调用 setActivePage 即可（等价于路由跳转）。
 */
import { defineStore } from 'pinia'
import { ref } from 'vue'
import { isMobileWebview } from '@/utils/platform'

/** 默认页面：终端（桌面端） */
export const DEFAULT_DOCK_PAGE = 'terminal'

/**
 * 移动端默认页面：会话。
 * 移动端底部导航的第一项就是会话，且开局直接落到终端页只会看到一个空态
 *（移动端没有本地终端，必须先选会话），所以默认停在会话列表更符合主流程。
 */
export const MOBILE_DEFAULT_DOCK_PAGE = 'sessions'

/**
 * 移动端底部导航的页面顺序（与 MobileTabBar 的 5 项一一对应）。
 * 左右滑动切页依赖这份顺序取「上一页 / 下一页」，故放在 store 里作为单一真相源。
 */
export const MOBILE_PAGE_ORDER = ['sessions', 'terminal', 'sftp', 'dashboard', 'profile'] as const

/** 移动端子页面（目前只有会话编辑页；后续可扩展更多） */
export interface MobileSubPage {
  /** 子页类型 */
  type: 'session-edit'
  /** 编辑模式时的会话 id（新建为 undefined） */
  hostId?: string
  /** 新建模式时的默认上级文件夹 id */
  defaultParentId?: string | null
}

export const useDockStore = defineStore('dock', () => {
  /** 当前选中的页面 id（平台在启动时固定，故此处直接求值一次即可） */
  const activePage = ref<string>(isMobileWebview() ? MOBILE_DEFAULT_DOCK_PAGE : DEFAULT_DOCK_PAGE)

  /**
   * 切页方向：驱动移动端「页面整体左右平移」的切换动效。
   * 'next' = 新页在右、从右侧滑入；'prev' = 新页在左、从左侧滑入；'' = 不做动效。
   * 由 setActivePage 按底部导航顺序自动算出，调用方（TabBar / 滑动手势 / 业务跳转）无需关心。
   */
  const pageSlide = ref<'' | 'next' | 'prev'>('')

  function setActivePage(id: string): void {
    pageSlide.value = slideDirectionOf(activePage.value, id)
    activePage.value = id
  }

  /** 按移动端导航顺序判断滑动方向；桌面端页面不在顺序表内（索引 -1）→ 不动效 */
  function slideDirectionOf(from: string, to: string): '' | 'next' | 'prev' {
    if (from === to) return ''
    const order: readonly string[] = MOBILE_PAGE_ORDER
    const a = order.indexOf(from)
    const b = order.indexOf(to)
    if (a < 0 || b < 0) return ''
    return b > a ? 'next' : 'prev'
  }

  /** 移动端子页面栈：会话列表上的「新建/编辑会话」不再弹窗，而是整页进入，
   *  可叠加多层；桌面端始终为空（桌面仍用弹窗，不进子页栈）。 */
  const mobileSubPages = ref<MobileSubPage[]>([])

  /** 进入一个移动端子页（压栈） */
  function pushMobileSubPage(page: MobileSubPage): void {
    mobileSubPages.value.push(page)
  }
  /** 返回（出栈顶层） */
  function popMobileSubPage(): void {
    mobileSubPages.value.pop()
  }
  /** 清空（如切到其它底部导航页时调用） */
  function clearMobileSubPages(): void {
    mobileSubPages.value = []
  }

  return {
    activePage,
    pageSlide,
    setActivePage,
    mobileSubPages,
    pushMobileSubPage,
    popMobileSubPage,
    clearMobileSubPages,
  }
})
