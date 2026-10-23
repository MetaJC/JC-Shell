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
  <div class="m-page m-profile">
    <!-- ===== 账号卡片 ===== -->
    <div class="m-card m-profile__account">
      <button class="m-profile__avatar" :class="{ 'is-logging-in': auth.isLoggingIn }" @click="onAvatarClick">
        <img v-if="avatarUrl" :src="avatarUrl" alt="avatar" />
        <span v-else-if="auth.isLoggingIn" class="m-profile__spin" />
        <el-icon v-else :size="26"><UserFilled /></el-icon>
      </button>
      <div class="m-profile__ident">
        <div class="m-profile__name">{{ displayName }}</div>
        <div class="m-profile__meta">{{ metaText }}</div>
      </div>
      <span v-if="isLoggedIn && points !== null" class="m-profile__points">{{ t('profile.points') }} {{ points }}</span>
    </div>

    <!-- ===== 分组长列表（分组只做视觉切分，不参与交互） ===== -->
    <div v-for="group in groups" :key="group.title" class="m-group">
      <div class="m-group__title">{{ group.title }}</div>
      <button v-for="cell in group.cells" :key="cell.key" class="m-cell" @click="cell.onClick">
        <el-icon :size="18" class="m-cell__icon"><component :is="cell.icon" /></el-icon>
        <span class="m-cell__label">{{ cell.label }}</span>
        <span v-if="cell.value" class="m-cell__value">{{ cell.value }}</span>
        <el-icon :size="14" class="m-cell__arrow"><ArrowRight /></el-icon>
      </button>
    </div>

    <!-- 登录态错误提示（验证码超时 / 网络失败等，由 auth store 统一给出文案）。
         紧跟登录按钮上方，让用户点完登录立刻看到失败原因。 -->
    <div v-if="auth.errorMsg" class="m-profile__error">
      <span>{{ auth.errorMsg }}</span>
      <button class="m-profile__error-close" @click="auth.clearError()">&times;</button>
    </div>

    <!-- 账号操作：放在页面最下面（手机 App 惯例：信息列在上方，账号动作沉底）。
         未登录→登录；登录中→取消；已登录→退出登录 -->
    <div class="m-profile__actions">
      <el-button v-if="auth.isLoggingIn" class="m-btn-block" @click="auth.cancelLogin()">
        {{ t('user.menu.cancelLogin') }}
      </el-button>
      <el-button v-else-if="!isLoggedIn && !capPhoneLogin" type="primary" class="m-btn-block" @click="auth.startLogin()">
        {{ t('user.menu.login') }}
      </el-button>
      <el-button v-else-if="!isLoggedIn && capPhoneLogin" type="primary" class="m-btn-block" @click="showPhoneLogin = true">
        {{ t('user.menu.login') }}
      </el-button>
      <el-button v-else class="m-btn-block" @click="onLogout">{{ t('user.menu.logout') }}</el-button>
    </div>

    <MobilePhoneLogin v-model:visible="showPhoneLogin" />
  </div>
</template>

<script setup lang="ts">
/**
 * 移动端「我的」页（底部导航第 5 项）
 * 作者：JC
 *
 * 移动端没有桌面顶部的「文件/编辑/视图/终端/工具/帮助」菜单栏（手机宽度挤不下），
 * 所有账号与设置类入口统一收敛到这一页，等价于手机 App 的「我的」。
 *
 * 设计要点：
 *  1. 只做入口聚合与账号操作，具体设置项仍复用桌面端设置面板（openSettings(tab)），
 *     不重复实现一份移动端设置 UI，避免两边配置项长期不同步。
 *  2. 依赖桌面独立窗口的入口（个人中心 / 邀请得积分）用 CAP.multiWindow() 门控 ——
 *     Tauri Android 上所有窗口同处一个 Activity 视图栈，独立窗口会永久盖住主窗口。
 */
import { computed, defineComponent, h, ref, markRaw, watch, type Component } from 'vue'
import { invoke } from '@/api'
import { ElMessageBox } from 'element-plus'
import {
  ArrowRight, Brush, InfoFilled, Lock, Monitor, Refresh, Setting, User, UserFilled,
} from '@element-plus/icons-vue'
import { useAuthStore } from '@/stores/auth'
import { useAppStore, type ThemeMode } from '@/stores/app'
import { useI18nStore, type Locale } from '@/stores/i18n'
import { useUpdateStore } from '@/stores/update'
import { openSettings } from '@/composables/useSettingsOpener'
import { lockAppIfPossible } from '@/composables/useLockApp'
import { CAP } from '@/utils/platform'
import MobilePhoneLogin from '@/components/pages/mobile/MobilePhoneLogin.vue'

interface ProfileCell {
  key: string
  label: string
  icon: Component
  /** 右侧补充说明（如当前主题 / 语言），无则不显示 */
  value?: string
  onClick: () => void
}
interface ProfileGroup {
  title: string
  cells: ProfileCell[]
}

/** 语言图标：Element Plus 没有「地球」图标，沿用头像菜单里那枚内联 SVG，保持两端视觉一致 */
const IconGlobe = defineComponent({
  render: () => h('svg', {
    viewBox: '0 0 24 24', width: 18, height: 18, fill: 'none',
    stroke: 'currentColor', 'stroke-width': 1.6, 'stroke-linecap': 'round', 'stroke-linejoin': 'round',
  }, [
    h('circle', { cx: 12, cy: 12, r: 9 }),
    h('path', { d: 'M3 12h18' }),
    h('path', { d: 'M12 3a15 15 0 0 1 0 18' }),
    h('path', { d: 'M12 3a15 15 0 0 0 0 18' }),
  ]),
})

const auth = useAuthStore()
const app = useAppStore()
const i18n = useI18nStore()
const updateStore = useUpdateStore()
const t = i18n.t

// 移动端用 APP 内手机号登录向导代替桌面端 OAuth 跳转；桌面端此开关恒为 false
const capPhoneLogin = computed(() => CAP.appPhoneLogin())
const showPhoneLogin = ref(false)

// ===== 账号信息 =====
const isLoggedIn = computed(() => auth.state.status === 'authenticated')
const avatarUrl = computed(() => {
  if (auth.state.status === 'authenticated' && auth.state.user.avatar) return auth.state.user.avatar as string
  return ''
})
/** 昵称优先，其次手机号后四位，最后用户名 */
const displayName = computed(() => {
  if (auth.state.status !== 'authenticated') return t('user.notLoggedIn')
  const u = auth.state.user
  if (u.nickname && String(u.nickname).trim()) return String(u.nickname).trim()
  const digits = String(u.phone ?? '').replace(/\D/g, '')
  if (digits.length >= 4) return digits.slice(-4)
  return u.username || t('user.notLoggedIn')
})
const metaText = computed(() => {
  if (auth.state.status !== 'authenticated') return t('profile.loginHint')
  return auth.state.user.username || ''
})

/** AI 助手积分余额（未登录 / 拉取失败不显示） */
const points = ref<number | null>(null)
async function refreshPoints(): Promise<void> {
  if (!isLoggedIn.value) { points.value = null; return }
  try {
    const vo = await invoke<{ balance?: number }>('ai_asset_balance')
    points.value = typeof vo?.balance === 'number' ? vo.balance : null
  } catch {
    points.value = null
  }
}

function onAvatarClick(): void {
  if (isLoggedIn.value || auth.isLoggingIn) return
  if (capPhoneLogin.value) showPhoneLogin.value = true
  else auth.startLogin()
}
function onLogout(): void {
  void ElMessageBox.confirm(t('user.logoutConfirm'), t('user.logoutConfirmTitle'), {
    confirmButtonText: t('common.ok'),
    cancelButtonText: t('common.cancel'),
    type: 'warning' as const,
  }).then(() => auth.logout()).catch(() => { /* 取消 */ })
}

// ===== 设置与系统入口 =====
const themeLabel = computed(() => {
  const m = app.themeMode
  return m === 'light' ? t('theme.light')
    : m === 'dark' ? t('theme.dark')
      : m === 'system' ? t('theme.system')
        : t('theme.eyeCare')
})
const langLabel = computed(() => (i18n.locale === 'en' ? t('user.submenu.en') : t('user.submenu.zhCN')))

/** 主题循环切换（与头像菜单一致：切主题顺便换强调色，避免同色系看不出变化） */
const THEME_CYCLE: ThemeMode[] = ['light', 'dark', 'system', 'eye-care']
function cycleTheme(): void {
  const idx = THEME_CYCLE.indexOf(app.themeMode)
  app.setTheme(THEME_CYCLE[(idx + 1) % THEME_CYCLE.length])
  app.randomizeAccent()
}
function toggleLang(): void {
  const next: Locale = i18n.locale === 'en' ? 'zh-CN' : 'en'
  i18n.setLocale(next)
}
function checkUpdate(): void {
  updateStore.openDialog()
  void updateStore.checkUpdate('manual')
}
function openProfile(): void {
  void invoke('open_profile')
}

const groups = computed<ProfileGroup[]>(() => {
  const appGroup: ProfileGroup = {
    title: t('profile.group.app'),
    cells: [
      { key: 'settings', label: t('user.menu.settings'), icon: markRaw(Setting), onClick: () => void openSettings('general') },
      { key: 'theme', label: t('user.menu.theme'), icon: markRaw(Brush), value: themeLabel.value, onClick: cycleTheme },
      { key: 'lang', label: t('user.menu.lang'), icon: markRaw(IconGlobe), value: langLabel.value, onClick: toggleLang },
    ],
  }
  const accountGroup: ProfileGroup = {
    title: t('profile.group.account'),
    cells: [
      { key: 'account-settings', label: t('settings.menu.account'), icon: markRaw(User), onClick: () => void openSettings('account') },
      { key: 'hosts', label: t('settings.menu.host'), icon: markRaw(Monitor), onClick: () => void openSettings('host') },
    ],
  }
  const systemGroup: ProfileGroup = {
    title: t('profile.group.system'),
    cells: [
      { key: 'update', label: t('profile.checkUpdate'), icon: markRaw(Refresh), onClick: checkUpdate },
      { key: 'lock', label: t('profile.lock'), icon: markRaw(Lock), onClick: () => void lockAppIfPossible() },
      { key: 'about', label: t('settings.menu.about'), icon: markRaw(InfoFilled), onClick: () => void openSettings('about') },
    ],
  }
  // 个人中心 / 邀请得积分依赖桌面独立窗口：移动端不展示（见文件头说明）
  if (CAP.multiWindow()) {
    accountGroup.cells.push({
      key: 'profile-window', label: t('user.menu.profile'), icon: markRaw(UserFilled), onClick: openProfile,
    })
  }
  return [accountGroup, appGroup, systemGroup]
})

// 登录态变化就刷新积分（immediate 覆盖首次进入本页的情况）
watch(isLoggedIn, () => { void refreshPoints() }, { immediate: true })
</script>
