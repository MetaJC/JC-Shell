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

<script setup lang="ts">
/**
 * 设置弹窗：左侧菜单（按组分组）+ 右侧内容
 * 作者：JC
 */
import { ref, computed, watch, reactive, markRaw, onMounted, onUnmounted, type Component } from 'vue'
import { useAppStore, type ThemeMode } from '@/stores/app'
import { useI18nStore, type Locale } from '@/stores/i18n'
import { useAuthStore } from '@/stores/auth'
import { useConsentStore } from '@/stores/consent'
import { useDockStore } from '@/stores/dock'
import { useHostStore, type HostItem, type Namespace } from '@/stores/host'
import { useAiStore } from '@/stores/ai'
import { useSettingsStore, SHORTCUT_DEFAULT } from '@/stores/settings'
import { useSyncStore } from '@/stores/sync'
import { useUpdateStore } from '@/stores/update'
import { copyText } from '@/utils/clipboard'
import { listKnownHosts, clearKnownHosts as clearKnownHostsUtil } from '@/utils/knownHosts'
import {
  securityStatus, securitySetPassword, securityDisable, securityUpdateScopes,
  securityWipeAndDisable, securitySetBiometric,
  type SecurityScopes, type SecurityStatus,
} from '@/utils/securityPassword'
import { biometricAvailability, type BiometricKind, type BiometricMode } from '@/utils/biometric'
import { openSetPassword, openVerify } from '@/utils/securityDialog'
import { invoke, isTauriEnv } from '@/api'
import { ElMessage, ElMessageBox } from 'element-plus'
import { handleApiError } from '@/utils/errorHandler'
import { isMobileWebview, CAP } from '@/utils/platform'
import { appName } from '@/utils/edition'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { open as openInOs } from '@tauri-apps/plugin-shell'
import UpdateDialog from '@/components/update/UpdateDialog.vue'
import {
  User, Setting, Monitor, Folder, Brush, Operation,
  Lock, Upload, Download, ArrowLeft, ArrowRight,
  Connection as GlobeIcon, Collection as InfoCardIcon,
  Key, MagicStick, InfoFilled, WarningFilled,
} from '@element-plus/icons-vue'

const app = useAppStore()
const i18n = useI18nStore()
const auth = useAuthStore()
const consent = useConsentStore()
// 账号区协议同意复选框：未勾选禁止登录/注册
const uaUrl = computed(() => consent.config?.userAgreementUrl || null)
const ppUrl = computed(() => consent.config?.privacyPolicyUrl || null)
const pcAgreed = ref(false)
function openLegalLink(url: string | null) {
  if (!url) { ElMessage.warning(t('consent.linkMissing')); return }
  if (isTauriEnv()) void invoke('open_external_url', { url })
  else window.open(url, '_blank')
}
function onPcLogin() {
  if (!pcAgreed.value) { ElMessage.warning(t('user.phoneLogin.agreeRequired')); return }
  consent.markAgreed()
  void auth.startLogin()
}
/** 全局设置（修改即实时生效 + 自动持久化 + 跨窗口同步） */
const prefs = useSettingsStore()
const settings = prefs.settings
const hostStore = useHostStore()
const aiStore = useAiStore()
const sync = useSyncStore()
const t = (k: string, p?: Record<string, string | number>) => i18n.t(k, p)

type MenuKey =
  | 'account'
  | 'data' | 'general' | 'terminal' | 'host'
  | 'shortcut'
  | 'appearance' | 'language' | 'preference'
  | 'security' | 'beta' | 'about'

interface MenuItem {
  key: MenuKey
  label: string
  icon: Component
  group: number
  /** 叶子项：本身直接就是设置项、无下级页面（移动端一级列表不显示右箭头） */
  leaf?: boolean
}
const MENU_ITEMS: MenuItem[] = [
  // 个人
  { key: 'account',  label: 'settings.menu.account',  icon: markRaw(User),     group: 1 },
  // 应用
  { key: 'data',     label: 'settings.menu.data',     icon: markRaw(Folder),   group: 2 },
  { key: 'general',  label: 'settings.menu.general',  icon: markRaw(Setting),  group: 2 },
  { key: 'terminal', label: 'settings.menu.terminal', icon: markRaw(Monitor),  group: 2 },
  { key: 'host',     label: 'settings.menu.host',     icon: markRaw(GlobeIcon),group: 2 },
  { key: 'shortcut', label: 'settings.menu.shortcut', icon: markRaw(Key),      group: 2 },
  { key: 'preference', label: 'settings.menu.preference', icon: markRaw(Operation), group: 2 },
  // 扩展
  { key: 'appearance', label: 'settings.menu.appearance', icon: markRaw(Brush),        group: 3 },
  { key: 'language',   label: 'settings.menu.language',   icon: markRaw(GlobeIcon),    group: 3 },
  // 系统
  { key: 'security',   label: 'settings.menu.security',   icon: markRaw(Lock),         group: 4 },
  { key: 'beta',       label: 'settings.menu.beta',       icon: markRaw(MagicStick),   group: 4 },
  { key: 'about',      label: 'settings.menu.about',      icon: markRaw(InfoCardIcon), group: 4 },
]

const active = ref<MenuKey>((app.settingsInitialTab as MenuKey) || 'account')

// ===== 移动端分级导航（桌面端不使用）=====
// 移动端交互约定：一级 = 设置分组列表页，点击进入二级详情页（带返回键），
// 不使用顶部 Tab。桌面端仍为左侧菜单，以下状态在桌面端不参与渲染。
const isMobile = isMobileWebview()
/** 是否支持生物识别解锁（仅移动端）：为 false 时整个「启用生物识别解锁」设置项不显示 */
const canUseBiometric = CAP.biometricUnlock()
// 初始落点按入口 tab 计算：弹窗内容是打开时才挂载的，挂载瞬间 settingsVisible
// 已经是 true，watch 不会对初始值触发 → 必须用初始值兜底，否则永远落在首页
const mobileAtRoot = ref<boolean>(isMobile ? app.settingsInitialTab === 'general' : true)
/** 移动端打开时是否「跳过首页直达二级页」（从「我的」的账户/主机/关于入口进来）。
 *  决定二级页返回键行为：跳过过首页 → 直接回「我的」；经过首页 → 回首页 */
const mobileSkipRoot = ref(isMobile && app.settingsInitialTab !== 'general')
/** 移动端导航过渡方向：'slide-fwd' = 进入下级（新页从右往左进入、旧页向左退出）；
 *  'slide-back' = 返回（旧页向左往右退出、目标页从左进入） */
const mobileTrans = ref<'slide-fwd' | 'slide-back'>('slide-fwd')
function openMobileSection(key: MenuKey): void {
  mobileTrans.value = 'slide-fwd'
  active.value = key
  mobileAtRoot.value = false
  mobileSkipRoot.value = false // 从首页点进去的，返回仍回首页
}
function backToMobileRoot(): void {
  mobileTrans.value = 'slide-back'
  mobileAtRoot.value = true
}
const dockStore = useDockStore()
/** 移动端一级设置页的返回键：设置是覆盖在「我的」之上的整页弹窗，
 *  回到「我的」页（先把前台页切到 profile，再关掉设置弹窗本身） */
function backToProfile(): void {
  dockStore.setActivePage('profile')
  void closeSettingsSelf()
}
/** 移动端返回键统一入口：按「是否经过首页」决定回首页还是回「我的」 */
function mobileBack(): void {
  if (mobileAtRoot.value || mobileSkipRoot.value) backToProfile()
  else backToMobileRoot()
}

watch(() => app.settingsInitialTab, (v) => {
  // 弹窗关闭时 MainWindow 会把 initialTab 复位成 'general'；
  // 此时关闭动画还在播，若跟着切 active 会在淡出中闪现「通用」页，故仅在打开时跟随
  if (v && app.settingsVisible) active.value = v as MenuKey
})

// 每次打开弹窗按入口复位落点：'general' = 从「我的」点「设置」→ 首页；
// 其余 tab = 从「我的」直达对应二级页，返回键直接回「我的」。
// immediate：组件可能在 visible 已为 true 时才挂载（弹窗内容懒渲染），需立即对齐一次
watch(() => app.settingsVisible, (visible) => {
  if (!isMobile || !visible) return
  mobileAtRoot.value = app.settingsInitialTab === 'general'
  mobileSkipRoot.value = !mobileAtRoot.value
  if (!mobileAtRoot.value) active.value = app.settingsInitialTab as MenuKey
}, { immediate: true })

const groupBoundary = (idx: number, list: MenuItem[]) =>
  idx > 0 && list[idx].group !== list[idx - 1].group

/** 移动端菜单：快捷键设置仅桌面有意义（手机无实体键盘），不渲染该入口 */
const menuItems = computed<MenuItem[]>(() =>
  isMobile ? MENU_ITEMS.filter((m) => m.key !== 'shortcut') : MENU_ITEMS,
)

// ===== 公共（复制走统一入口，受「剪贴板自动清除」安全设置约束） =====
function copy(val: string, label?: string) {
  copyText(val)
    .then(() => ElMessage.success(label ? label + ' ' + t('common.copied') : t('common.copied')))
    .catch(() => ElMessage.warning(t('common.copyFail')))
}

// ===== 账户 =====
const isLoggedIn = computed(() => auth.state.status === 'authenticated')
const user = computed(() => (auth.state.status === 'authenticated' ? auth.state.user : null))
function phoneMasked(v?: string | null) {
  if (!v) return '-'
  const d = String(v).replace(/\D/g, '')
  return d.length < 7 ? d : d.slice(0, 3) + '****' + d.slice(-4)
}

/** 打开个人中心（管理账户 / 编辑资料） */
async function openProfile(menu?: string) {
  try {
    await invoke('open_profile', menu ? { menu } : undefined)
  } catch (e: any) {
    handleApiError(e, 'components/AppSettingsDialog.openProfile', {
      prefix: t('common.operationFail'),
    })
  }
}

/** 手动从云端拉取并合并配置 */
async function syncPull() {
  if (sync.busy) return
  try {
    const ok = await sync.manualPull()
    if (ok) ElMessage.success(t('settings.account.status.sync.success'))
    else if (sync.lastError) ElMessage.warning(sync.lastError)
  } catch (e: any) {
    handleApiError(e, 'components/AppSettingsDialog.syncPull', {
      prefix: t('common.operationFail'),
    })
  }
}

/** 手动把本地配置强制推送到云端 */
async function syncPush() {
  if (sync.busy) return
  try {
    const ok = await sync.manualPush()
    if (ok) ElMessage.success(t('settings.account.status.sync.success'))
    else if (sync.lastError) ElMessage.warning(sync.lastError)
  } catch (e: any) {
    handleApiError(e, 'components/AppSettingsDialog.syncPush', {
      prefix: t('common.operationFail'),
    })
  }
}

/** 自动同步开关变更 */
function onAutoSyncChange(v: boolean | string | number) {
  if (v) {
    void sync.enableAutoSync()
  } else {
    sync.disableAutoSync()
  }
}

/** 上次同步时间友好文本 */
const lastSyncText = computed(() => {
  const ts = sync.lastSyncAt
  if (!ts) return ''
  try {
    return new Date(ts).toLocaleString(i18n.locale === 'zh-CN' ? 'zh-CN' : 'en-US')
  } catch {
    return ''
  }
})

// ===== 数据管理（导入导出/备份/清缓存/数据目录） =====
const dataDir = ref('')
function stamp(): string {
  const d = new Date()
  const p = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}_${p(d.getHours())}${p(d.getMinutes())}`
}
/** 弹出系统「另存为」对话框导出 JSON（用户自选保存位置与文件名） */
async function saveJsonAs(defaultName: string, payload: unknown, title: string, doneMsg: string): Promise<void> {
  try {
    const path = await invoke<string | null>('pick_save_path', { title, defaultName })
    if (!path) return // 用户取消
    await invoke('save_text_file', { path, content: JSON.stringify(payload, null, 2) })
    ElMessage.success(doneMsg)
  } catch (e: any) {
    handleApiError(e, 'components/AppSettingsDialog.saveJsonAs', {
      prefix: t('common.operationFail'),
    })
  }
}
async function exportHosts(): Promise<void> {
  await saveJsonAs(`jc-shell-hosts-${stamp()}.json`, {
    app: 'jc-shell', type: 'hosts', version: 1,
    exportedAt: new Date().toISOString(),
    data: hostStore.snapshot(),
  }, t('settings.data.host.export'), t('settings.data.host.exportDone'))
}
const importInputRef = ref<HTMLInputElement | null>(null)
function importHosts(): void { importInputRef.value?.click() }
async function onImportFile(e: Event): Promise<void> {
  const input = e.target as HTMLInputElement
  const file = input.files && input.files[0]
  input.value = ''
  if (!file) return
  try {
    const obj = JSON.parse(await file.text())
    const data = obj && obj.data ? obj.data : obj
    if (!data || !Array.isArray(data.hosts)) throw new Error('invalid format')
    // 同 id 覆盖、新 id 追加；命名空间取并集
    const map = new Map(hostStore.hosts.map(h => [h.id, h]))
    let added = 0, updated = 0
    for (const h of data.hosts as HostItem[]) {
      if (!h || !h.id || !h.type) continue
      const exist = map.get(h.id)
      if (exist) { Object.assign(exist, h); updated++ } else { hostStore.hosts.push(h); added++ }
    }
    if (Array.isArray(data.namespaces)) {
      for (const ns of data.namespaces as Namespace[]) {
        if (ns && ns.id && !hostStore.namespaces.some(n => n.id === ns.id)) hostStore.namespaces.push(ns)
      }
    }
    hostStore.persist()
    ElMessage.success(t('settings.data.host.importDone', { added, updated }))
  } catch {
    ElMessage.error(t('settings.data.host.importFail'))
  }
}
async function backupDb(): Promise<void> {
  try {
    const rows = await invoke<unknown[]>('list_all_settings')
    await saveJsonAs(`jc-shell-backup-${stamp()}.json`, {
      app: 'jc-shell', type: 'db-backup', version: 1,
      exportedAt: new Date().toISOString(),
      settings: rows,
    }, t('settings.data.backup.title'), t('settings.data.backup.done'))
  } catch {
    ElMessage.error(t('common.operationFail'))
  }
}
function clearCache() {
  ElMessageBox.confirm(t('settings.data.clearCache.confirm'), t('settings.data.clearCache.title'), { type: 'warning' })
    .then(() => {
      aiStore.clear()
      ElMessage.success(t('settings.data.clearCache.done'))
    }).catch(() => {})
}
async function openDataDir(): Promise<void> {
  try {
    const dir = dataDir.value || (await invoke<string>('get_data_dir'))
    await openInOs(dir)
  } catch {
    ElMessage.error(t('common.operationFail'))
  }
}
onMounted(async () => {
  try { dataDir.value = await invoke<string>('get_data_dir') } catch { /* 保留默认提示 */ }
  if (!hostStore.loaded) hostStore.load().catch(() => {})
  knownCount.value = listKnownHosts().length
  await refreshSec()
})

// ===== 快捷键（可编辑，按功能分组；组合键持久化在全局设置） =====
interface ShortcutItem { key: string; labelKey: string; combo: string }
interface ShortcutGroup { id: string; titleKey: string; items: ShortcutItem[] }
function comboOf(key: string): string { return settings.shortcuts[key] || SHORTCUT_DEFAULT[key] || '' }
const shortcutGroups = ref<ShortcutGroup[]>([
  {
    id: 'tab', titleKey: 'settings.shortcut.group.tab',
    items: [
      { key: 'newTab',   labelKey: 'settings.shortcut.newTab',   combo: comboOf('newTab') },
      { key: 'closeTab', labelKey: 'settings.shortcut.closeTab', combo: comboOf('closeTab') },
      { key: 'nextTab',  labelKey: 'settings.shortcut.nextTab',  combo: comboOf('nextTab') },
      { key: 'prevTab',  labelKey: 'settings.shortcut.prevTab',  combo: comboOf('prevTab') },
    ],
  },
  {
    id: 'view', titleKey: 'settings.shortcut.group.view',
    items: [
      { key: 'toggleAi',      labelKey: 'settings.shortcut.toggleAi',      combo: comboOf('toggleAi') },
      { key: 'toggleAgent',   labelKey: 'settings.shortcut.toggleAgent',   combo: comboOf('toggleAgent') },
      { key: 'toggleSidebar', labelKey: 'settings.shortcut.toggleSidebar', combo: comboOf('toggleSidebar') },
      { key: 'settings',      labelKey: 'settings.shortcut.settings',      combo: comboOf('settings') },
    ],
  },
  {
    id: 'security', titleKey: 'settings.shortcut.group.security',
    items: [
      { key: 'lock', labelKey: 'settings.shortcut.lock', combo: comboOf('lock') },
    ],
  },
  {
    id: 'split', titleKey: 'settings.shortcut.group.split',
    items: [
      { key: 'splitPane',      labelKey: 'settings.shortcut.splitPane',      combo: comboOf('splitPane') },
      { key: 'splitHorizontal', labelKey: 'settings.shortcut.splitHorizontal', combo: comboOf('splitHorizontal') },
      { key: 'splitVertical',  labelKey: 'settings.shortcut.splitVertical',  combo: comboOf('splitVertical') },
      { key: 'closePane',      labelKey: 'settings.shortcut.closePane',      combo: comboOf('closePane') },
    ],
  },
  {
    id: 'editor', titleKey: 'settings.shortcut.group.editor',
    items: [
      { key: 'editorSave',    labelKey: 'settings.shortcut.editorSave',    combo: comboOf('editorSave') },
      { key: 'editorFind',    labelKey: 'settings.shortcut.editorFind',    combo: comboOf('editorFind') },
      { key: 'editorReplace', labelKey: 'settings.shortcut.editorReplace', combo: comboOf('editorReplace') },
      { key: 'editorFormat',  labelKey: 'settings.shortcut.editorFormat',  combo: comboOf('editorFormat') },
    ],
  },
])
/** 扁平视图：冲突检测/录入/重置均在全量列表上进行 */
const shortcutList = computed<ShortcutItem[]>(() => shortcutGroups.value.flatMap(g => g.items))
const recordingKey = ref<string | null>(null)
const shortcutConflict = ref<string | null>(null)

function startRecord(k: string) {
  recordingKey.value = k
  shortcutConflict.value = null
}
function onShortcutKeydown(e: KeyboardEvent) {
  if (!recordingKey.value) return
  e.preventDefault()
  if (['Control', 'Shift', 'Alt', 'Meta'].includes(e.key)) return
  const parts: string[] = []
  if (e.ctrlKey) parts.push('Ctrl')
  if (e.shiftKey) parts.push('Shift')
  if (e.altKey) parts.push('Alt')
  if (e.metaKey) parts.push('Meta')
  let k = e.key
  if (k === ' ') k = 'Space'
  if (k.length === 1) k = k.toUpperCase()
  parts.push(k)
  const combo = parts.join('+')
  const target = recordingKey.value
  const dup = shortcutList.value.find((s) => s.key !== target && s.combo === combo)
  if (dup) {
    shortcutConflict.value = combo
    ElMessage.warning(t('settings.shortcut.conflict', { name: t(dup.labelKey) }))
    return
  }
  const item = shortcutList.value.find((s) => s.key === target)
  if (item) item.combo = combo
  recordingKey.value = null
  shortcutConflict.value = null
}
function resetShortcuts() {
  shortcutList.value.forEach((s) => { if (SHORTCUT_DEFAULT[s.key]) s.combo = SHORTCUT_DEFAULT[s.key] })
  recordingKey.value = null
  shortcutConflict.value = null
}
/** 当前是否 macOS（沿用 DockLayout.vue 的平台判断方式） */
const isMac = /Mac/i.test(navigator.userAgent) && !/Windows/i.test(navigator.userAgent)
/**
 * 按操作系统渲染组合键：
 * - Mac 显示 ⌘⌥⇧⌃（Meta 即 Command，⌃ 为 Control）
 * - Windows 显示 Ctrl/Alt/Shift/Win（Meta 即 Win 键）
 * 存储仍用跨平台 token（Ctrl/Shift/Alt/Meta），仅显示层转换，不影响匹配逻辑。
 */
function displayCombo(combo: string): string {
  if (!combo) return ''
  const parts = combo.split('+')
  if (isMac) {
    const map: Record<string, string> = { Ctrl: '⌃', Shift: '⇧', Alt: '⌥', Meta: '⌘' }
    return parts.map((p) => map[p] ?? p).join('')
  }
  const map: Record<string, string> = { Ctrl: 'Ctrl', Shift: 'Shift', Alt: 'Alt', Meta: 'Win' }
  return parts.map((p) => map[p] ?? p).join('+')
}
/** 录入框失焦（点了别处但没按键）→ 取消录入，原样恢复，不写任何值 */
function cancelRecord() {
  recordingKey.value = null
  shortcutConflict.value = null
}
onMounted(() => window.addEventListener('keydown', onShortcutKeydown))
onUnmounted(() => window.removeEventListener('keydown', onShortcutKeydown))

// 组合键修改 → 写入全局设置（自动持久化 + 跨窗口同步）
watch(shortcutList, () => {
  const next: Record<string, string> = {}
  shortcutList.value.forEach((s) => { next[s.key] = s.combo })
  prefs.patchSettings({ shortcuts: next })
}, { deep: true })
// 其他窗口改了快捷键 → 同步到本地列表（仅赋值，上面的 watch 会写回相同值，无环）
watch(() => settings.shortcuts, () => {
  shortcutList.value.forEach((s) => {
    const v = settings.shortcuts[s.key]
    if (v && v !== s.combo) s.combo = v
  })
}, { deep: true })

// ===== 主机与 SSH（已知主机：连接成功过的地址记录在本地） =====
const knownCount = ref<number>(listKnownHosts().length)
function clearKnownHosts() {
  ElMessageBox.confirm(t('settings.host.known.clearConfirm'), t('settings.host.known.title'), { type: 'warning' })
    .then(() => {
      clearKnownHostsUtil()
      knownCount.value = 0
      ElMessage.success(t('settings.host.known.clearDone'))
    }).catch(() => {})
}

// ===== 外观（主题/强调色/终端背景色直接写 app store，实时生效） =====
const themeForm = computed<ThemeMode>({
  get: () => app.themeMode,
  set: (v) => app.setTheme(v)
})
const accentColorSwatch = computed<string>({
  get: () => app.accentColor,
  set: (v) => app.setAccent(v)
})
function pickRandomAccent() {
  app.randomizeAccent()
}

// ===== 语言（切换即生效） =====
const localeForm = computed<Locale>({
  get: () => i18n.locale,
  set: (v) => i18n.setLocale(v)
})

// ===== 安全：统一安全密码（本地保护） =====
/** 当前安全密码状态（后端为唯一真相源，前端只做展示与触发） */
const secStatus = ref<SecurityStatus>({
  has_password: false,
  scopes: { view_password: true, startup: false, edit_delete: false, export_data: false, export_password: true },
  locked: false,
  lock_remaining_sec: 0,
  biometric_supported: false,
  biometric_enabled: false,
  biometric_mode: 'both',
})
/** 用户勾选的保护范围（未启用时本地暂存，启用时一并提交）；export_password 始终开启 */
const scopesForm = reactive<SecurityScopes>({
  view_password: true, startup: false, edit_delete: false, export_data: false, export_password: true,
})

async function refreshSec(): Promise<void> {
  try {
    const st = await securityStatus()
    // eslint-disable-next-line no-console
    console.log('[security] status', st)
    secStatus.value = st
    if (st.has_password) {
      scopesForm.view_password = true
      scopesForm.startup = st.scopes.startup
      scopesForm.edit_delete = st.scopes.edit_delete
      scopesForm.export_data = st.scopes.export_data
      scopesForm.export_password = true // 始终开启
    }
  } catch (e) {
    // eslint-disable-next-line no-console
    console.error('[security] status failed', e)
  }
  await refreshBio()
}

// ===== 生物识别解锁（仅移动端）=====
/** 本机生物识别当前是否可用（硬件就绪 + 已录入指纹/人脸） */
const bioReady = ref(false)
/** 有硬件但未录入：提示要区别于「不可用」，引导用户去系统设置录入 */
const bioNotEnrolled = ref(false)
/** 本机确实具备哪些模态（仅用于提示，不用于裁剪可选项） */
const bioKinds = ref<BiometricKind[]>([])
/** 原生扩展是否生效（Android 上拿不到模态列表就说明跑的是未打补丁的原生代码） */
const bioExtActive = ref(false)
/** 本机能否做到「只认指纹」的精确限制（走原生 FingerprintManager 通道） */
const bioFingerprintOnly = ref(false)

/** 问一次系统生物识别状态（桌面端直接跳过，不发 IPC） */
async function refreshBio(): Promise<void> {
  if (!canUseBiometric) {
    bioReady.value = false
    bioNotEnrolled.value = false
    bioKinds.value = []
    return
  }
  const { available, notEnrolled, kinds, extensionActive, fingerprintOnly } = await biometricAvailability()
  bioReady.value = available
  bioNotEnrolled.value = notEnrolled
  bioKinds.value = kinds
  bioExtActive.value = extensionActive
  bioFingerprintOnly.value = fingerprintOnly
}

/**
 * 指纹 / 人脸两个**独立开关**（用户诉求：可只开一个、也可都开，而不是三选一）。
 * 两种开关的组合正好映射后端的三种方式：
 *   指纹✕ 人脸✓ → 'face'；指纹✓ 人脸✕ → 'fingerprint'；都✓ → 'both'。
 * 刻意**不按探测结果裁剪选项** —— 容器/机型的能力上报并不可靠（而且上报不到不代表不能用），
 * 屏蔽掉选项只会让用户以为功能没了。
 */
const bioFingerprintOn = computed(() => secStatus.value.biometric_mode !== 'face')
const bioFaceOn = computed(() => secStatus.value.biometric_mode !== 'fingerprint')

/** 本机识别能力提示：它同时是「原生扩展是否生效」的现场自证信号（见 utils/biometric.ts） */
const bioCapHint = computed(() => {
  const names = bioKinds.value.map((k) => t(k === 'fingerprint'
    ? 'settings.security.password.biometric.kind.fingerprint'
    : 'settings.security.password.biometric.kind.face'))
  if (names.length) return t('settings.security.password.biometric.detected', { kinds: names.join(' / ') })
  return bioExtActive.value
    ? t('settings.security.password.biometric.detectedNone')
    : t('settings.security.password.biometric.detectedUnknown')
})

/**
 * 选了「只开指纹」但本机没有可用的指纹专用通道时，明确告知会回落为系统验证
 * （否则用户会以为「设置没生效」，实际是平台限制）。
 */
const bioFpOnlyHint = computed(() => {
  if (secStatus.value.biometric_mode !== 'fingerprint') return ''
  if (!bioReady.value || bioFingerprintOnly.value) return ''
  return t('settings.security.password.biometric.fingerprintOnlyUnsupported')
})

/** 该设置项的说明文案：按「未设密码 / 未录入 / 不可用 / 正常」四态给出可行动信息 */
const bioHint = computed(() => {
  if (!secStatus.value.has_password) return t('settings.security.password.biometric.needPassword')
  if (bioNotEnrolled.value) return t('settings.security.password.biometric.notEnrolled')
  if (!bioReady.value) return t('settings.security.password.biometric.unavailable')
  return t('settings.security.password.biometric.hint')
})

/** 开启 / 关闭生物识别解锁：两者都必须先验证安全密码（后端会再校验一次 PIN 哈希） */
async function onBiometricToggle(v: boolean | string | number): Promise<void> {
  const val = !!v
  if (!secStatus.value.has_password) {
    ElMessage.warning(t('settings.security.password.biometric.needPassword'))
    return
  }
  const r = await openVerify({
    message: val
      ? t('settings.security.password.biometric.confirmEnable')
      : t('settings.security.password.biometric.confirmDisable'),
    showWipe: false,
    title: t('settings.security.password.biometric.title'),
  })
  if (r.action !== 'ok') { await refreshSec(); return }
  try {
    // 开启时带上当前选中的方式；关闭时不关心方式
    await securitySetBiometric(r.pin, val, val ? secStatus.value.biometric_mode : undefined)
    ElMessage.success(val
      ? t('settings.security.password.biometric.enabled')
      : t('settings.security.password.biometric.disabled'))
  } catch (e: any) {
    handleApiError(e, 'components/AppSettingsDialog.onBiometricToggle', { prefix: t('settings.security.password.biometric.fail') })
  } finally {
    // 开关绑的是后端状态，失败时这里会把它拉回真实值，不会停在乐观翻转态
    await refreshSec()
  }
}

/** 切换指纹 / 人脸：与总开关一样必须验证安全密码（后端会再比对一次 PIN 哈希） */
async function onBiometricKindToggle(kind: BiometricKind, v: boolean | string | number): Promise<void> {
  const on = !!v
  const useFp = kind === 'fingerprint' ? on : bioFingerprintOn.value
  const useFace = kind === 'face' ? on : bioFaceOn.value
  // 两个都关等于关掉生物识别本身，不允许在这里「假关闭」，要关请用上面的总开关
  if (!useFp && !useFace) {
    ElMessage.warning(t('settings.security.password.biometric.atLeastOne'))
    return
  }
  const mode: BiometricMode = useFp && useFace ? 'both' : useFp ? 'fingerprint' : 'face'
  if (mode === secStatus.value.biometric_mode) return
  const r = await openVerify({
    message: t('settings.security.password.biometric.confirmMode'),
    showWipe: false,
    title: t('settings.security.password.biometric.title'),
  })
  if (r.action !== 'ok') { await refreshSec(); return }
  try {
    await securitySetBiometric(r.pin, true, mode)
    ElMessage.success(t('settings.security.password.biometric.modeUpdated'))
  } catch (e: any) {
    handleApiError(e, 'components/AppSettingsDialog.onBiometricKindToggle', { prefix: t('settings.security.password.biometric.fail') })
  } finally {
    // 开关值是后端状态派生的，失败时这里会把它拉回真实值，不会停在乐观翻转态
    await refreshSec()
  }
}

/** 启用 / 关闭安全密码：开启需设密码（两次校验）；关闭需先验证当前密码，或「抹除会话数据并关闭」 */
async function onSecurityEnableToggle(v: boolean | string | number): Promise<void> {
  if (v) {
    const r = await openSetPassword({ ...scopesForm })
    if (!r) { refreshSec(); return }
    try {
      await securitySetPassword(r.pin, { ...scopesForm })
      ElMessage.success(t('settings.security.password.enabled'))
      // 乐观更新：先让开关亮起，再异步对表（避免凭据库读取抖动导致 UI 回弹）
      secStatus.value.has_password = true
      await refreshSec()
    } catch (e: any) {
      handleApiError(e, 'components/AppSettingsDialog.onSecurityEnableToggle', { prefix: '设置安全密码失败' })
      refreshSec()
    }
  } else {
    const r = await openVerify({
      message: t('settings.security.password.disableConfirm'),
      showWipe: true,
      title: t('settings.security.password.disableTitle'),
    })
    if (r.action === 'cancel') { refreshSec(); return }
    if (r.action === 'wipe') {
      try {
        await securityWipeAndDisable()
        ElMessage.success(t('settings.security.password.wiped'))
        // 会话数据已被后端清空，重新加载内存（现在为空列表）
        if (hostStore.loaded) await hostStore.load()
        await refreshSec()
      } catch (e: any) {
        handleApiError(e, 'components/AppSettingsDialog.onSecurityEnableToggle', { prefix: '清除失败' })
      }
      return
    }
    // action === 'ok'：验证通过才允许关闭
    try {
      await securityDisable(r.pin)
      ElMessage.success(t('settings.security.password.disabled'))
    } catch (e: any) {
      handleApiError(e, 'components/AppSettingsDialog.onSecurityEnableToggle', { prefix: '关闭失败' })
    } finally {
      await refreshSec()
    }
  }
}

/** 修改保护范围：已启用时必须先验证当前密码，通过后才落库（验证前不改 UI，杜绝先翻转再撤销的闪烁） */
async function onScopeChange(key: 'startup' | 'edit_delete' | 'export_data', v: boolean | string | number): Promise<void> {
  const val = !!v
  // 尚未启用：仅本地暂存，启用时一并提交（无需验证）
  if (!secStatus.value.has_password) {
    scopesForm[key] = val
    return
  }
  // 已启用：先验证当前密码，验证通过才应用本次修改
  const pw = await openVerify({
    message: t('settings.security.password.scopeConfirm'),
    showWipe: false,
    title: '修改保护范围',
  })
  if (pw.action === 'cancel') return
  scopesForm[key] = val
  try {
    await securityUpdateScopes(pw.pin, { ...scopesForm })
    ElMessage.success(t('settings.security.password.scopesUpdated'))
  } catch (e: any) {
    handleApiError(e, 'components/AppSettingsDialog.onScopeChange', { prefix: '修改保护范围失败' })
  } finally {
    await refreshSec()
  }
}

/** 切出应用时自动锁定：已启用安全密码时需先验证，通过后才落库 */
async function onLockOnAppSwitchChange(v: boolean | string | number): Promise<void> {
  const val = !!v
  // 尚未启用安全密码：直接切换，无需验证
  if (!secStatus.value.has_password) {
    settings.lockOnAppSwitch = val
    return
  }
  const pw = await openVerify({
    message: t('settings.security.session.lockConfirm'),
    showWipe: false,
    title: '修改自动锁定',
  })
  if (pw.action === 'cancel') return
  settings.lockOnAppSwitch = val
}
/** 凭证加密开关变更 → 立即按新策略重新落盘会话数据 */
function onEncryptToggle(v: boolean | string | number): void {
  if (hostStore.loaded) {
    hostStore.persist()
    ElMessage.info(v ? '凭证加密已开启，会话数据将加密保存' : '凭证加密已关闭，会话数据将明文保存')
  }
}

// ===== 实验性 / 安全：均绑定全局设置 =====

// ===== 关于 =====
const __ver: unknown = __APP_VERSION__
const version = ref(
  typeof __ver === 'string' && __ver.trim()
    ? __ver.trim()
    : (typeof (import.meta.env as any).VITE_APP_VERSION === 'string' && String((import.meta.env as any).VITE_APP_VERSION).trim())
      ? String((import.meta.env as any).VITE_APP_VERSION).trim()
      : '0.1.0'
)
const build = ref('20260822.1')
const website = ref('https://www.openjc.cc/jc-shell')
const docsUrl = ref('https://docs.openjc.cc')
const feedbackUrl = ref('https://github.com/openjc/jc-shell/issues')
const sourceUrl = ref('https://gitee.com/Open-JC/jc-shell-community')
const checking = ref(false)
/** 检查更新：走 Rust UpdateService（manual 模式必弹窗，与所有入口一致；不再用 tip 提示） */
async function checkUpdate(): Promise<void> {
  if (checking.value) return
  checking.value = true
  try {
    await useUpdateStore().checkUpdate('manual')
  } catch {
    // checkUpdate 内部已设置 errorMsg 并弹窗，这里不再显示额外 tip
  } finally {
    checking.value = false
  }
}

// ===== 恢复默认设置（通用 → 重置设置） =====
/** 二次确认后把所有设置项恢复默认；会话/主机数据不受影响 */
async function onResetSettings(): Promise<void> {
  try {
    await ElMessageBox.confirm(t('settings.general.resetConfirm'), t('settings.general.reset'), {
      type: 'warning',
      confirmButtonText: t('common.confirm'),
      cancelButtonText: t('common.cancel'),
    })
  } catch {
    return
  }
  prefs.resetAll()
  // 终端自定义配色一并回到默认（纯黑底 / 白字），否则重置后仍保留旧配色会显得没生效
  app.setTerminalBg('')
  app.setTerminalFg('')
  ElMessage.success(t('settings.general.resetDone'))
}

// ===== 确认/关闭 =====
/**
 * 本组件同时被两种宿主复用，关闭时必须只关掉「自己所在的那一层」：
 *   1) 独立设置窗口（label=settings，由 open_settings 创建）→ 关闭该窗口
 *   2) 主窗口内的 el-dialog（openSettings 的降级路径）→ 收起弹窗
 * 不能无脑 getCurrentWindow().close()，否则在宿主 2 中关掉的是主窗口
 * （开了「最小化到托盘」时表现为主窗口被最小化）。
 */
const SETTINGS_WINDOW_LABEL = 'settings'
function isStandaloneSettingsWindow(): boolean {
  try { return getCurrentWindow().label === SETTINGS_WINDOW_LABEL } catch { return false }
}

/**
 * 独立设置窗口（label=settings）时，把顶部区域标记为 Tauri 拖拽区，
 * 让该窗口可以被鼠标拖动；主窗口内 el-dialog 形态不需要（由 dialog 自身 draggable 处理）。
 */
const isStandaloneWindow = ref(false)
onMounted(() => { isStandaloneWindow.value = isStandaloneSettingsWindow() })
/** 关闭自己所在的那一层，不影响主窗口 */
async function closeSettingsSelf(): Promise<void> {
  if (isStandaloneSettingsWindow()) {
    try { await getCurrentWindow().close(); return } catch { /* 非 Tauri / 关闭失败 → 降级 */ }
  }
  app.closeSettings()
}
/** 所有设置修改即实时生效并自动持久化；确认后仅关闭设置面板自身 */
async function confirmAll(): Promise<void> {
  prefs.flush()
  await closeSettingsSelf()
}

const userNameDisplay = computed(() => {
  if (auth.state.status !== 'authenticated') return t('user.notLoggedIn')
  const u = auth.state.user
  return (u.nickname && String(u.nickname).trim()) || u.username || t('user.notLoggedIn')
})
const userInitial = computed(() => {
  const s = userNameDisplay.value
  return s ? s.slice(0, 1).toUpperCase() : '?'
})
</script>

<template>
  <div class="settings-root h-full w-full flex flex-col bg-[var(--jc-bg)] overflow-hidden">
    <div class="settings-body flex flex-1 min-h-0">
      <!-- ========== 左侧菜单 ========== -->
      <aside class="settings-sidebar shrink-0 w-[220px] h-full flex flex-col border-r border-[var(--jc-border)] bg-[var(--jc-bg-soft)]">
        <div class="sidebar-account" :data-tauri-drag-region="isStandaloneWindow ? '' : null">
          <div class="avatar-circle" :style="{ background: app.accentColor }">{{ userInitial }}</div>
          <div class="account-meta">
            <div class="account-name">{{ userNameDisplay }}</div>
            <div class="account-plan">
              <el-tag v-if="isLoggedIn" size="small" type="success" effect="plain">{{ t('settings.account.plan.free') }}</el-tag>
              <span v-else class="plan-muted">{{ t('settings.account.plan.anonymous') }}</span>
            </div>
          </div>
        </div>

        <nav class="menu-list flex-1 overflow-y-auto px-2 pb-2">
          <template v-for="(m, idx) in menuItems" :key="m.key">
            <div v-if="groupBoundary(idx, menuItems)" class="menu-divider" />
            <button
              class="menu-item"
              :class="{ 'is-active': active === m.key }"
              @click="active = m.key"
            >
              <el-icon :size="15"><component :is="m.icon" /></el-icon>
              <span>{{ t(m.label) }}</span>
            </button>
          </template>
        </nav>
      </aside>

      <!-- ========== 右侧内容 ========== -->
      <main class="settings-main flex-1 min-w-0 h-full flex flex-col">
        <!-- 移动端始终显示头部：二级页带返回键（回一级列表），
            一级页也带返回键（回「我的」页）；桌面端保持原样 -->
        <header class="settings-main-header shrink-0" :data-tauri-drag-region="isStandaloneWindow ? '' : null">
          <!-- 移动端：一级页返回「我的」、二级页返回一级列表，标题分别为「设置」/ 分区名 -->
          <button v-if="isMobile" class="m-set-back" @click="mobileBack">
            <el-icon><ArrowLeft /></el-icon>
          </button>
          <h1 class="content-title">
            {{ isMobile
              ? (mobileAtRoot ? t('settings.dialog.title') : t(MENU_ITEMS.find((m) => m.key === active)!.label))
              : t('settings.dialog.title') + ' · ' + t(MENU_ITEMS.find((m) => m.key === active)!.label) }}
          </h1>
        </header>
        <div class="settings-main-content flex-1 overflow-y-auto pr-1">
          <!-- 移动端分级导航过渡：key 在 一级/二级 间切换时触发滑动；
               桌面端 key 恒为 'static'，不触发过渡，保持原左侧菜单切换行为 -->
          <Transition :name="mobileTrans">
            <div :key="isMobile ? (mobileAtRoot ? 'root' : active) : 'static'" class="m-set-pane">

          <!-- 移动端一级页：设置分组列表（分级导航，点击进入二级页；不用顶部 Tab） -->
          <div v-if="isMobile && mobileAtRoot" class="m-set-root">
            <button
              v-for="(m, i) in menuItems"
              :key="m.key"
              class="m-set-root__cell"
              :class="{ 'is-new-group': groupBoundary(i, menuItems) }"
              @click="openMobileSection(m.key)"
            >
              <el-icon class="m-set-root__icon"><component :is="m.icon" /></el-icon>
              <span class="m-set-root__label">{{ t(m.label) }}</span>
              <el-icon v-if="!m.leaf" class="m-set-root__arrow"><ArrowRight /></el-icon>
            </button>
          </div>

          <template v-else>

          <!-- ========== 账户 ========== -->
          <section v-if="active === 'account'" class="content-inner">
            <div class="section">
              <div class="section-title">{{ t('settings.account.info.title') }}</div>
              <div v-if="!isLoggedIn" class="anon-card">
                <p>{{ t('settings.account.status.anonymous') }}</p>
                <label class="pc-consent">
                  <input v-model="pcAgreed" type="checkbox" class="pc-consent__cb" />
                  <span class="pc-consent__text">
                    {{ t('consent.agreePrefix') }}
                    <a href="#" class="pc-consent__link" @click.prevent="openLegalLink(uaUrl)">{{ t('consent.userAgreement') }}</a>
                    {{ t('consent.agreeAnd') }}
                    <a href="#" class="pc-consent__link" @click.prevent="openLegalLink(ppUrl)">{{ t('consent.privacyPolicy') }}</a>
                  </span>
                </label>
                <div style="margin-top: 12px">
                  <el-button type="primary" :disabled="!pcAgreed" @click="onPcLogin">
                    {{ t('user.menu.login') }}
                  </el-button>
                </div>
              </div>
              <div v-else class="info-card">
                <div class="info-row">
                  <div>
                    <div class="info-row-title">{{ user?.id }}</div>
                    <div class="info-row-sub">{{ phoneMasked(user?.phone) }}</div>
                  </div>
                  <div class="info-row-actions">
                    <el-button plain @click="openProfile('account')">{{ t('settings.account.info.manage') }}</el-button>
                    <el-dropdown trigger="click">
                      <el-button plain><el-icon><Setting /></el-icon></el-button>
                      <template #dropdown>
                        <el-dropdown-menu>
                          <el-dropdown-item @click="openProfile('account')">{{ t('settings.account.info.editProfile') }}</el-dropdown-item>
                          <el-dropdown-item divided @click="openProfile('account')">{{ t('settings.account.info.bindPhone') }}</el-dropdown-item>
                        </el-dropdown-menu>
                      </template>
                    </el-dropdown>
                  </div>
                </div>
                <div class="info-row">
                  <div>
                    <div class="info-row-title">{{ t('settings.account.plan.free') }}</div>
                    <div class="info-row-sub">{{ t('settings.account.plan.upgradeHint') }}</div>
                  </div>
                </div>
              </div>
            </div>

            <div class="section" v-if="isLoggedIn">
              <div class="section-title">{{ t('settings.account.status.sync.title') }}</div>
              <el-form label-width="220" label-position="left">
                <el-form-item>
                  <template #label>
                    <span>{{ t('settings.account.status.sync.auto') }}</span>
                    <el-tooltip placement="top" :content="t('settings.account.status.sync.autoHint')">
                      <el-icon style="margin-left:4px;color:var(--text-3);"><InfoFilled /></el-icon>
                    </el-tooltip>
                  </template>
                  <el-switch v-model="settings.autoSync" @change="onAutoSyncChange" />
                </el-form-item>
                <el-form-item>
                  <template #label>
                    <span>{{ t('settings.account.status.sync.sshKeys') }}</span>
                    <el-tooltip placement="top" :content="t('settings.account.status.sync.sshKeysHint')">
                      <el-icon style="margin-left:4px;color:var(--text-3);"><InfoFilled /></el-icon>
                    </el-tooltip>
                  </template>
                  <el-switch v-model="settings.syncSshKeys" />
                </el-form-item>
              </el-form>
              <div class="action-row">
                <el-button :icon="Download" plain :loading="sync.busy" @click="syncPull">{{ t('settings.account.status.sync.pull') }}</el-button>
                <el-button :icon="Upload" plain :loading="sync.busy" @click="syncPush">{{ t('settings.account.status.sync.push') }}</el-button>
              </div>
              <!-- 熔断拦截: 自动合并结果条目骤减, 已拦下不落地, 必须让用户知道 -->
              <div v-if="sync.blocked" class="sync-blocked">
                <el-icon><WarningFilled /></el-icon>
                <span>
                  {{ t('settings.account.status.sync.blocked', { doc: sync.blocked.doc === 'hosts' ? t('settings.account.status.sync.conflict.hosts') : t('settings.account.status.sync.conflict.prefs'), kept: sync.blocked.kept, local: sync.blocked.local }) }}
                </span>
              </div>
              <div v-if="lastSyncText || sync.lastError" class="sync-status">
                <span v-if="lastSyncText" class="sync-status-time">{{ t('settings.account.status.sync.lastSync', { time: lastSyncText }) }}</span>
                <span v-if="sync.lastError" class="sync-status-error">{{ sync.lastError }}</span>
              </div>
              <div class="section-footer">
                <el-button type="danger" plain @click="() => auth.logout()">
                  {{ t('user.menu.logout') }}
                </el-button>
              </div>
            </div>
          </section>

          <!-- ========== 数据管理 ========== -->
          <section v-if="active === 'data'" class="content-inner">
            <div class="section">
              <div class="section-title">{{ t('settings.data.host.title') }}</div>
              <div class="action-row">
                <el-button :icon="Download" plain @click="importHosts">{{ t('settings.data.host.import') }}</el-button>
                <el-button :icon="Upload"   plain @click="exportHosts">{{ t('settings.data.host.export') }}</el-button>
              </div>
              <input ref="importInputRef" type="file" accept=".json,application/json" style="display: none" @change="onImportFile" />
            </div>
            <div class="section">
              <div class="section-title">{{ t('settings.data.backup.title') }}</div>
              <div class="action-row">
                <el-button plain @click="backupDb">{{ t('settings.data.backup.export') }}</el-button>
                <el-button plain @click="clearCache">{{ t('settings.data.clearCache.title') }}</el-button>
              </div>
            </div>
            <div class="section">
              <div class="section-title">{{ t('settings.data.dir.title') }}</div>
              <el-form label-width="180" label-position="left">
                <el-form-item>
                  <template #label><span>{{ t('settings.data.dir.path') }}</span></template>
                  <div class="dir-row" style="display: flex; gap: 8px;">
                    <el-input :model-value="dataDir" readonly style="flex: 1" @click="copy(dataDir, t('settings.data.dir.path'))" />
                    <el-button plain @click="openDataDir">{{ t('settings.data.dir.open') }}</el-button>
                  </div>
                </el-form-item>
              </el-form>
            </div>
            <div class="section">
              <div class="section-title">{{ t('settings.data.sshKey.title') }}</div>
              <el-form label-width="180" label-position="left">
                <el-form-item>
                  <template #label><span>{{ t('settings.data.sshKey.ttl') }}</span></template>
                  <el-radio-group v-model="settings.sshKeyCacheTtl">
                    <el-radio-button value="forever">{{ t('settings.data.sshKey.ttl.forever') }}</el-radio-button>
                    <el-radio-button value="90d">{{ t('settings.data.sshKey.ttl.days90') }}</el-radio-button>
                  </el-radio-group>
                </el-form-item>
              </el-form>
              <span class="field-hint">{{ t('settings.data.sshKey.ttlHint') }}</span>
            </div>
          </section>

          <!-- ========== 通用 ========== -->
          <section v-if="active === 'general'" class="content-inner">
            <div class="section">
              <div class="section-title">{{ t('settings.general.startup.title') }}</div>
              <el-form label-width="220" label-position="left">
                <el-form-item>
                  <template #label><span>{{ t('settings.general.startup.autoLaunch') }}</span></template>
                  <el-switch v-model="settings.autoLaunch" />
                </el-form-item>
                <el-form-item>
                  <template #label>
                    <span>{{ t('settings.general.startup.minimizeToTray') }}</span>
                    <el-tooltip placement="top" content="开启后点右上角 × 窗口会最小化到系统托盘，右键托盘图标可恢复或退出。">
                      <el-icon style="margin-left:4px;color:var(--text-3);"><InfoFilled /></el-icon>
                    </el-tooltip>
                  </template>
                  <el-switch v-model="settings.minimizeToTray" />
                </el-form-item>
                <el-form-item>
                  <template #label><span>{{ t('settings.general.startup.confirmOnExit') }}</span></template>
                  <el-switch v-model="settings.confirmOnExit" />
                </el-form-item>
              </el-form>
            </div>
            <div class="section">
              <div class="section-title">{{ t('settings.general.update.title') }}</div>
              <el-form label-width="220" label-position="left">
                <el-form-item>
                  <template #label><span>{{ t('settings.general.update.auto') }}</span></template>
                  <el-switch v-model="settings.silentUpdate" />
                </el-form-item>
              </el-form>
            </div>
            <div class="section">
              <div class="section-title">
                {{ t('settings.general.proxy.title') }}
              </div>
              <el-form label-width="220" label-position="left">
                <el-form-item>
                  <template #label><span>{{ t(isMobile ? 'settings.general.data.proxy.short' : 'settings.general.data.proxy') }}</span></template>
                  <el-radio-group v-model="settings.proxyMode" style="white-space: nowrap">
                    <el-radio-button value="direct">{{ t('settings.general.data.proxy.direct') }}</el-radio-button>
                    <el-radio-button value="system">{{ t('settings.general.data.proxy.system') }}</el-radio-button>
                    <el-radio-button value="manual">{{ t('settings.general.data.proxy.manual') }}</el-radio-button>
                  </el-radio-group>
                </el-form-item>
                <template v-if="settings.proxyMode === 'manual'">
                  <el-form-item>
                    <template #label><span>{{ t('settings.general.proxy.host') }}</span></template>
                    <el-input v-model="settings.manualProxyHost" class="fit-input" style="width: 260px" placeholder="127.0.0.1" />
                  </el-form-item>
                  <el-form-item>
                    <template #label><span>{{ t('settings.general.proxy.port') }}</span></template>
                    <el-input-number v-model="settings.manualProxyPort" :min="1" :max="65535" />
                  </el-form-item>
                </template>
              </el-form>
            </div>
            <div class="section section-reset">
              <div class="section-title">{{ t('settings.general.reset.title') }}</div>
              <el-button plain type="danger" @click="onResetSettings">{{ t('settings.general.reset') }}</el-button>
              <span class="field-hint">{{ t('settings.general.resetHint') }}</span>
            </div>
          </section>

          <!-- ========== 终端 ========== -->
          <section v-if="active === 'terminal'" class="content-inner">
            <!-- 颜色（背景色 + 字体颜色）：放在终端设置最上方，方便快速调整 -->
            <div class="section">
              <div class="section-title">{{ t('settings.appearance.termColor.title') }}</div>

              <!-- 终端背景色 -->
              <div class="color-field">
                <div class="color-field-label">{{ t('settings.appearance.termBg.title') }}</div>
                <div class="accent-row">
                  <div
                    v-for="c in app.TERMINAL_BG_PRESET" :key="c" class="accent-chip"
                    :class="{ active: app.terminalBg === c }"
                    :style="{ background: c }"
                    @click="app.setTerminalBg(c)" />
                  <el-popover :width="240" trigger="click" popper-class="custom-color-popover">
                    <template #reference>
                      <button type="button" class="custom-color-btn" :class="{ active: !!app.terminalBg }">
                        <span class="custom-color-swatch" :style="app.terminalBg ? { background: app.terminalBg } : {}"></span>
                        <span class="custom-color-label">{{ t('settings.appearance.custom') }}</span>
                      </button>
                    </template>
                    <div class="custom-color-pop">
                      <el-color-picker
                        :model-value="app.terminalBgColor"
                        size="small"
                        show-alpha
                        @change="(v: string | null) => app.setTerminalBg(v || '')" />
                      <el-button size="small" plain @click="app.setTerminalBg('')">{{ t('settings.appearance.termBg.reset') }}</el-button>
                    </div>
                  </el-popover>
                </div>
              </div>

              <!-- 终端字体颜色（逻辑同背景色） -->
              <div class="color-field">
                <div class="color-field-label">{{ t('settings.appearance.termFg.title') }}</div>
                <div class="accent-row">
                  <div
                    v-for="c in app.TERMINAL_FG_PRESET" :key="c" class="accent-chip"
                    :class="{ active: app.terminalFg === c }"
                    :style="{ background: c }"
                    @click="app.setTerminalFg(c)" />
                  <el-popover :width="240" trigger="click" popper-class="custom-color-popover">
                    <template #reference>
                      <button type="button" class="custom-color-btn" :class="{ active: !!app.terminalFg }">
                        <span class="custom-color-swatch" :style="app.terminalFg ? { background: app.terminalFg } : {}"></span>
                        <span class="custom-color-label">{{ t('settings.appearance.custom') }}</span>
                      </button>
                    </template>
                    <div class="custom-color-pop">
                      <el-color-picker
                        :model-value="app.terminalFgColor"
                        size="small"
                        @change="(v: string | null) => app.setTerminalFg(v || '')" />
                      <el-button size="small" plain @click="app.setTerminalFg('')">{{ t('settings.appearance.termFg.reset') }}</el-button>
                    </div>
                  </el-popover>
                </div>
              </div>
            </div>
            <div class="section">
              <div class="section-title">{{ t('settings.terminal.session.title') }}</div>
              <el-form label-width="220" label-position="left">
                <el-form-item>
                  <template #label><span>{{ t('settings.terminal.session.shell') }}</span></template>
                  <el-select v-model="settings.defaultShell" :teleported="!isMobile" class="fit-input" style="width: 240px">
                    <el-option label="自动识别"     value="auto" />
                    <el-option label="PowerShell"   value="powershell" />
                    <el-option label="CMD"          value="cmd" />
                    <el-option label="Bash (Git)"   value="bash" />
                    <el-option label="Zsh (WSL/Mac)" value="zsh" />
                  </el-select>
                </el-form-item>
                <el-form-item>
                  <template #label><span>{{ t('settings.terminal.session.keepAlive') }}</span></template>
                  <el-input-number v-model="settings.sshKeepAlive" :min="0" :max="300" />
                  <span class="field-hint">{{ t('settings.terminal.session.keepAliveHint') }}</span>
                </el-form-item>
                <el-form-item>
                  <template #label><span>{{ t('settings.terminal.session.connectTimeout') }}</span></template>
                  <el-input-number v-model="settings.sshConnectTimeout" :min="3" :max="120" />
                  <span class="field-hint">{{ t('settings.terminal.unit.seconds') }}</span>
                </el-form-item>
                <el-form-item>
                  <template #label><span>{{ t('settings.terminal.session.autoReconnect') }}</span></template>
                  <el-switch v-model="settings.autoReconnect" />
                </el-form-item>
                <el-form-item v-if="settings.autoReconnect">
                  <template #label><span>{{ t('settings.terminal.session.maxReconnect') }}</span></template>
                  <el-input-number v-model="settings.maxReconnectTimes" :min="1" :max="20" />
                  <span class="field-hint">{{ t('settings.terminal.unit.times') }}</span>
                </el-form-item>
              </el-form>
            </div>
            <div class="section">
              <div class="section-title">{{ t('settings.terminal.view.title') }}</div>
              <el-form label-width="220" label-position="left">
                <el-form-item>
                  <template #label><span>{{ t('settings.appearance.font.terminal') }}</span></template>
                  <!-- teleported：移动端 WebView 里 popper 挂 body 拿不到弹窗主题变量，下拉会空白；
                       移动端渲染在弹窗内部（深色变量可用），桌面端保持 teleport -->
                  <el-select v-model="settings.terminalFont" :teleported="!isMobile" class="fit-input" style="width: 320px">
                    <el-option label="JetBrains Mono" value="JetBrains Mono" />
                    <el-option label="Fira Code" value="Fira Code" />
                    <el-option label="Consolas"  value="Consolas" />
                    <el-option label="Cascadia Code" value="Cascadia Code" />
                    <el-option label="Meslo LG M"  value="Meslo LG M" />
                  </el-select>
                </el-form-item>
                <el-form-item>
                  <template #label><span>{{ t('settings.terminal.view.fontSize') }}</span></template>
                  <!-- 上限 32：与 FONT_MAX 一致；移动端快捷面板上限 22 会 +4 偏移写回（最高 26），
                       上限若仍是 24 会把该值截断 -->
                  <el-slider v-model="settings.terminalFontSize" :min="10" :max="32" :step="1" show-input style="width: 340px" />
                </el-form-item>
                <el-form-item>
                  <template #label><span>{{ t('settings.terminal.view.lineHeight') }}</span></template>
                  <el-slider v-model="settings.terminalLineHeight" :min="1.0" :max="2.0" :step="0.05" show-input style="width: 340px" />
                </el-form-item>
                <el-form-item>
                  <template #label><span>{{ t('settings.terminal.view.cursorStyle') }}</span></template>
                  <el-radio-group v-model="settings.cursorStyle">
                    <el-radio-button value="block">{{ t('settings.terminal.view.cursor.block') }}</el-radio-button>
                    <el-radio-button value="bar">{{ t('settings.terminal.view.cursor.bar') }}</el-radio-button>
                    <el-radio-button value="underline">{{ t('settings.terminal.view.cursor.underline') }}</el-radio-button>
                  </el-radio-group>
                </el-form-item>
                <el-form-item>
                  <template #label><span>{{ t('settings.terminal.view.cursorBlink') }}</span></template>
                  <el-switch v-model="settings.cursorBlink" />
                </el-form-item>
                <el-form-item>
                  <template #label><span>{{ t('settings.terminal.view.scrollback') }}</span></template>
                  <el-input-number v-model="settings.scrollbackLines" :min="1000" :max="200000" :step="1000" style="width: 180px" />
                  <span class="field-hint">{{ t('settings.terminal.unit.lines') }}</span>
                </el-form-item>
              </el-form>
            </div>
            <div class="section">
              <div class="section-title">{{ t('settings.terminal.behavior.title') }}</div>
              <el-form label-width="220" label-position="left">
                <el-form-item>
                  <template #label><span>{{ t('settings.terminal.behavior.selectCopy') }}</span></template>
                  <el-switch v-model="settings.selectToCopy" />
                </el-form-item>
                <el-form-item>
                  <template #label><span>{{ t('settings.terminal.behavior.rightClickPaste') }}</span></template>
                  <el-switch v-model="settings.rightClickPaste" />
                </el-form-item>
                <el-form-item>
                  <template #label><span>{{ t('settings.terminal.behavior.dragUpload') }}</span></template>
                  <el-switch v-model="settings.allowDragUpload" />
                </el-form-item>
                <el-form-item>
                  <template #label><span>{{ t('settings.terminal.behavior.bell') }}</span></template>
                  <el-switch v-model="settings.bellAlert" />
                </el-form-item>
                <el-form-item>
                  <template #label><span>{{ t('settings.terminal.behavior.highlightPrompt') }}</span></template>
                  <el-switch v-model="settings.highlightPrompt" />
                </el-form-item>
              </el-form>
            </div>
          </section>

          <!-- ========== 偏好设置 ========== -->
          <section v-if="active === 'preference'" class="content-inner">
            <div class="section">
              <div class="section-title">{{ t('settings.preference.title') }}</div>
              <el-form label-width="220" label-position="left">
                <el-form-item>
                  <template #label><span>{{ t('settings.preference.middleCloseTab') }}</span></template>
                  <el-switch v-model="settings.middleCloseTab" />
                  <span class="field-hint">{{ t('settings.preference.middleCloseTabHint') }}</span>
                </el-form-item>
              </el-form>
            </div>
            <div class="section">
              <div class="section-title">{{ t('settings.preference.interaction.title') }}</div>
              <el-form label-width="220" label-position="left">
                <el-form-item>
                  <template #label><span>{{ t('settings.preference.interaction.multiSync') }}</span></template>
                  <el-radio-group v-model="settings.multiSyncTrigger" class="radio-col">
                    <el-radio value="doubleCtrl">{{ t('settings.preference.interaction.multiSync.doubleCtrl') }}</el-radio>
                    <el-radio value="longCtrl">{{ t('settings.preference.interaction.multiSync.longCtrl') }}</el-radio>
                    <el-radio value="alwaysOn">{{ t('settings.preference.interaction.multiSync.alwaysOn') }}</el-radio>
                    <el-radio value="alwaysOff">{{ t('settings.preference.interaction.multiSync.alwaysOff') }}</el-radio>
                  </el-radio-group>
                </el-form-item>
                <el-form-item>
                  <template #label><span>{{ t('settings.preference.interaction.fsToolbar') }}</span></template>
                  <el-radio-group v-model="settings.fsToolbarToggle" class="radio-col">
                    <el-radio value="singleAlt">{{ t('settings.preference.interaction.fsToolbar.singleAlt') }}</el-radio>
                    <el-radio value="doubleAlt">{{ t('settings.preference.interaction.fsToolbar.doubleAlt') }}</el-radio>
                  </el-radio-group>
                </el-form-item>
              </el-form>
            </div>
            <div class="section">
              <div class="section-title">{{ t('settings.preference.sideDock.title') }}</div>
              <el-form label-width="220" label-position="left">
                <el-form-item>
                  <template #label><span>{{ t('settings.preference.sideDock.pinned') }}</span></template>
                  <el-switch v-model="settings.sideDockPinned" />
                  <span class="field-hint">{{ t('settings.preference.sideDock.pinnedHint') }}</span>
                </el-form-item>
              </el-form>
            </div>
            <div class="section">
              <div class="section-title">{{ t('settings.preference.forward.title') }}</div>
              <el-form label-width="220" label-position="left">
                <el-form-item>
                  <template #label><span>{{ t('settings.preference.forward.style') }}</span></template>
                  <el-radio-group v-model="settings.forwardConfigStyle" class="radio-col">
                    <el-radio value="page">{{ t('settings.preference.forward.style.page') }}</el-radio>
                    <el-radio value="wizard">{{ t('settings.preference.forward.style.wizard') }}</el-radio>
                  </el-radio-group>
                </el-form-item>
              </el-form>
            </div>
          </section>

          <!-- ========== 主机与 SSH ========== -->
          <section v-if="active === 'host'" class="content-inner">
            <div class="section">
              <div class="section-title">{{ t('settings.host.default.title') }}</div>
              <el-form label-width="220" label-position="left">
                <el-form-item>
                  <template #label><span>{{ t('settings.host.default.port') }}</span></template>
                  <el-input-number v-model="settings.defaultPort" :min="1" :max="65535" />
                </el-form-item>
                <el-form-item>
                  <template #label><span>{{ t('settings.host.default.user') }}</span></template>
                  <el-input v-model="settings.defaultUser" class="fit-input" style="width: 260px" />
                </el-form-item>
                <el-form-item>
                  <template #label><span>{{ t('settings.host.default.auth') }}</span></template>
                  <el-radio-group v-model="settings.defaultAuth">
                    <el-radio-button value="password">{{ t('settings.host.default.auth.password') }}</el-radio-button>
                    <el-radio-button value="key">{{ t('settings.host.default.auth.key') }}</el-radio-button>
                  </el-radio-group>
                </el-form-item>
              </el-form>
            </div>
            <div class="section">
              <div class="section-title">
                {{ t('settings.host.jump.title') }}
                <span class="section-subtitle">{{ t('common.featureDev') }}</span>
              </div>
              <el-form label-width="220" label-position="left">
                <el-form-item>
                  <template #label><span>{{ t('settings.host.jump.enable') }}</span></template>
                  <el-switch v-model="settings.jumpEnabled" />
                </el-form-item>
                <el-form-item v-if="settings.jumpEnabled">
                  <template #label><span>{{ t('settings.host.jump.host') }}</span></template>
                  <el-input v-model="settings.jumpHost" class="fit-input" style="width: 320px" placeholder="user@host:22" />
                </el-form-item>
                <el-form-item>
                  <template #label><span>{{ t('settings.host.jump.retry') }}</span></template>
                  <el-input-number v-model="settings.jumpRetry" :min="0" :max="10" />
                  <span class="field-hint">{{ t('settings.terminal.unit.times') }}</span>
                </el-form-item>
              </el-form>
            </div>
            <div class="section">
              <div class="section-title">{{ t('settings.host.known.title') }}</div>
              <el-form label-width="220" label-position="left">
                <el-form-item>
                  <template #label><span>{{ t('settings.host.known.count') }}</span></template>
                  <span>{{ knownCount }}</span>
                </el-form-item>
                <el-form-item>
                  <template #label><span>{{ t('settings.host.known.trustAll') }}</span></template>
                  <el-button plain @click="clearKnownHosts">{{ t('settings.host.known.clear') }}</el-button>
                </el-form-item>
              </el-form>
            </div>
          </section>

          <!-- ========== 快捷键（可编辑，按功能分组） ========== -->
          <section v-if="active === 'shortcut'" class="content-inner">
            <div class="section">
              <div class="section-title">{{ t('settings.shortcut.title') }}</div>
              <p class="section-subtitle">{{ t('settings.shortcut.hint') }}</p>
              <template v-for="group in shortcutGroups" :key="group.id">
                <div class="shortcut-group-title">{{ t(group.titleKey) }}</div>
                <el-form label-width="180" label-position="left">
                  <el-form-item v-for="item in group.items" :key="item.key">
                    <template #label><span>{{ t(item.labelKey) }}</span></template>
                    <el-input
                      :model-value="recordingKey === item.key ? t('settings.shortcut.record') : displayCombo(item.combo)"
                      readonly
                      :class="{ 'is-recording': recordingKey === item.key, 'is-conflict': shortcutConflict === item.combo }"
                      style="width: 220px"
                      @focus="startRecord(item.key)"
                      @click="startRecord(item.key)"
                      @blur="cancelRecord"
                    />
                  </el-form-item>
                </el-form>
              </template>
              <div class="section-footer">
                <el-button plain @click="resetShortcuts">{{ t('settings.shortcut.reset') }}</el-button>
              </div>
            </div>
          </section>

          <!-- ========== 外观 ========== -->
          <section v-if="active === 'appearance'" class="content-inner">
            <div class="section">
              <div class="section-title">
                {{ t('settings.appearance.theme.title') }}
                <span class="section-subtitle">{{ t('settings.appearance.theme.hint') }}</span>
              </div>
              <el-radio-group v-model="themeForm">
                <el-radio-button value="light">{{ t('theme.light') }}</el-radio-button>
                <el-radio-button value="dark">{{ t('theme.dark') }}</el-radio-button>
                <el-radio-button value="system">{{ t('theme.system') }}</el-radio-button>
                <el-radio-button value="eye-care">{{ t('theme.eyeCare') }}</el-radio-button>
              </el-radio-group>
            </div>
            <div class="section">
              <div class="section-title">{{ t('settings.appearance.accent.title') }}</div>
              <div class="accent-row">
                <div
                  v-for="c in app.ACCENT_PRESET" :key="c" class="accent-chip"
                  :class="{ active: accentColorSwatch === c }"
                  :style="{ background: c }"
                  @click="accentColorSwatch = c" />
                <el-button size="small" plain type="primary" @click="pickRandomAccent">{{ t('settings.appearance.accent.random') }}</el-button>
              </div>
            </div>
            <div class="section">
              <div class="section-title">{{ t('settings.appearance.font.title') }}</div>
              <el-form label-width="220" label-position="left">
                <el-form-item>
                  <template #label><span>{{ t('settings.appearance.font.base') }}</span></template>
                  <el-slider v-model="settings.baseFontSize" :min="12" :max="18" :step="1" show-input style="width: 340px" />
                </el-form-item>
              </el-form>
            </div>
            <div class="section">
              <div class="section-title">{{ t('settings.appearance.dpi.title') }}</div>
              <el-form label-width="220" label-position="left">
                <el-form-item>
                  <template #label><span>{{ t('settings.appearance.dpi.followSystem') }}</span></template>
                  <el-switch v-model="settings.followDpi" />
                </el-form-item>
                <el-form-item>
                  <template #label><span>{{ t('settings.appearance.dpi.scale') }}</span></template>
                  <el-slider v-model="settings.uiScale" :min="80" :max="150" :step="5" show-input :disabled="settings.followDpi" style="width: 340px" />
                </el-form-item>
                <el-form-item>
                  <template #label><span>{{ t('settings.appearance.dpi.terminalOpacity') }}</span></template>
                  <el-slider v-model="settings.terminalOpacity" :min="30" :max="100" :step="5" show-input style="width: 340px" />
                </el-form-item>
              </el-form>
            </div>
          </section>

          <!-- ========== 语言 ========== -->
          <section v-if="active === 'language'" class="content-inner">
            <div class="section">
              <div class="section-title">{{ t('settings.language.title') }}</div>
              <p class="section-subtitle">{{ t('settings.language.hint') }}</p>
              <el-radio-group v-model="localeForm" size="large">
                <el-radio-button value="zh-CN">{{ t('user.submenu.zhCN') }}</el-radio-button>
                <el-radio-button value="en">{{ t('user.submenu.en') }}</el-radio-button>
              </el-radio-group>
            </div>
          </section>

          <!-- ========== 安全 ========== -->
          <section v-if="active === 'security'" class="content-inner">
            <div class="section">
              <div class="section-title">{{ t('settings.security.clipboard.title') }}</div>
              <el-form label-width="220" label-position="left">
                <el-form-item>
                  <template #label><span>{{ t('settings.security.clipboard.autoClear') }}</span></template>
                  <el-switch v-model="settings.clipboardClearEnabled" />
                </el-form-item>
                <el-form-item v-if="settings.clipboardClearEnabled">
                  <template #label><span>{{ t('settings.security.clipboard.clearAfter') }}</span></template>
                  <el-input-number v-model="settings.clipboardClearSeconds" :min="5" :max="600" />
                  <span class="field-hint">{{ t('settings.terminal.unit.seconds') }}</span>
                </el-form-item>
              </el-form>
            </div>
            <div class="section">
              <div class="section-title">{{ t('settings.security.session.title') }}</div>
              <el-form label-width="220" label-position="left">
                <el-form-item>
                  <template #label><span>{{ t('settings.security.password.enable') }}</span></template>
                  <el-switch :model-value="secStatus.has_password" @change="onSecurityEnableToggle" />
                  <span class="field-hint">{{ t('settings.security.password.enableHint') }}</span>
                </el-form-item>
                <!-- 生物识别解锁：仅移动端出现。开关直接绑后端状态（失败会被 refreshSec 拉回），
                     未设密码 / 本机未录入指纹人脸时置灰，并按四态给出说明文案 -->
                <el-form-item v-if="canUseBiometric">
                  <template #label><span>{{ t('settings.security.password.biometric.enable') }}</span></template>
                  <el-switch
                    :model-value="secStatus.biometric_enabled"
                    :disabled="!secStatus.has_password || (!secStatus.biometric_enabled && !bioReady)"
                    @change="onBiometricToggle"
                  />
                  <span class="field-hint">{{ bioHint }}</span>
                </el-form-item>
                <!-- 验证方式：指纹 / 人脸**两个独立开关**，至少保留一种。
                     都开启时交由系统选择（多数机型会用人脸），识别不出来时可切换 -->
                <el-form-item v-if="canUseBiometric && secStatus.biometric_enabled">
                  <template #label><span>{{ t('settings.security.password.biometric.methods') }}</span></template>
                  <div class="bio-methods">
                    <span class="bio-method">
                      <el-switch
                        :model-value="bioFingerprintOn"
                        @change="(v: any) => onBiometricKindToggle('fingerprint', v)"
                      />
                      <span>{{ t('settings.security.password.biometric.kind.fingerprint') }}</span>
                    </span>
                    <span class="bio-method">
                      <el-switch
                        :model-value="bioFaceOn"
                        @change="(v: any) => onBiometricKindToggle('face', v)"
                      />
                      <span>{{ t('settings.security.password.biometric.kind.face') }}</span>
                    </span>
                  </div>
                  <span class="bio-hint">{{ bioCapHint }}</span>
                  <span v-if="bioFpOnlyHint" class="bio-hint">{{ bioFpOnlyHint }}</span>
                </el-form-item>
              </el-form>

              <div class="scope-box">
                <div class="scope-row">
                  <el-checkbox :model-value="true" disabled />
                  <span class="scope-label">{{ t('settings.security.password.scope.view') }}</span>
                  <span class="field-hint">{{ t('settings.security.password.scope.required') }}</span>
                </div>
                <div class="scope-row">
                  <el-checkbox :model-value="scopesForm.startup" @change="(v: any) => onScopeChange('startup', v)" />
                  <span class="scope-label">{{ t('settings.security.password.scope.startup') }}</span>
                </div>
                <div class="scope-row">
                  <el-checkbox :model-value="settings.lockOnAppSwitch" @change="onLockOnAppSwitchChange" />
                  <span class="scope-label">{{ t('settings.security.session.lockOnAppSwitch') }}</span>
                </div>
                <div class="scope-row">
                  <el-checkbox :model-value="scopesForm.edit_delete" @change="(v: any) => onScopeChange('edit_delete', v)" />
                  <span class="scope-label">{{ t('settings.security.password.scope.edit') }}</span>
                </div>
                <div class="scope-row">
                  <el-checkbox :model-value="true" disabled />
                  <span class="scope-label">{{ t('settings.security.password.scope.exportPassword') }}</span>
                  <span class="field-hint">{{ t('settings.security.password.scope.required') }}</span>
                </div>
                <div class="scope-row">
                  <el-checkbox :model-value="scopesForm.export_data" @change="(v: any) => onScopeChange('export_data', v)" />
                  <span class="scope-label">{{ t('settings.security.password.scope.exportData') }}</span>
                </div>
              </div>

            </div>
            <div class="section">
              <div class="section-title">{{ t('settings.security.credential.title') }}</div>
              <el-form label-width="220" label-position="left">
                <el-form-item>
                  <template #label><span>{{ t('settings.security.credential.encrypt') }}</span></template>
                  <el-switch v-model="settings.encryptCredentials" @change="onEncryptToggle" />
                </el-form-item>
                <el-form-item>
                  <template #label><span>{{ t('settings.security.credential.verifyKeyPermission') }}</span></template>
                  <el-switch v-model="settings.verifyKeyPermission" />
                  <span class="field-hint">{{ t('settings.security.credential.verifyKeyHint') }}</span>
                </el-form-item>
              </el-form>
            </div>
          </section>

          <!-- ========== 实验性 ========== -->
          <section v-if="active === 'beta'" class="content-inner">
            <div class="section">
              <div class="section-title">
                {{ t('settings.beta.title') }}
                <el-tag size="small" type="warning" effect="plain" style="margin-left: 8px">Beta</el-tag>
              </div>
              <p class="section-subtitle">{{ t('settings.beta.warn') }}</p>
              <el-form label-width="240" label-position="left">
                <el-form-item>
                  <template #label><span>{{ t('settings.beta.plugin') }}</span></template>
                  <el-switch v-model="settings.betaPlugin" />
                </el-form-item>
                <el-form-item>
                  <template #label><span>{{ t('settings.beta.cmdAi') }}</span></template>
                  <el-switch v-model="settings.betaCmdAi" />
                </el-form-item>
                <el-form-item>
                  <template #label><span>{{ t('settings.beta.sshShare') }}</span></template>
                  <el-switch v-model="settings.betaSshShare" />
                </el-form-item>
              </el-form>
            </div>
          </section>

          <!-- ========== 关于 ========== -->
          <section v-if="active === 'about'" class="content-inner">
            <div class="section about-card">
              <img class="about-logo" src="@/assets/logo.png" alt="JC Shell Logo" />
              <div class="about-name">{{ appName() }}</div>
              <el-descriptions :column="1" size="default" style="margin-top: 14px" border>
                <el-descriptions-item :label="t('settings.about.version')">
                  <span class="mono">{{ version }}</span>
                </el-descriptions-item>
                <el-descriptions-item :label="t('settings.about.build')">
                  <span class="mono">{{ build }}</span>
                </el-descriptions-item>
                <el-descriptions-item :label="t('settings.about.website')">
                  <a :href="website" target="_blank" rel="noreferrer" class="link">{{ website }}</a>
                  <el-button link size="small" @click="copy(website)">{{ t('common.copy') }}</el-button>
                </el-descriptions-item>
                <el-descriptions-item :label="t('settings.about.docs')">
                  <a :href="docsUrl" target="_blank" rel="noreferrer" class="link">{{ docsUrl }}</a>
                  <el-button link size="small" @click="copy(docsUrl)">{{ t('common.copy') }}</el-button>
                </el-descriptions-item>
                <el-descriptions-item :label="t('settings.about.feedback')">
                  <a :href="feedbackUrl" target="_blank" rel="noreferrer" class="link">{{ feedbackUrl }}</a>
                  <el-button link size="small" @click="copy(feedbackUrl)">{{ t('common.copy') }}</el-button>
                </el-descriptions-item>
                <el-descriptions-item :label="t('settings.about.source')">
                  <a :href="sourceUrl" target="_blank" rel="noreferrer" class="link">{{ sourceUrl }}</a>
                  <el-button link size="small" @click="copy(sourceUrl)">{{ t('common.copy') }}</el-button>
                </el-descriptions-item>

              </el-descriptions>
              <div style="margin-top: 18px">
                <el-button type="primary" :loading="checking" @click="checkUpdate">{{ t('settings.about.checkUpdate') }}</el-button>
              </div>
              <div class="copyright">{{ t('settings.about.copyright') }}</div>
            </div>
          </section>

          </template>
            </div>
          </Transition>

        </div>

        <!-- 移动端整页形态：不要弹窗页脚（关闭/确认），修改即实时生效无需确认 -->
        <footer v-show="!isMobile" class="settings-main-footer shrink-0 px-6 py-3 border-t border-[var(--jc-border)] flex justify-end gap-3">
          <el-button @click="closeSettingsSelf()">{{ t('common.close') }}</el-button>
          <el-button type="primary" @click="confirmAll">{{ t('common.confirm') }}</el-button>
        </footer>
      </main>
    </div>
    <!-- 检查更新弹窗：挂在设置窗口内，确保从设置触发时弹窗在该窗口上层显示 -->
    <UpdateDialog />
  </div>
</template>

<style scoped>
.settings-root { color: var(--jc-text); }

/* 选项较多的单选型设置：允许换行，避免挤成一排被裁切 */
.radio-col {
  display: flex;
  flex-wrap: wrap;
  gap: 2px 14px;
}

/* 左侧 */
.settings-sidebar { }
.sidebar-account {
  display: flex; align-items: center; gap: 10px;
  padding: 14px 14px 12px;
  border-bottom: 1px solid var(--jc-border);
}
.avatar-circle {
  width: 38px; height: 38px; border-radius: 10px;
  display: flex; align-items: center; justify-content: center;
  color: #fff; font-weight: 700; font-size: 16px;
  flex-shrink: 0;
}
.account-meta { min-width: 0; flex: 1; }
.account-name { font-size: 14px; font-weight: 600; color: var(--jc-text); line-height: 1.2; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.account-plan { margin-top: 4px; }
.plan-muted { font-size: 11px; color: var(--jc-text-muted); }

.menu-list { padding-top: 6px; }
.menu-divider { height: 1px; background: var(--jc-border); margin: 8px 6px; }
.menu-item {
  display: flex; align-items: center; gap: 10px;
  width: 100%;
  padding: 8px 10px;
  background: transparent;
  border: 0;
  border-radius: 6px;
  color: var(--jc-text-soft);
  font-size: 13px;
  text-align: left;
  cursor: pointer;
  transition: background-color 0.12s, color 0.12s;
  margin-bottom: 2px;
}
.menu-item:hover { background: var(--jc-bg-hover); color: var(--jc-text); }
.menu-item.is-active { background: var(--jc-bg-hover); color: var(--jc-accent); font-weight: 600; }

/* 右侧 */
.settings-main { min-width: 0; }
.settings-main-header { padding: 18px 24px 14px; border-bottom: 1px solid var(--jc-border); }
.content-title { font-size: 20px; font-weight: 700; color: var(--jc-text); margin: 0; line-height: 1.3; }
.settings-main-content { padding: 20px 24px 20px; }
.content-inner { max-width: 820px; }
.section { margin-bottom: 26px; }
.section-title { font-size: 14px; color: var(--jc-text); font-weight: 600; margin-bottom: 12px; display: flex; align-items: center; }
.section-subtitle { display: block; font-size: 12px; color: var(--jc-text-muted); margin-top: 4px; font-weight: 400; }
.section-footer { margin-top: 10px; padding-top: 12px; border-top: 1px dashed var(--jc-border); text-align: right; }

/* 快捷键分组标题 */
.shortcut-group-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--jc-text-muted);
  margin: 16px 0 10px;
  padding-bottom: 6px;
  border-bottom: 1px solid var(--jc-border);
}
.section .shortcut-group-title:first-of-type { margin-top: 4px; }

.field-hint { margin-left: 8px; font-size: 12px; color: var(--jc-text-muted); }

/* 生物识别「验证方式」：指纹 / 人脸两个独立开关并排，提示各自独占一行
   （.field-hint 是行内 + 左外边距，多条会挤在一行，这里单独给一套整行样式） */
.bio-methods { display: flex; align-items: center; gap: 22px; }
.bio-method { display: inline-flex; align-items: center; gap: 6px; font-size: 13px; color: var(--jc-text); }
.bio-hint { display: block; width: 100%; margin-top: 4px; font-size: 12px; color: var(--jc-text-muted); }

/* 安全功能可选项两列展示：按列填充 —— 左列 查看会话密码 / 软件启动时验证 / 切出应用时自动锁定，
   右列 改删会话 / 导出会话密码 / 导出会话数据（grid-template-rows: 3 行保证 3+3 排布） */
.scope-box {
  background: var(--jc-bg-soft);
  border: 1px solid var(--jc-border);
  border-radius: 8px;
  padding: 10px 12px;
  margin-bottom: 12px;
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  grid-template-rows: repeat(3, auto);
  grid-auto-flow: column;
  row-gap: 6px;
  column-gap: 16px;
  align-items: center;
}
.scope-row { display: flex; align-items: center; gap: 8px; }
.scope-label { font-size: 13px; color: var(--jc-text); }
.mt-2 { margin-top: 8px; }

.accent-row { display: flex; flex-wrap: wrap; align-items: center; gap: 10px; }
.accent-chip { width: 24px; height: 24px; border-radius: 50%; cursor: pointer; border: 2px solid var(--jc-border); transition: transform 0.1s; }
.accent-chip:hover { transform: scale(1.1); }
.accent-chip.active { border-color: var(--jc-text); box-shadow: 0 0 0 2px var(--jc-bg), 0 0 0 3px var(--jc-accent); }

/* 终端颜色设置（背景色 / 字体颜色） */
.color-field { margin-bottom: 14px; }
.color-field-label { font-size: 12px; color: var(--jc-text-muted); margin-bottom: 6px; }

/* 自定义颜色按钮：明显的彩虹渐变色块 + 文案，一眼能看出是「自定义颜色」 */
.custom-color-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 24px;
  padding: 0 10px 0 4px;
  border: 1px solid var(--jc-border);
  border-radius: 14px;
  background: var(--jc-bg-soft);
  color: var(--jc-text);
  font-size: 12px;
  cursor: pointer;
  transition: border-color 0.12s, box-shadow 0.12s;
}
.custom-color-btn:hover { border-color: var(--jc-accent); }
.custom-color-btn.active {
  border-color: var(--jc-accent);
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--jc-accent) 25%, transparent);
}
.custom-color-swatch {
  width: 16px; height: 16px; border-radius: 50%;
  background: linear-gradient(135deg, #ff7676, #ffd166, #06d6a0, #4cc9f0, #b5179e);
  border: 1px solid rgba(0, 0, 0, 0.15);
  flex-shrink: 0;
}
.custom-color-label { line-height: 1; }
.custom-color-pop { display: flex; align-items: center; gap: 10px; }

.action-row { display: flex; gap: 10px; flex-wrap: wrap; }

.sync-status {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-top: 10px;
  font-size: 12px;
}
.sync-status-time { color: var(--jc-text-muted); }
.sync-status-error { color: var(--el-color-danger); }

.sync-blocked {
  display: flex;
  align-items: flex-start;
  gap: 6px;
  margin-top: 10px;
  padding: 8px 10px;
  border-radius: 6px;
  font-size: 12px;
  line-height: 1.5;
  color: var(--el-color-warning);
  background: color-mix(in srgb, var(--el-color-warning) 12%, transparent);
  border: 1px solid color-mix(in srgb, var(--el-color-warning) 40%, transparent);
}

.anon-card, .info-card { background: var(--jc-bg-soft); border: 1px solid var(--jc-border); border-radius: 8px; padding: 16px; color: var(--jc-text-soft); font-size: 13px; }

/* 账号区协议同意复选框（未勾选禁止登录） */
.pc-consent { display: flex; align-items: flex-start; gap: 8px; margin-top: 12px; font-size: 13px; line-height: 1.5; color: var(--jc-text-soft); cursor: pointer; }
.pc-consent__cb { margin-top: 2px; width: 16px; height: 16px; flex: none; accent-color: var(--jc-accent, #3b82f6); }
.pc-consent__text { flex: 1; }
.pc-consent__link { color: var(--jc-accent, #3b82f6); text-decoration: none; }
.pc-consent__link:hover { text-decoration: underline; }
.info-row { display: flex; align-items: center; justify-content: space-between; padding: 12px 0; gap: 12px; }
.info-row + .info-row { border-top: 1px solid var(--jc-border); }
.info-row:first-child { padding-top: 0; }
.info-row:last-child { padding-bottom: 0; }
.info-row-title { font-size: 14px; color: var(--jc-text); font-weight: 600; }
.info-row-sub { font-size: 12px; color: var(--jc-text-muted); margin-top: 2px; }
.info-row-actions { display: flex; align-items: center; gap: 6px; flex-shrink: 0; }

.about-card { text-align: center; padding: 24px; background: var(--jc-bg-soft); border: 1px solid var(--jc-border); border-radius: 10px; }
.about-logo { width: 64px; height: 64px; margin: 0 auto; border-radius: 16px; object-fit: contain; }
.about-name { margin-top: 10px; font-size: 18px; font-weight: 700; color: var(--jc-text); }
.copyright { margin-top: 18px; color: var(--jc-text-muted); font-size: 12px; }
.link { color: var(--jc-accent); text-decoration: none; margin-right: 6px; font-size: 13px; }
.mono { font-family: Consolas, 'JetBrains Mono', monospace; }

/* 快捷键录制态 */
.is-recording :deep(.el-input__wrapper) {
  box-shadow: 0 0 0 1px var(--jc-accent) inset !important;
  background-color: var(--jc-bg-hover);
}
.is-conflict :deep(.el-input__wrapper) {
  box-shadow: 0 0 0 1px var(--el-color-danger) inset !important;
}

.settings-main-footer { background: var(--jc-bg-panel); }
</style>
