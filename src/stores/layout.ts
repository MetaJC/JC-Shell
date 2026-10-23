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
 * 布局持久化 Store
 * 作者：JC
 * 维护面板 dock 位置、分隔尺寸、设备卡片配置；启动时从 SQLite 加载，变更自动落盘。
 */
import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { invoke, isTauriEnv } from '@/api'
import { isMobileWebview } from '@/utils/platform'

/** 面板 ID（固定三块） */
export type PanelId = 'hostList' | 'fileSystem' | 'deviceInfo'

/** 停靠方向 */
export type Dock = 'left' | 'right' | 'bottom'

/** 设备卡片 ID（固定六张 + AI 助手） */
export type DeviceCardId = 'system' | 'cpu' | 'mem' | 'disk' | 'net' | 'proc' | 'aiAssistant'

/** 设备卡片配置 */
export interface DeviceCardConfig {
  id: DeviceCardId
  visible: boolean
  collapsed: boolean
}

/** 设备卡片排布模式：list=纵向单列；grid=网格多列 */
export type DeviceViewMode = 'list' | 'grid'

/** 全局布局配置（序列化为 JSON 存 app_setting 表 ui_layout 键） */
export interface LayoutConfig {
  leftPanelId: PanelId
  rightPanelId: PanelId
  bottomPanelId: PanelId
  leftWidth: number
  rightWidth: number
  centerTopPercent: number
  deviceCards: DeviceCardConfig[]
  /** 设备卡片排布模式 */
  deviceViewMode: DeviceViewMode
  /** CPU 卡片是否处于进程级视图 */
  cpuProcessView: boolean
  /** 内存卡片是否处于进程级视图 */
  memProcessView: boolean
  /** 各面板的折叠状态 */
  leftCollapsed: boolean
  rightCollapsed: boolean
  bottomCollapsed: boolean
}

const SETTING_KEY = 'ui_layout'

const DEFAULT_DEVICE_CARDS: DeviceCardConfig[] = [
  { id: 'system', visible: true, collapsed: true },
  { id: 'cpu', visible: true, collapsed: true },
  { id: 'mem', visible: true, collapsed: true },
  { id: 'disk', visible: true, collapsed: true },
  { id: 'net', visible: true, collapsed: true },
  { id: 'proc', visible: true, collapsed: true },
  // AI 助手卡片默认关闭，由用户在卡片设置中手动添加
  { id: 'aiAssistant', visible: false, collapsed: false }
]

const DEFAULT_LAYOUT: LayoutConfig = {
  leftPanelId: 'hostList',
  rightPanelId: 'deviceInfo',
  bottomPanelId: 'fileSystem',
  leftWidth: 220,
  rightWidth: 300,
  centerTopPercent: 55,
  deviceCards: DEFAULT_DEVICE_CARDS,
  deviceViewMode: 'list',
  cpuProcessView: false,
  memProcessView: false,
  leftCollapsed: false,
  rightCollapsed: false,
  bottomCollapsed: false
}

// 用户布局数据由 layout store 直写 SQLite，无防抖（避免卸载丢数据）

export const useLayoutStore = defineStore('layout', () => {
  const leftPanelId = ref<PanelId>(DEFAULT_LAYOUT.leftPanelId)
  const rightPanelId = ref<PanelId>(DEFAULT_LAYOUT.rightPanelId)
  const bottomPanelId = ref<PanelId>(DEFAULT_LAYOUT.bottomPanelId)
  const leftWidth = ref(DEFAULT_LAYOUT.leftWidth)
  const rightWidth = ref(DEFAULT_LAYOUT.rightWidth)
  const centerTopPercent = ref(DEFAULT_LAYOUT.centerTopPercent)
  const deviceCards = ref<DeviceCardConfig[]>(DEFAULT_DEVICE_CARDS.map(c => ({ ...c })))
  const deviceViewMode = ref<DeviceViewMode>(DEFAULT_LAYOUT.deviceViewMode)
  const cpuProcessView = ref(DEFAULT_LAYOUT.cpuProcessView)
  const memProcessView = ref(DEFAULT_LAYOUT.memProcessView)
  const leftCollapsed = ref(DEFAULT_LAYOUT.leftCollapsed)
  const rightCollapsed = ref(DEFAULT_LAYOUT.rightCollapsed)
  const bottomCollapsed = ref(DEFAULT_LAYOUT.bottomCollapsed)
  const loaded = ref(false)

  /** 序列化快照 */
  function snapshot(): LayoutConfig {
    return {
      leftPanelId: leftPanelId.value,
      rightPanelId: rightPanelId.value,
      bottomPanelId: bottomPanelId.value,
      leftWidth: leftWidth.value,
      rightWidth: rightWidth.value,
      centerTopPercent: centerTopPercent.value,
      deviceCards: deviceCards.value.map(c => ({ ...c })),
      deviceViewMode: deviceViewMode.value,
      cpuProcessView: cpuProcessView.value,
      memProcessView: memProcessView.value,
      leftCollapsed: leftCollapsed.value,
      rightCollapsed: rightCollapsed.value,
      bottomCollapsed: bottomCollapsed.value
    }
  }

  /**
   * 移动端布局默认值（每次加载都强制，不沿用桌面的持久化状态）。
   *
   * 移动端不渲染三块停靠面板 —— 会话 / 设备信息 / 文件系统已改为底部导航的独立页面
   * （见 `CAP.dockPanels()` 与 `components/pages/mobile/`），面板本身在终端页根本不挂载。
   * 这里把三者的折叠态统一置为「折叠」，只是让状态与事实一致：
   * 万一从桌面切到移动端时残留了「展开」，也不会在极端时序下先渲染出一帧面板。
   */
  function applyMobileLayoutDefaults(): void {
    if (!isMobileWebview()) return
    leftCollapsed.value = true
    rightCollapsed.value = true
    bottomCollapsed.value = true
    // 设备卡片：移动端默认「全部展开 + 单列」。手机上一屏就一列，折叠后只剩一行摘要，
    // 信息密度反而更低；纵向滚动是触摸端的自然操作，不需要靠折叠省空间
    deviceCards.value.forEach(c => { c.collapsed = false })
    deviceViewMode.value = 'list'
  }

  /** 从 SQLite 加载（非 Tauri 环境静默失败，使用默认值） */
  async function load(): Promise<void> {
    if (!isTauriEnv()) { loaded.value = true; return }
    try {
      const json = await invoke<string | null>('get_setting', { key: SETTING_KEY })
      if (!json) {
        applyMobileLayoutDefaults()
        loaded.value = true
        return
      }
      const cfg = JSON.parse(json) as Partial<LayoutConfig>
      if (cfg.leftPanelId) leftPanelId.value = cfg.leftPanelId
      if (cfg.rightPanelId) rightPanelId.value = cfg.rightPanelId
      if (cfg.bottomPanelId) bottomPanelId.value = cfg.bottomPanelId
      if (typeof cfg.leftWidth === 'number') leftWidth.value = cfg.leftWidth
      if (typeof cfg.rightWidth === 'number') rightWidth.value = cfg.rightWidth
      if (typeof cfg.centerTopPercent === 'number') centerTopPercent.value = cfg.centerTopPercent
      if (Array.isArray(cfg.deviceCards) && cfg.deviceCards.length) {
        // 以默认顺序为基底，保留用户顺序/可见性/折叠态
        const map = new Map(cfg.deviceCards.map(c => [c.id, c]))
        deviceCards.value = DEFAULT_DEVICE_CARDS.map(d => ({
          id: d.id,
          visible: map.get(d.id)?.visible ?? true,
          collapsed: map.get(d.id)?.collapsed ?? true
        }))
        // 应用用户自定义顺序
        const order = cfg.deviceCards.map(c => c.id)
        deviceCards.value.sort((a, b) => order.indexOf(a.id) - order.indexOf(b.id))
      }
      if (cfg.deviceViewMode === 'list' || cfg.deviceViewMode === 'grid') deviceViewMode.value = cfg.deviceViewMode
      if (typeof cfg.cpuProcessView === 'boolean') cpuProcessView.value = cfg.cpuProcessView
      if (typeof cfg.memProcessView === 'boolean') memProcessView.value = cfg.memProcessView
      if (typeof cfg.leftCollapsed === 'boolean') leftCollapsed.value = cfg.leftCollapsed
      if (typeof cfg.rightCollapsed === 'boolean') rightCollapsed.value = cfg.rightCollapsed
      if (typeof cfg.bottomCollapsed === 'boolean') bottomCollapsed.value = cfg.bottomCollapsed
      // 移动端：强制回到移动端默认布局，不沿用桌面的持久化展开状态
      applyMobileLayoutDefaults()
    } catch (e) {
      console.warn('[layout] load failed:', e)
    } finally {
      loaded.value = true
    }
  }

  /**
   * 落盘（直写不做 debounce）。
   * 原因：用户操作后可能立刻卸载/强退，setTimeout 防抖窗口内的 set_setting 调用会丢失，
   * 导致重装后布局/侧栏宽度等丢失。Layout 数据 < 5KB，单次写 < 1ms，拖拽期间高频写不卡。
   */
  async function persist(): Promise<void> {
    if (!isTauriEnv()) return
    try {
      await invoke('set_setting', { key: SETTING_KEY, value: JSON.stringify(snapshot()) })
    } catch (e) {
      console.warn('[layout] persist failed:', e)
    }
  }

  /** 将面板移动到目标 dock；若目标已被占用则与原位互换 */
  function movePanel(panelId: PanelId, targetDock: Dock): void {
    const currentDock = panelAt(panelId)
    if (!currentDock || currentDock === targetDock) return
    // 目标 dock 的当前面板
    const occupantId = panelIdAt(targetDock)
    if (occupantId && occupantId !== panelId) {
      // 互换
      setPanelIdAt(targetDock, panelId)
      setPanelIdAt(currentDock, occupantId)
    } else {
      setPanelIdAt(targetDock, panelId)
      setPanelIdAt(currentDock, null)
    }
    persist()
  }

  function panelAt(panelId: PanelId): Dock | null {
    if (leftPanelId.value === panelId) return 'left'
    if (rightPanelId.value === panelId) return 'right'
    if (bottomPanelId.value === panelId) return 'bottom'
    return null
  }

  function panelIdAt(dock: Dock): PanelId | null {
    return dock === 'left' ? leftPanelId.value : dock === 'right' ? rightPanelId.value : bottomPanelId.value
  }

  function setPanelIdAt(dock: Dock, id: PanelId | null): void {
    if (dock === 'left') leftPanelId.value = (id ?? 'hostList') as PanelId
    else if (dock === 'right') rightPanelId.value = (id ?? 'deviceInfo') as PanelId
    else bottomPanelId.value = (id ?? 'fileSystem') as PanelId
  }

  /** 卡片顺序变更（拖拽或设置） */
  function reorderCards(ids: DeviceCardId[]): void {
    const map = new Map(deviceCards.value.map(c => [c.id, c]))
    deviceCards.value = ids.map(id => map.get(id)!).filter(Boolean)
    persist()
  }

  /** 切换卡片可见性（设置弹窗） */
  function toggleCardVisible(id: DeviceCardId): void {
    const c = deviceCards.value.find(c => c.id === id)
    if (c) { c.visible = !c.visible; persist() }
  }

  /** 切换单卡折叠态（主窗口） */
  function toggleCardCollapsed(id: DeviceCardId): void {
    const c = deviceCards.value.find(c => c.id === id)
    if (c) { c.collapsed = !c.collapsed; persist() }
  }

  /** 一键全部展开/折叠（主窗口） */
  function setAllCollapsed(collapsed: boolean): void {
    deviceCards.value.forEach(c => { c.collapsed = collapsed })
    persist()
  }

  /** 是否所有卡片都已折叠 */
  const allCollapsed = computed(() => deviceCards.value.every(c => c.collapsed))

  /** 修改尺寸（分隔条） */
  function setLeftWidth(v: number) { leftWidth.value = v; persist() }
  function setRightWidth(v: number) { rightWidth.value = v; persist() }
  function setCenterTopPercent(v: number) { centerTopPercent.value = v; persist() }

  /** 切换设备卡片排布模式 */
  function setDeviceViewMode(mode: DeviceViewMode): void {
    deviceViewMode.value = mode
    persist()
  }

  /** 切换 CPU 进程级视图 */
  function toggleCpuProcessView(): void {
    cpuProcessView.value = !cpuProcessView.value
    persist()
  }

  /** 切换内存进程级视图 */
  function toggleMemProcessView(): void {
    memProcessView.value = !memProcessView.value
    persist()
  }

  /** 切换各面板的折叠状态 */
  function toggleLeftCollapsed(): void { leftCollapsed.value = !leftCollapsed.value; persist() }
  function toggleRightCollapsed(): void { rightCollapsed.value = !rightCollapsed.value; persist() }
  function toggleBottomCollapsed(): void { bottomCollapsed.value = !bottomCollapsed.value; persist() }

  /** 一键重置布局到默认值并删除持久化记录 */
  async function resetLayout(): Promise<void> {
    leftPanelId.value = DEFAULT_LAYOUT.leftPanelId
    rightPanelId.value = DEFAULT_LAYOUT.rightPanelId
    bottomPanelId.value = DEFAULT_LAYOUT.bottomPanelId
    leftWidth.value = DEFAULT_LAYOUT.leftWidth
    rightWidth.value = DEFAULT_LAYOUT.rightWidth
    centerTopPercent.value = DEFAULT_LAYOUT.centerTopPercent
    deviceCards.value = DEFAULT_DEVICE_CARDS.map(c => ({ ...c }))
    deviceViewMode.value = DEFAULT_LAYOUT.deviceViewMode
    cpuProcessView.value = DEFAULT_LAYOUT.cpuProcessView
    memProcessView.value = DEFAULT_LAYOUT.memProcessView
    leftCollapsed.value = DEFAULT_LAYOUT.leftCollapsed
    rightCollapsed.value = DEFAULT_LAYOUT.rightCollapsed
    bottomCollapsed.value = DEFAULT_LAYOUT.bottomCollapsed
    if (isTauriEnv()) {
      try { await invoke('delete_setting', { key: SETTING_KEY }) }
      catch (e) { console.warn('[layout] reset failed:', e) }
    }
  }

  return {
    leftPanelId, rightPanelId, bottomPanelId,
    leftWidth, rightWidth, centerTopPercent,
    deviceCards, deviceViewMode, cpuProcessView, memProcessView,
    leftCollapsed, rightCollapsed, bottomCollapsed,
    loaded, allCollapsed,
    load, persist, movePanel, panelAt, panelIdAt,
    reorderCards, toggleCardVisible, toggleCardCollapsed, setAllCollapsed,
    setLeftWidth, setRightWidth, setCenterTopPercent,
    setDeviceViewMode, toggleCpuProcessView, toggleMemProcessView, resetLayout,
    toggleLeftCollapsed, toggleRightCollapsed, toggleBottomCollapsed
  }
})
