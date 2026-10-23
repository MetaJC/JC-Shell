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
 * 自动更新 Store
 * 作者：JC
 * 负责：检查更新、静默下载、状态维护、定时检查、忽略版本、下载进度、自动安装
 */
import { defineStore } from 'pinia'
import { ref, computed, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { isTauriEnv } from '@/api'
import { useSettingsStore } from './settings'
import { useI18nStore } from './i18n'
import { platformType } from '@/utils/platform'

export interface AssetInfo {
  platform: string
  fileName: string
  fileSize?: number
  fileSha256?: string
  downloadUrls: string[]
}

export interface ProductInfo {
  key: string
  name: string
}

export interface LatestVersionInfo {
  versionCode: string
  versionName: string
  changelog?: string
  releaseNotesUrl?: string
  isMandatory?: boolean
  minVersion?: string
  publishTime?: string
  assets: AssetInfo[]
}

export interface CheckUpdateResponse {
  hasUpdate: boolean
  product?: ProductInfo
  latest?: LatestVersionInfo
}

export interface DownloadInfo {
  filePath: string
  fileSize: number
  fileSha256?: string
  versionCode: string
}

/** 检查更新触发模式 */
export type CheckMode = 'auto_startup' | 'auto_schedule' | 'manual'

export type UpdateStatus = 'IDLE' | 'CHECKING' | 'NO_UPDATE' | 'AVAILABLE' | 'DOWNLOADING' | 'DOWNLOADED' | 'INSTALLING' | 'ERROR'

/** 下载进度事件载荷 */
interface DownloadProgressPayload {
  downloaded: number
  total: number
}

const CHECK_INTERVAL_MS = 2 * 60 * 60 * 1000 // 2 小时
const STARTUP_DELAY_MS = 1 * 1000 // 启动后 1 秒首检
const IGNORED_KEY = 'jc_shell_ignored_versions'

function errToString(e: unknown): string {
  if (e == null) return '未知错误'
  if (typeof e === 'string') return e
  if (typeof e === 'object') {
    const obj = e as { message?: string; msg?: string; kind?: string }
    if (obj.message) return String(obj.message)
    if (obj.msg) return String(obj.msg)
    if (obj.kind) return `${obj.kind}`
    try { return JSON.stringify(e) } catch { return '未知错误' }
  }
  return String(e)
}

/**
 * 遥测 / 更新安装包平台标识。
 * 桌面三系（WINDOWS / MACOS / LINUX）逻辑保持不变；移动端走 platformType() 优先分支，
 * 避免 Android WebView 的 UA（"Linux; Android …"，且不含 ARM/aarch64 字样）被错误兜底成 LINUX_AMD64。
 * 仅新增移动端分支，桌面行为完全一致（符合"移动端改动不影响 PC"红线）。
 */
export function detectPlatform(): string {
  // 移动端：直接按真实平台类型返回，不依赖 UA 关键字
  const p = platformType()
  if (p === 'android') return 'ANDROID'
  if (p === 'ios') return 'IOS'
  // 桌面端：沿用原有 UA 探测（win/mac 优先，其余兜底 LINUX）
  const ua = navigator.userAgent.toLowerCase()
  const isArm = navigator.userAgent.includes('ARM') || navigator.userAgent.includes('aarch64')
  if (ua.includes('win')) return isArm ? 'WINDOWS_ARM64' : 'WINDOWS_X64'
  if (ua.includes('mac')) return isArm ? 'MACOS_ARM64' : 'MACOS_X64'
  return isArm ? 'LINUX_ARM64' : 'LINUX_AMD64'
}

function getCurrentVersion(): string {
  const injected: unknown = __APP_VERSION__
  if (typeof injected === 'string' && injected.trim()) return injected.trim()
  const viteEnv: unknown = import.meta.env.VITE_APP_VERSION
  if (typeof viteEnv === 'string' && viteEnv.trim()) return viteEnv.trim()
  return '0.1.0'
}

function loadIgnored(): Set<string> {
  try {
    const arr = JSON.parse(localStorage.getItem(IGNORED_KEY) || '[]') as string[]
    return new Set(arr)
  } catch { return new Set() }
}
function saveIgnored(set: Set<string>): void {
  try { localStorage.setItem(IGNORED_KEY, JSON.stringify([...set])) } catch { /* ignore */ }
}

/**
 * 自动弹窗前先确保主窗口可见并聚焦。
 * 启动首检/托盘检查时窗口可能不在前台，直接开弹窗会出现渲染异常/被遮挡，
 * 用户点击又落在遮罩上导致弹窗"一闪而过"。
 */
async function ensureWindowFocus(): Promise<void> {
  if (!isTauriEnv()) return
  try {
    const win = getCurrentWindow()
    await win.show()
    await win.unminimize()
    await win.setFocus()
  } catch { /* ignore */ }
}

export const useUpdateStore = defineStore('update', () => {
  const settingsStore = useSettingsStore()

  const status = ref<UpdateStatus>('IDLE')
  const lastCheckAt = ref<number>(0)
  const latestInfo = ref<LatestVersionInfo | null>(null)
  const productInfo = ref<ProductInfo | null>(null)
  const downloadInfo = ref<DownloadInfo | null>(null)
  const errorMsg = ref<string | null>(null)
  const dialogVisible = ref(false)
  const ignoredVersions = ref<Set<string>>(loadIgnored())

  /** 下载进度百分比 0-100 */
  const downloadProgress = ref(0)
  /** 是否正在下载 */
  const downloading = computed(() => status.value === 'DOWNLOADING')

  /** 启动检查模式：控制弹窗行为（启动时弹窗 vs 定时检查静默） */
  const startupCheckMode = ref<'startup' | 'schedule'>('startup')

  let checkTimer: number | null = null
  let startupTimer: number | null = null
  let progressUnlisten: (() => void) | null = null
  let bgDownloadTimer: number | null = null

  const updateReady = computed(() => status.value === 'DOWNLOADED' && !!downloadInfo.value)
  const updateAvailable = computed(() => status.value === 'AVAILABLE')
  const currentVersion = getCurrentVersion()
  const platform = detectPlatform()

  /** 当前安装级别：user(用户级) / system(系统级) / unknown(无法判断，如开发态) */
  const installScope = ref<'user' | 'system' | 'unknown'>('unknown')
  let scopeLoaded = false

  /**
   * 探测当前安装级别（只需一次）。
   * 用户级装的程序若拿系统级包升级，会装成第二份或被要求先卸载，反之亦然，
   * 所以下载前必须先知道自己是哪一种。
   */
  async function ensureInstallScope(): Promise<void> {
    if (scopeLoaded) return
    scopeLoaded = true
    try {
      const s = await invoke<string>('get_install_scope')
      if (s === 'user' || s === 'system') installScope.value = s
    } catch (e) {
      console.warn('[update] 获取安装级别失败，按默认策略选择安装包', e)
    }
  }

  /**
   * 从 assets 中挑出最适合当前客户端的安装包。
   * 后台细分了「用户级 / 系统级」安装包后，客户端只发通用键（如 WINDOWS_X64），
   * 排序规则：精确匹配 >（与当前安装级别一致的包）> 另一级别 > 通用。
   * 找不到任何候选时返回 null，由调用方给用户友好提示。
   */
  function pickAsset(): AssetInfo | null {
    if (!latestInfo.value?.assets?.length) return null
    const list = latestInfo.value.assets
    return rankAssetsByPlatform(platform, list)[0] || null
  }

  /**
   * 按 platform 与安装包匹配度排序，最前面的就是最佳选择。
   * 仅改顺序，不丢任何 asset。
   */
  function rankAssetsByPlatform(clientPlatform: string, assets: AssetInfo[]): AssetInfo[] {
    if (!clientPlatform || !assets?.length) return assets?.slice() || []
    const upper = clientPlatform.toUpperCase()
    // 系统级安装优先系统级包；用户级/未知则优先用户级（无需管理员，覆盖最广）
    const systemFirst = installScope.value === 'system'
    const orderOf = (p: string): number => {
        const pu = (p || '').toUpperCase()
        if (pu === upper) return 0
        if (upper === 'WINDOWS_X64') {
            if (pu === 'WINDOWS_USER_X64') return systemFirst ? 2 : 1
            if (pu === 'WINDOWS_SYSTEM_X64') return systemFirst ? 1 : 2
        }
        if (upper === 'WINDOWS_ARM64') {
            if (pu === 'WINDOWS_USER_ARM64') return systemFirst ? 2 : 1
            if (pu === 'WINDOWS_SYSTEM_ARM64') return systemFirst ? 1 : 2
        }
        if (upper === 'LINUX_AMD64') {
            if (pu === 'LINUX_DEB_AMD64') return 1
            if (pu === 'LINUX_RPM_AMD64') return 2
        }
        return 99
    }
    return assets.slice().sort((a, b) => orderOf(a.platform) - orderOf(b.platform))
  }

  const currentIgnored = computed(() =>
    !!latestInfo.value && ignoredVersions.value.has(latestInfo.value.versionCode)
  )

  function ignoreVersion(code: string): void {
    if (ignoredVersions.value.has(code)) return
    const next = new Set(ignoredVersions.value)
    next.add(code)
    ignoredVersions.value = next
    saveIgnored(next)
  }
  function unignoreVersion(code: string): void {
    if (!ignoredVersions.value.has(code)) return
    const next = new Set(ignoredVersions.value)
    next.delete(code)
    ignoredVersions.value = next
    saveIgnored(next)
  }
  function isIgnored(code: string): boolean { return ignoredVersions.value.has(code) }

  /** 重置状态到检查前 */
  function resetState(): void {
    status.value = 'IDLE'
    errorMsg.value = null
    downloadProgress.value = 0
  }

  /**
   * 检查更新核心逻辑
   *  - mode='manual'：用户主动点击（设置菜单/顶部导航/托盘），无论结果都弹窗
   *  - mode='auto_startup'：启动时自动检查，有更新弹窗（含忽略复选框）
   *  - mode='auto_schedule'：定时轮询，根据闲时自动更新设置决定行为
   */
  async function checkUpdate(mode: CheckMode = 'auto_schedule'): Promise<CheckUpdateResponse | null> {
    console.info('[update] 检查更新触发, mode=' + mode)
    status.value = 'CHECKING'
    errorMsg.value = null
    downloadProgress.value = 0

    try {
      const resp = await invoke<CheckUpdateResponse>('check_update', {
        currentVersion: currentVersion,
        channel: settingsStore.settings.updateChannel,
        platform,
      })
      lastCheckAt.value = Date.now()

      if (resp.hasUpdate && resp.latest) {
        const code = resp.latest.versionCode
        latestInfo.value = resp.latest
        productInfo.value = resp.product ?? null
        if (isIgnored(code) && mode !== 'manual') {
          // 被忽略版本：自动/启动模式不弹窗不下载
          console.info('[update] 命中已忽略版本 ' + code + '，跳过弹窗下载（主动点击不受此限制）')
          status.value = 'NO_UPDATE'
          return resp
        }
        // 命中忽略但用户主动点击：仍然弹窗显示，让用户在弹窗内"恢复检查"
        if (isIgnored(code) && mode === 'manual') {
          console.info('[update] 命中已忽略版本 ' + code + '，但用户主动点击，弹窗显示')
        }
        status.value = 'AVAILABLE'

        // 根据模式决定行为（检查只检查，不下载；下载必须由用户确认或闲时自动更新触发）
        switch (mode) {
          case 'manual':
            // 用户主动点击：无论结果必弹窗；重置为普通模式按钮组（忽略此版本/下载更新）
            startupCheckMode.value = 'schedule'
            dialogVisible.value = true
            break

          case 'auto_startup':
            // 启动时检查：开启「闲时自动更新」且非强制 → 不提示、静默下载，完成后右上角提示；
            // 否则弹窗（含"忽略此次更新"复选框，默认不勾选）
            if (settingsStore.settings.silentUpdate && !resp.latest.isMandatory) {
              console.info('[update] 启动检查：闲时自动更新开启，改为静默下载（不弹窗）')
              startupCheckMode.value = 'schedule'
              void silentDownloadLatest()
            } else {
              startupCheckMode.value = 'startup'
              await ensureWindowFocus()
              dialogVisible.value = true
            }
            break

          case 'auto_schedule':
            // 定时轮询：根据"闲时自动更新"设置决定行为
            if (settingsStore.settings.silentUpdate && !resp.latest.isMandatory) {
              // 闲时自动更新开启 + 非强制 → 后台静默下载，完成后右上角提示（不弹窗）
              console.info('[update] 定时检查：闲时自动更新开启，启动静默下载')
              void silentDownloadLatest()
            } else {
              // 其他情况：保持 AVAILABLE 状态，由右上角"新版本已就绪"或下次手动检查触发
              // （不弹窗，避免与 startup 弹窗或手动检查弹窗冲突）
              console.info('[update] 定时检查：闲时自动更新关闭，不自动下载')
            }
            break
        }
      } else {
        status.value = 'NO_UPDATE'
        if (mode === 'manual') {
          startupCheckMode.value = 'schedule'
          dialogVisible.value = true
        }
      }
      return resp
    } catch (e) {
      status.value = 'ERROR'
      // 仅在控制台记录详细错误（含后端地址等内部信息），客户端只展示友好文案，不暴露后端接口地址
      console.warn('检查更新失败', e)
      errorMsg.value = useI18nStore().t('settings.about.updateCheckFail')
      if (mode === 'manual' || mode === 'auto_startup') dialogVisible.value = true
      return null
    }
  }

  /**
   * 下载完成后是否自动安装并重启。
   *  - true：用户主动点过「更新」（手动更新）→ 下载完即自动安装重启，一气呵成；
   *  - false：闲时静默下载 → 只准备就绪，等用户点右上角「新版本已就绪」再装。
   */
  let autoInstallOnComplete = false

  /** 后台静默下载（闲时自动更新；不自动安装，等用户点就绪提示） */
  async function silentDownloadLatest(): Promise<void> {
    autoInstallOnComplete = false
    await ensureInstallScope()
    const asset = pickAsset()
    if (!asset || !latestInfo.value) return
    try {
      status.value = 'DOWNLOADING'
      downloadProgress.value = 0
      await invoke<void>('silent_download_update', {
        asset, versionCode: latestInfo.value.versionCode
      })
      const info = await invoke<DownloadInfo | null>('get_download_info')
      if (info) {
        downloadInfo.value = info
        status.value = 'DOWNLOADED'
        downloadProgress.value = 100
      } else {
        status.value = 'AVAILABLE'
      }
    } catch (e) {
      console.warn('静默下载失败', e)
      status.value = 'AVAILABLE'
    }
  }

  /**
   * 手动下载（用户主动点「更新」）。用户已经表达了更新意愿，所以下载完成即
   * 自动安装并重启，不再要求二次点击；期间即使关闭弹窗也会继续，完成后照样自动安装。
   */
  async function startDownload(): Promise<void> {
    autoInstallOnComplete = true
    await ensureInstallScope()
    const asset = pickAsset()
    if (!asset || !latestInfo.value) {
      // 避免静默 return 导致"点击无反应"：明确报错，让用户知道原因
      errorMsg.value = '未找到匹配当前平台的安装包，无法下载（请检查后端该版本是否上传了对应平台资源）'
      status.value = 'ERROR'
      return
    }
    status.value = 'DOWNLOADING'
    downloadProgress.value = 0
    try {
      await invoke<void>('download_update', { asset, versionCode: latestInfo.value.versionCode })
      const info = await invoke<DownloadInfo | null>('get_download_info')
      if (info) {
        downloadInfo.value = info
        downloadProgress.value = 100
        if (autoInstallOnComplete) {
          // 手动更新：下载完成直接进入安装，用户感知上是"下载完就自动装好并重启"
          status.value = 'INSTALLING'
          await installNow()
        } else {
          status.value = 'DOWNLOADED'
        }
      } else {
        status.value = 'AVAILABLE'
        errorMsg.value = '下载完成但未找到安装包，请重试'
      }
    } catch (e) {
      status.value = 'ERROR'
      errorMsg.value = errToString(e)
    }
  }

  /** 取消下载（用户显式取消，不自动安装） */
  async function cancelDownload(): Promise<void> {
    try { await invoke<void>('cancel_update_download') } catch { /* ignore */ }
    autoInstallOnComplete = false
    status.value = 'AVAILABLE'
    downloadProgress.value = 0
  }

  /**
   * 点击右上角「新版本已就绪」：直接静默安装并重启。
   * 安装包早已下载好，用户交互上就是"重启了一下软件"——不再弹窗要用户二次确认。
   */
  async function installReady(): Promise<void> {
    if (!downloadInfo.value) {
      errorMsg.value = '下载信息已失效，请重新检查更新'
      status.value = 'ERROR'
      dialogVisible.value = true
      return
    }
    status.value = 'INSTALLING'
    await installNow()
  }

  /** 执行安装并退出 */
  async function installNow(): Promise<void> {
    if (!downloadInfo.value) {
      errorMsg.value = '尚未下载安装包，无法安装'
      return
    }
    try {
      await invoke<void>('install_update')
    } catch (e) {
      errorMsg.value = errToString(e)
    }
  }

  function openDialog() { dialogVisible.value = true }
  function closeDialog() { dialogVisible.value = false }

  /**
   * 启动模式下"稍后更新"→ 5min 后台自动下载
   * 下载完成后仅置为 DOWNLOADED，由右上角"新版本已就绪"按钮引导用户安装（不自动拉起）
   */
  function scheduleBackgroundDownload(_versionCode: string, delayMs: number): void {
    cancelBackgroundDownload()
    bgDownloadTimer = window.setTimeout(async () => {
      bgDownloadTimer = null
      if (!latestInfo.value) return
      autoInstallOnComplete = false // 闲时下载：只准备就绪，等用户点提示再装
      await ensureInstallScope()
      const asset = pickAsset()
      if (!asset) return
      try {
        status.value = 'DOWNLOADING'
        downloadProgress.value = 0
        await invoke<void>('silent_download_update', {
          asset, versionCode: latestInfo.value.versionCode
        })
        const info = await invoke<DownloadInfo | null>('get_download_info')
        if (info) {
          downloadInfo.value = info
          status.value = 'DOWNLOADED'
          downloadProgress.value = 100
        } else {
          status.value = 'AVAILABLE'
        }
      } catch (e) {
        console.warn('后台定时下载失败', e)
        status.value = 'AVAILABLE'
      }
    }, delayMs)
  }

  function cancelBackgroundDownload(): void {
    if (bgDownloadTimer !== null) {
      window.clearTimeout(bgDownloadTimer)
      bgDownloadTimer = null
    }
  }

  /** 启动时注册：5 秒后首检（auto_startup 模式，有更新弹窗）+ 每 2 小时轮询 */
  function startScheduler() {
    stopScheduler()
    console.info('[update] 启动调度器注册')
    startupTimer = window.setTimeout(async () => {
      console.info('[update] 5s 启动首检触发')
      try {
        // 先恢复已有下载状态（如果有之前下载的安装包）
        const info = await invoke<DownloadInfo | null>('get_download_info')
        if (info && info.filePath) {
          console.info('[update] 检测到已下载包，匹配版本')
          // 走静默 check_update：获取 latest 但不弹窗
          const resp = await invoke<CheckUpdateResponse>('check_update', {
            currentVersion,
            channel: settingsStore.settings.updateChannel,
            platform,
          })
          if (resp.hasUpdate && resp.latest && resp.latest.versionCode === info.versionCode) {
            latestInfo.value = resp.latest
            productInfo.value = resp.product ?? null
            downloadInfo.value = info
            status.value = 'DOWNLOADED'
            downloadProgress.value = 100
            // 不弹窗：只让右上角「新版本已就绪」常驻显示。
            // 用户没点之前，关掉应用再打开依旧是当前版本，提示仍然在。
            console.info('[update] 启动时检测到已就绪的更新包 → 仅显示右上角提示（不弹窗）')
            return
          }
        }
        // 正常 startup 检查
        await checkUpdate('auto_startup')
      } catch (e) {
        console.warn('[update] 启动首检失败', e)
      }
    }, STARTUP_DELAY_MS)
    checkTimer = window.setInterval(() => {
      console.info('[update] 定时轮询触发')
      checkUpdate('auto_schedule').catch((e) => {
        console.warn('[update] 定时轮询失败', e)
      })
    }, CHECK_INTERVAL_MS)
    console.info('[update] 调度器已注册, startupTimer=' + startupTimer + ', checkTimer=' + checkTimer)
  }
  function stopScheduler() {
    if (startupTimer !== null) { window.clearTimeout(startupTimer); startupTimer = null }
    if (checkTimer !== null) { window.clearInterval(checkTimer); checkTimer = null }
  }

  /** 监听下载进度事件 */
  async function startProgressListener(): Promise<void> {
    if (progressUnlisten) return
    progressUnlisten = await listen<DownloadProgressPayload>('update-download-progress', (ev) => {
      const { downloaded, total } = ev.payload
      if (total > 0) {
        downloadProgress.value = Math.round((downloaded / total) * 100)
      }
    })
  }

  /** 恢复已有下载状态（启动时调用） */
  async function restoreDownloadState(): Promise<void> {
    try {
      const info = await invoke<DownloadInfo | null>('get_download_info')
      if (info && info.filePath) {
        // 有已下载的包 → 静默校验版本是否匹配（不能用 checkUpdate('manual')，那会弹窗）
        const resp = await invoke<CheckUpdateResponse>('check_update', {
          currentVersion,
          channel: settingsStore.settings.updateChannel,
          platform,
        })
        if (resp.hasUpdate && resp.latest && resp.latest.versionCode === info.versionCode) {
          latestInfo.value = resp.latest
          productInfo.value = resp.product ?? null
          downloadInfo.value = info
          status.value = 'DOWNLOADED'
          downloadProgress.value = 100
          console.info('[update] 恢复已就绪的更新包 → 仅显示右上角提示（不弹窗）')
        } else if (!resp.hasUpdate) {
          status.value = 'NO_UPDATE'
        } else {
          status.value = 'AVAILABLE'
        }
      } else {
        // 没有已下载的包，不调用 check_update（让 scheduler 5s 后自己触发）
        console.info('[update] 无已下载包，跳过 restore')
      }
    } catch (e) {
      console.warn('[update] restoreDownloadState 失败', e)
    }
  }

  /** 监听下载进度事件（在 useUpdateStore 后立即调用） */
  void startProgressListener()

  return {
    // state
    status, lastCheckAt, latestInfo, productInfo, downloadInfo, errorMsg, dialogVisible,
    ignoredVersions, currentIgnored, downloadProgress, startupCheckMode,
    // computed
    updateReady, updateAvailable, currentVersion, platform, downloading,
    // actions
    checkUpdate, startDownload, cancelDownload, installReady, installNow,
    openDialog, closeDialog, startScheduler, stopScheduler, pickAsset,
    ignoreVersion, unignoreVersion, isIgnored, silentDownloadLatest,
    restoreDownloadState, resetState,
    scheduleBackgroundDownload, cancelBackgroundDownload,
  }
})
