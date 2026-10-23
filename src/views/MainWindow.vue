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
  <div class="h-full w-full relative">
    <DockLayout @open-host="onOpenHost" @menu-action="onMenuAction">
      <TerminalWorkspace ref="workspaceRef" :zen="app.zenMode" />
    </DockLayout>
    <!-- 主密码锁屏：打开应用/窗口恢复时验证 -->
    <LockScreen v-if="app.locked" @unlock="app.locked = false" />
    <SecurityDialog />
    <!-- 「换一种验证方式」选择器：生物识别识别不出来时改用指纹 / 人脸 / 安全密码 -->
    <BiometricMethodDialog />
    <!-- 会话导入 / 导出（顶部菜单「工具」与会话面板工具栏共用，由单例状态驱动） -->
    <SessionTransferDialog v-model="transferDlg.visible" :view="transferDlg.view" />
    <!-- 更新弹窗（版本检查/下载/安装） -->
    <UpdateDialog />
    <!-- 同步冲突裁决弹窗（两端都改过时挂起同步，等用户选策略） -->
    <SyncConflictDialog />
    <!-- 帮助：社区QQ群弹窗 -->
    <QqGroupDialog v-model="qqGroupDialogVisible" />
    <!-- 启动弹窗（内容由管理员在后台配置，按后台设置的时机弹出） -->
    <LaunchDialog />
    <!-- 设置/主机管理/快捷键/关于 等菜单的统一弹窗（兜底，确保菜单在主窗口内一定生效） -->
    <el-dialog
      v-model="app.settingsVisible"
      :title="settingsDialogTitle"
      :width="settingsDialogWidth"
      align-center
      draggable
      destroy-on-close
      class="jc-settings-dialog"
      @close="onSettingsDialogClose"
    >
      <AppSettingsDialog />
    </el-dialog>
    <!-- 退出确认弹窗（含最小化按钮） -->
    <el-dialog
      v-model="exitDialogVisible"
      title="退出确认"
      width="380px"
      :close-on-click-modal="false"
      :close-on-press-escape="false"
      :show-close="false"
      append-to-body
      destroy-on-close
    >
      <div class="exit-confirm-body">
        <el-icon class="exit-warn-icon"><Warning /></el-icon>
        <span>确定退出 {{ appName() }} 吗？</span>
      </div>
      <template #footer>
        <div class="exit-dialog-footer">
          <div class="footer-left">
            <el-button @click="onExitMinimize">
              <el-icon style="margin-right: 4px"><Minus /></el-icon>最小化
            </el-button>
          </div>
          <div class="footer-right">
            <el-button @click="exitDialogVisible = false">取消</el-button>
            <el-button type="primary" @click="onExitConfirm">退出</el-button>
          </div>
        </div>
      </template>
    </el-dialog>
  </div>
</template>

<script setup lang="ts">
/**
 * 主窗口
 * 作者：JC
 * 三面板动态停靠布局（DockLayout）+ 终端工作区（TerminalWorkspace）。
 * 面板折叠/拖拽/会话连接等逻辑分别由对应组件承载。
 */
import { onMounted, onBeforeUnmount, ref, watch, nextTick, computed } from 'vue'
import { ElMessage } from 'element-plus'
import { handleApiError } from '@/utils/errorHandler'
import { Warning, Minus } from '@element-plus/icons-vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { open as openInOs } from '@tauri-apps/plugin-shell'
import { listen } from '@tauri-apps/api/event'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { useLayoutStore } from '@/stores/layout'
import { useAuthStore } from '@/stores/auth'
import { useHostStore } from '@/stores/host'
import { useTerminalStore, type SplitMode } from '@/stores/terminal'
import { useSettingsStore } from '@/stores/settings'
import { useSyncStore } from '@/stores/sync'
import { useAppStore } from '@/stores/app'
import { useUpdateStore } from '@/stores/update'
import { useTransferStore } from '@/stores/transfer'
import { attachShortcuts } from '@/composables/useShortcuts'
import { openSettings } from '@/composables/useSettingsOpener'
import { lockAppIfPossible } from '@/composables/useLockApp'
import { securityStatus } from '@/utils/securityPassword'
import { isBiometricPromptActive } from '@/utils/biometric'
import { transferDlg, openImportSessions, openExportSessions } from '@/utils/sessionTransferDialog'
import { CAP } from '@/utils/platform'
import { appName } from '@/utils/edition'
import { invoke, isTauriEnv } from '@/api'
import { startTelemetry, stopTelemetry } from '@/utils/telemetry'
import DockLayout from '@/components/layout/DockLayout.vue'
import TerminalWorkspace from '@/components/terminal/TerminalWorkspace.vue'
import LockScreen from '@/components/common/LockScreen.vue'
import SecurityDialog from '@/components/common/SecurityDialog.vue'
import BiometricMethodDialog from '@/components/common/BiometricMethodDialog.vue'
import UpdateDialog from '@/components/update/UpdateDialog.vue'
import SyncConflictDialog from '@/components/settings/SyncConflictDialog.vue'
import QqGroupDialog from '@/components/help/QqGroupDialog.vue'
import LaunchDialog from '@/components/launch/LaunchDialog.vue'
import AppSettingsDialog from '@/components/settings/AppSettingsDialog.vue'
import SessionTransferDialog from '@/components/host/SessionTransferDialog.vue'
import { useQqGroupStore } from '@/stores/qqGroup'
import { useLaunchDialogStore } from '@/stores/launchDialog'
import type { HostItem } from '@/stores/host'
import type { SettingsTab } from '@/stores/app'

const layout = useLayoutStore()
const auth = useAuthStore()
const hostStore = useHostStore()
const termStore = useTerminalStore()
const prefs = useSettingsStore()
const sync = useSyncStore()
const app = useAppStore()
const updateStore = useUpdateStore()
const transferStore = useTransferStore()
const qqGroupStore = useQqGroupStore()
const launchDialogStore = useLaunchDialogStore()

const workspaceRef = ref<InstanceType<typeof TerminalWorkspace> | null>(null)
const exitDialogVisible = ref(false)
/** 社区QQ群弹窗显示状态（帮助菜单触发） */
const qqGroupDialogVisible = ref(false)
/** 退出确认弹窗：点击"退出" */
async function onExitConfirm() {
  exitDialogVisible.value = false
  // 先结束遥测会话（让本次使用时长落库），再退出进程
  await stopTelemetry()
  try { await invoke<void>('app_exit') } catch { /* ignore */ }
}
/** 退出确认弹窗：点击"最小化"（隐藏到托盘） */
async function onExitMinimize() {
  exitDialogVisible.value = false
  try {
    if (isTauriEnv()) {
      await invoke<void>('hide_main_window')
    } else {
      await getCurrentWindow().minimize()
    }
  } catch { /* ignore */ }
}

function onOpenHost(host: HostItem) {
  workspaceRef.value?.openHost(host)
  // 移动端「连上就切到终端页」由会话页自己负责（见 pages/mobile/MobileSessionsPage.vue）：
  // 会话列表在移动端是底部导航的独立页面，而不是终端页里的可折叠面板，这里无需再动布局状态。
}

/**
 * 打开设置：统一走 openSettings()，与头像菜单 / 原生菜单 / Ctrl+, 保持完全一致
 * Tauri 下为独立设置窗口；Rust 端不可用时自动降级为主窗口内弹窗，不会点了没反应
 */
function openLocalSettings(tab: SettingsTab) {
  void openSettings(tab)
}

/** 设置弹窗标题（按 tab 给文案） */
const settingsDialogTitle = computed(() => {
  switch (app.settingsInitialTab) {
    case 'host': return '主机管理'
    case 'shortcut': return '快捷键参考'
    case 'about': return `关于 ${appName()}`
    default: return '设置'
  }
})
/** 设置弹窗宽度（按 tab 给不同尺寸） */
const settingsDialogWidth = computed(() => {
  if (!CAP.multiWindow()) return '100%'
  switch (app.settingsInitialTab) {
    case 'host': return '900px'
    case 'shortcut': return '820px'
    case 'about': return '720px'
    default: return '820px'
  }
})
/** 关闭时清空初始 tab，避免下次打开还停在同一个 tab */
function onSettingsDialogClose() {
  app.settingsInitialTab = 'general'
}

/** 顶部菜单 action：终端/编辑等需要操作 workspace 的行为 */
async function onMenuAction(key: string) {
  switch (key) {
    // 文件
    case 'file:newTab':
    case 'terminal:newLocal':
      workspaceRef.value?.openLocalTerminal()
      break
    case 'file:closeTab':
      workspaceRef.value?.closeActiveTab()
      break
    // 编辑
    case 'edit:copy':
      workspaceRef.value?.copyActiveSelection()
      break
    case 'edit:paste':
      workspaceRef.value?.pasteToActive()
      break
    case 'edit:selectAll':
      workspaceRef.value?.selectAllActive()
      break
    case 'edit:clear':
    case 'terminal:clear':
      workspaceRef.value?.clearActiveTerminal()
      break
    // 视图 - 分屏
    case 'view:splitNone':
      workspaceRef.value?.setSplitMode('none')
      break
    case 'view:splitH':
      workspaceRef.value?.setSplitMode('h')
      break
    case 'view:splitV':
      workspaceRef.value?.setSplitMode('v')
      break
    // 视图 - 字号
    case 'view:zoomIn':
      workspaceRef.value?.zoomInActive()
      break
    case 'view:zoomOut':
      workspaceRef.value?.zoomOutActive()
      break
    case 'view:zoomReset':
      workspaceRef.value?.zoomResetActive()
      break
    // 终端
    case 'terminal:newSsh':
      app.openSettings('host')
      break
    case 'terminal:reconnect':
      workspaceRef.value?.reconnectActive()
      break
    case 'terminal:exportLog':
      workspaceRef.value?.exportActiveTerminalLog()
      break
    // 工具
    case 'tools:settings':
      openLocalSettings('general')
      break
    case 'tools:hostManager':
      openLocalSettings('host')
      break
    case 'tools:transferQueue':
      // 显示当前激活会话的传输队列（无激活会话时使用空 sessionId，由 fs 组件兜底匹配）
      transferStore.openGlobal(termStore.tabs.find(t => t.id === termStore.activeTabId)?.sessionId || '')
      // 找到 fileSystem 所在 dock 并展开，确保用户能看到队列
      ensureFileSystemVisible()
      break
    case 'tools:importSessions':
      openImportSessions()
      break
    case 'tools:exportSessions':
      openExportSessions()
      break
    // 帮助
    case 'help:about':
      openLocalSettings('about')
      break
    case 'help:shortcuts':
      openLocalSettings('shortcut')
      break
    case 'help:qqGroups':
      qqGroupDialogVisible.value = true
      qqGroupStore.fetchGroups('jc-shell')
      break
    case 'help:feedback':
      // 优先用 Rust 内置 feedback 窗口（已配置在 tauri.conf.json）；
      // 命令失败（如 Rust 端未重新编译）时回退到主窗口弹窗，避免 404 / 无反应
      invoke('open_feedback').catch(() => {
        ElMessage.warning('反馈窗口启动失败，请检查 Tauri 客户端是否已重新编译')
      })
      break
    case 'help:checkUpdate':
      // 任何位置点"检查更新"都走 manual 模式（必弹窗：发现新版本/已是最新/检查失败/已忽略）
      updateStore.openDialog()
      await updateStore.checkUpdate('manual')
      break
    default:
      break
  }
}

/** 找到 fileSystem 所在 dock 并展开（菜单触发传输队列时使用） */
function ensureFileSystemVisible() {
  if (layout.leftPanelId === 'fileSystem' && layout.leftCollapsed) layout.toggleLeftCollapsed()
  else if (layout.rightPanelId === 'fileSystem' && layout.rightCollapsed) layout.toggleRightCollapsed()
  else if (layout.bottomPanelId === 'fileSystem' && layout.bottomCollapsed) layout.toggleBottomCollapsed()
}

/** 分屏循环：无 → 左右 → 上下 → 网格 → 无（快捷键 splitPane） */
const SPLIT_CYCLE: SplitMode[] = ['none', 'h', 'v', 'grid']
function cycleSplitMode(): void {
  const idx = SPLIT_CYCLE.indexOf(termStore.splitMode)
  workspaceRef.value?.setSplitMode(SPLIT_CYCLE[(idx + 1) % SPLIT_CYCLE.length])
}

// ===== 全局快捷键（组合键可在设置中编辑，实时生效） =====
let detachShortcuts: (() => void) | null = null
let unlistenClose: UnlistenFn | null = null
let unlistenTrayCheck: UnlistenFn | null = null

/** 开机自启动：开关变更时同步到系统（Windows 注册表） */
watch(() => prefs.settings.autoLaunch, (v) => {
  if (!isTauriEnv()) return
  invoke('set_auto_launch', { enabled: v }).catch((e: any) => {
    handleApiError(e, 'views/MainWindow.autoLaunch', { prefix: '设置开机自启动失败' })
  })
})

/** 凭证加密开关变更 → 立即按新策略重新落盘主机数据 */
watch(() => prefs.settings.encryptCredentials, () => {
  if (hostStore.loaded) hostStore.persist()
})

// ===== 统一安全密码锁屏（打开应用/窗口从后台恢复时验证） =====
let wasHidden = false
async function onVisibilityChange() {
  if (document.hidden) {
    wasHidden = true
  } else if (wasHidden) {
    wasHidden = false
    // 切出应用（切换到其它软件）后锁定：默认关闭，由「设置 → 安全」中的
    // 「切出应用时自动锁定」控制。此前该开关名为「系统挂起时锁定」，
    // 但用的就是 visibilitychange —— 在 macOS 上切到别的软件立刻触发，
    // 用户会被频繁锁屏。这里改为显式命名，并默认不开启。
    if (!prefs.settings.lockOnAppSwitch) return
    // 生物识别弹窗在 Android 上是独立 Activity：它的弹出/关闭会让本页收到
    // hidden→visible，看起来就是「用户切出去又回来了」。若不跳过，解锁过程中会被再次上锁，
    // 锁屏又立刻触发一次生物识别 → 无限重复验证人脸/指纹。
    if (isBiometricPromptActive()) return
    try {
      const st = await securityStatus()
      if (st.has_password) app.locked = true
    } catch { /* 后端未就绪忽略 */ }
  }
}

/**
 * 手动锁定（全局快捷键触发）。
 * 实现抽到 useLockApp：移动端「我的」页也有同一个入口，两处必须行为一致
 *（未设置安全密码时提示而不是直接锁上，否则锁屏后解不开）。
 */
const lockNow = lockAppIfPossible

/**
 * 清理长期未使用的 SSH 私钥托管文件。
 * 私钥内容始终保存在会话配置里，托管文件只是缓存，清理后连接时会自动重建，
 * 所以这里失败也不影响任何功能，静默忽略即可。
 */
async function pruneSshKeys(): Promise<void> {
  try {
    if (prefs.settings.sshKeyCacheTtl !== '90d') return // 永久保存
    await invoke('prune_ssh_keys', { maxIdleDays: 90 })
  } catch {
    /* 清理失败不影响使用 */
  }
}

onMounted(async () => {
  // tauri.conf 主窗口 visible=false：等首屏渲染完一帧再显示，避免启动白屏闪烁
  // 关键：必须使用 rAF 保证至少一帧真实内容已绘制，否则仍会闪白
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      void (async () => {
        try {
          const { getCurrentWindow } = await import('@tauri-apps/api/window')
          await getCurrentWindow().show()
        } catch (e) {
          console.warn('[jc-shell] show window 失败', e)
        }
      })()
    })
  })

  // 更新调度放最前面：不依赖登录态，也不被后面 auth/主机加载/关闭拦截的任何异常阻断。
  // （历史问题：调度器排在多个 await 之后，前面任一环节抛错则检查更新永远不会启动）
  console.info('[jc-shell] onMounted: 启动 update 调度器')
  updateStore.startScheduler()
  // 使用遥测（DAU / 使用时长）：同样不依赖登录态，失败静默忽略
  void startTelemetry()

  // 启动弹窗：拉取后台配置，由 store 按后台设置的时机决定是否自动弹出。
  // 用 void 异步执行：不 await，避免后端不可达 / 网络异常拖慢或阻断启动流程。
  void (async () => {
    try {
      const shouldShow = await launchDialogStore.fetchDialog()
      if (shouldShow) launchDialogStore.openDialog()
    } catch {
      /* store 内部已处理，这里兜底防止阻断启动 */
    }
  })()

  auth.subscribeEvents()
  // 子面板可能已为了确定会话归属而拉过登录态, 这里不重复请求
  if (!auth.resolved) await auth.load()
  layout.load()
  // 会话列表按账号隔离: 先绑定登录态, 之后登录/登出/换号会自动切到各自的列表
  hostStore.bindAccountScope()
  if (!hostStore.loaded) await hostStore.load()
  // 启动云同步服务(主窗口 = leader，唯一真正执行同步的引擎)
  if (isTauriEnv()) void sync.init('leader')
  // 清理过期的 SSH 私钥托管文件（私钥内容仍存在会话里，清理后连接时会自动重建）
  if (isTauriEnv()) void pruneSshKeys()
  // 统一安全密码：若开启「软件启动」保护范围，则启动需验证。
  // 后端 AppState 异步初始化，这里短暂重试，确保启动锁屏在后端就绪后被正确评估。
  for (let i = 0; i < 20; i++) {
    try {
      const st = await securityStatus()
      if (st.has_password && st.scopes.startup) app.locked = true
      break
    } catch {
      await new Promise((r) => setTimeout(r, 150))
    }
  }
  document.addEventListener('visibilitychange', onVisibilityChange)
  detachShortcuts = attachShortcuts({
    newTab: () => workspaceRef.value?.openLocalTerminal(),
    closeTab: () => workspaceRef.value?.closeActiveTab(),
    nextTab: () => workspaceRef.value?.cycleTab(1),
    prevTab: () => workspaceRef.value?.cycleTab(-1),
    splitPane: cycleSplitMode,
    // 水平分割线 → 上下布局（v）；垂直分割线 → 左右布局（h）
    splitHorizontal: () => workspaceRef.value?.setSplitMode('v'),
    splitVertical: () => workspaceRef.value?.setSplitMode('h'),
    closePane: () => workspaceRef.value?.setSplitMode('none'),
    lock: () => { void lockNow() },
  })

  // 「关闭按钮」：拦截窗口关闭请求，按设置走 最小化到托盘 / 确认退出 / 直接退出。
  // 依赖 Rust 端 lib.rs 的 Tauri 2.x tray：托盘始终挂在系统任务栏，
  // 开关打开时点 × → 调 hide_main_window Rust 命令（走 AppHandle.get_webview_window("main")，
  // 不依赖前端 webview context，避免命中错的窗口）→ 托盘图标可点恢复。
  // try/catch 包裹：注册失败不能阻断后续初始化（更新托盘监听等）。
  if (isTauriEnv()) {
    try {
      unlistenClose = await getCurrentWindow().onCloseRequested(async (event) => {
        const s = prefs.settings
        // 1) 开启"最小化到托盘" → 拦截并通过 Rust 命令精准 hide main 窗口
        if (s.minimizeToTray) {
          event.preventDefault()
          try {
            await invoke<void>('hide_main_window')
            ElMessage.info('已最小化到系统托盘，右键托盘图标可恢复窗口或退出程序')
          } catch (e) {
            // 技术细节只进日志 + 上报，用户仅看到友好提示
            console.error('[jc-shell] hide_main_window 失败:', e)
            handleApiError(e, 'views/MainWindow.hide_main_window', { prefix: '最小化到托盘失败' })
          }
          return
        }
        // 2) 开启"退出前确认" → 弹窗确认后真正退出 app 进程
        if (s.confirmOnExit) {
          event.preventDefault()
          exitDialogVisible.value = true
          return
        }
        // 3) 都不设 → 默认走 app_exit（而不是 win.destroy()，避免进程残留）
        await stopTelemetry()
        try { await invoke<void>('app_exit') } catch { /* ignore */ }
      })
    } catch (e) {
      console.error('[jc-shell] 注册窗口关闭拦截失败（关闭行为将退回系统默认）:', e)
    }
  }

  // 监听托盘菜单「检查更新」emit 事件：调 update store 走 manual 流程（必弹窗）
  if (isTauriEnv()) {
    try {
      unlistenTrayCheck = await listen<void>('tray://check_update', async () => {
        // 确保窗口可见并聚焦
        try {
          const win = getCurrentWindow()
          await win.show()
          await win.setFocus()
        } catch { /* ignore */ }
        // 延迟一帧等待 DOM 渲染，再触发弹窗
        await nextTick()
        updateStore.checkUpdate('manual').catch((e) => {
          console.error('[jc-shell] 托盘检查更新失败:', e)
        })
      })
    } catch (e) {
      console.error('[jc-shell] 注册 tray://check_update 事件监听失败:', e)
    }
  }

  // 全局事件：ESC 退出全屏 + 禁用浏览器默认右键菜单
  document.addEventListener('keydown', onGlobalKeyDown)
  document.addEventListener('contextmenu', onGlobalContextMenu)
})

onBeforeUnmount(() => {
  detachShortcuts?.()
  unlistenClose?.()
  unlistenTrayCheck?.()
  updateStore.stopScheduler()
  document.removeEventListener('visibilitychange', onVisibilityChange)
  document.removeEventListener('keydown', onGlobalKeyDown)
  document.removeEventListener('contextmenu', onGlobalContextMenu)
})

/** 全屏模式下按 ESC 退出全屏 */
function onGlobalKeyDown(e: KeyboardEvent) {
  if (e.key === 'Escape' && document.fullscreenElement) {
    document.exitFullscreen().catch((err) => {
      console.warn('[jc-shell] 退出全屏失败:', err)
    })
  }
}

/** 全局禁用浏览器默认右键菜单 */
function onGlobalContextMenu(e: Event) {
  e.preventDefault()
}
</script>

<style>
/* 弹层置顶规则（.el-overlay / .el-overlay.sec-overlay / .is-message-box / .el-message）
   已下沉到全局 style.css：主窗口与独立设置窗口等不同 webview 共用同一套样式，
   否则设置窗口拿不到 .sec-overlay 的 10012 置顶，安全验证弹窗会被设置页压在底下。 */

/* 设置弹窗：去掉 body padding + 固定高度，让 AppSettingsDialog 撑满 */
.jc-settings-dialog .el-dialog__body {
  padding: 0 !important;
  height: 640px;
  max-height: calc(100vh - 160px);
  overflow: hidden;
}

.jc-settings-dialog .el-dialog__header {
  padding: 12px 16px;
  border-bottom: 1px solid var(--jc-border);
  margin-right: 0;
}

.jc-dialog.el-dialog {
  background-color: var(--jc-bg-panel) !important;
  border: 1px solid var(--jc-border);
  border-radius: 8px;
  /* 新建/编辑会话弹窗切换「认证方式」时内容行数变化会导致弹窗高度改变，
     WebView 在旧高度处会残留未重绘的空洞、透出下层文件面板。
     这里做两层防御：隔离层叠上下文（避免文件面板里 .suck-light__glow 的
     mix-blend-mode 参与混合），并尝试提升合成层。
     真正的兜底见 style.css 中给 .el-dialog__body 补的背景色。
     注意：这里不加 overflow:hidden —— 上级文件夹的 el-select 用 :teleported="false"
     渲染在弹窗内部，裁剪会把它的下拉浮层切掉。 */
  isolation: isolate;
  backface-visibility: hidden;
  -webkit-backface-visibility: hidden;
}
.jc-dialog .el-dialog__header {
  border-bottom: 1px solid var(--jc-border);
  margin-right: 0;
  padding: 12px 16px;
}
.jc-dialog .el-dialog__title {
  color: var(--jc-text);
  font-weight: 600;
  font-size: 14px;
}
.jc-dialog .el-dialog__body {
  /* Element Plus 的 .el-dialog 自带 padding:16px，会把 header/body/footer 整体
     内缩 16px，形成一圈"只有 .el-dialog 自身背景能画到"的条带。
     实测：切换认证方式改变弹窗高度后，这圈条带会残留未重绘的空洞、透出下层文件面板
     （弹窗左右内侧各一块 16px 宽的空缺）。
     这里用负外边距把 body 拉回整宽，让 body 自己的背景覆盖这圈条带；
     左右内边距同步补回 16px，内容位置与原来完全一致，视觉无变化。 */
  margin: 0 -16px;
  padding: 16px 32px;
  color: var(--jc-text-soft);
}
.jc-dialog .el-dialog__headerbtn .el-dialog__close {
  color: var(--jc-text-muted);
}
.jc-dialog .el-dialog__headerbtn:hover .el-dialog__close {
  color: var(--jc-text);
}
.jc-dialog .el-dialog__footer {
  border-top: 1px solid var(--jc-border);
  padding: 10px 16px;
}
.jc-dialog .el-input__wrapper {
  background-color: var(--jc-bg) !important;
  box-shadow: 0 0 0 1px var(--jc-border) inset !important;
}
.jc-dialog .el-input__inner {
  color: var(--jc-text-soft) !important;
}
.jc-dialog .el-select__wrapper {
  background-color: var(--jc-bg) !important;
  box-shadow: 0 0 0 1px var(--jc-border) inset !important;
}
.jc-dialog .el-select__placeholder,
.jc-dialog .el-select__selected-item {
  color: var(--jc-text-soft) !important;
}
.jc-dialog .el-input-number__decrease,
.jc-dialog .el-input-number__increase {
  background-color: var(--jc-bg) !important;
  border-color: var(--jc-border) !important;
  color: var(--jc-text-muted) !important;
}

/* 端口转发「选择模式 / 配置编辑」弹窗：不希望出现滚动条。
   EP 的 .el-overlay-dialog 默认 overflow:auto，只要弹窗总高（含 EP 默认 50px 底边距）
   超过视口就会冒出滚动条。这里把弹窗整体限制在视口内、固定上下边距等于 top 偏移，
   再把多余内容交给 body 内部滚动并隐藏滚动条，确保整窗无可见滚动条。
   （这两个弹窗只含 el-input / el-input-number，没有 teleport 浮层，故加 overflow 不会裁剪下拉。） */
.fwd-dialog.el-dialog {
  margin-top: 2vh !important;
  margin-bottom: 2vh !important;
  max-height: 96vh;
  display: flex;
  flex-direction: column;
}
.fwd-dialog .el-dialog__header,
.fwd-dialog .el-dialog__footer {
  flex: 0 0 auto;
}
.fwd-dialog .el-dialog__body {
  flex: 1 1 auto;
  min-height: 0;
  overflow: auto;
  scrollbar-width: none;          /* Firefox */
  -ms-overflow-style: none;       /* 旧 Edge/IE */
}
.fwd-dialog .el-dialog__body::-webkit-scrollbar {
  display: none;                  /* WebKit：彻底隐藏滚动条 */
}

/* 「选择转发模式」弹窗：垂直居中（不再贴顶），并给足纵向空间 */
.fwd-dialog.fwd-mode-dialog.el-dialog {
  height: 76vh;
  max-height: 76vh;
  margin: 12vh auto !important;   /* 12 + 76 + 12 = 100vh，正好居中 */
}
.fwd-mode-dialog .el-dialog__title {
  font-size: 16px;
}
.fwd-mode-dialog .el-dialog__body {
  display: flex;
  flex-direction: column;
}
/* 卡片区吃掉剩余高度，三张卡片等高 */
.fwd-mode-dialog .mode-list {
  flex: 1 1 auto;
  min-height: 0;
}

/* 「创建 / 编辑端口转发」弹窗：铺满整个主窗口，为架构图争取最大空间，避免元素遮挡 */
.fwd-dialog.fwd-config-dialog.el-dialog {
  width: 100% !important;
  height: 100vh;
  max-height: 100vh;
  margin: 0 !important;
  border-radius: 0;
}

/* ElMessageBox 暗色主题 */
.el-message-box {
  background-color: var(--jc-bg-panel) !important;
  border: 1px solid var(--jc-border) !important;
}
.el-message-box__header {
  background-color: var(--jc-bg-panel) !important;
  border-bottom: 1px solid var(--jc-border);
}
.el-message-box__title {
  color: var(--jc-text) !important;
}
.el-message-box__message,
.el-message-box__content {
  color: var(--jc-text-soft) !important;
}
.el-message-box__input {
  background-color: var(--jc-bg) !important;
  color: var(--jc-text-soft) !important;
  border: 1px solid var(--jc-border) !important;
}
.el-message-box__btns {
  border-top: 1px solid var(--jc-border);
  background-color: var(--jc-bg-panel) !important;
}

/* 退出确认弹窗样式 */
.exit-confirm-body {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 14px;
  color: var(--jc-text);
  padding: 8px 0;
}
.exit-warn-icon {
  font-size: 24px;
  color: var(--el-color-warning);
}
.exit-dialog-footer {
  display: flex;
  justify-content: space-between;
  align-items: center;
  width: 100%;
}
.exit-dialog-footer .footer-left {
  display: flex;
  gap: 8px;
}
.exit-dialog-footer .footer-right {
  display: flex;
  gap: 8px;
}
</style>
