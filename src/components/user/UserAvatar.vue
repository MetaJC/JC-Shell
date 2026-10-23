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
 * 左下角用户头像 + 昵称标签 + 二级菜单（含 邀请得积分 / 语言 / 主题 / 设置 / 登录 子菜单）
 * 作者：JC
 */
import { computed, ref, onMounted, onBeforeUnmount, h } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useAuthStore } from '@/stores/auth'
import { useI18nStore, type Locale } from '@/stores/i18n'
import { useAppStore, type ThemeMode } from '@/stores/app'
import { openSettings } from '@/composables/useSettingsOpener'
import { ElMessageBox, ElMessage } from 'element-plus'

const props = defineProps<{ collapsed?: boolean }>()

const auth = useAuthStore()
const i18n = useI18nStore()
const app = useAppStore()

const menuOpen = ref<boolean>(false)
const submenu = ref<'lang' | 'theme' | null>(null)

/** AI 积分余额（菜单打开时拉取；未登录/拉取失败不显示） */
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

type PrimaryAction = 'lang' | 'theme' | 'settings' | 'profile' | 'invite' | 'login' | 'cancel-login' | 'logout'
const t = (k: string, p?: Record<string, string | number>): string => i18n.t(k, p)

const avatarUrl = computed<string>(() => {
  if (auth.state.status === 'authenticated' && auth.state.user.avatar) return auth.state.user.avatar as string
  return ''
})

function phoneLast4(v: string | null | undefined): string {
  if (!v) return ''
  const d = String(v).replace(/\D/g, '')
  return d.length >= 4 ? d.slice(-4) : d
}

const displayName = computed<string>(() => {
  if (auth.state.status !== 'authenticated') return t('user.notLoggedIn')
  const u = auth.state.user
  if (u.nickname && String(u.nickname).trim()) return String(u.nickname).trim()
  const p4 = phoneLast4(u.phone)
  if (p4) return p4
  return u.username || t('user.notLoggedIn')
})
const labelText = computed<string>(() => displayName.value)
const isLoggedIn = computed<boolean>(() => auth.state.status === 'authenticated')

const themeActive = (m: ThemeMode): boolean => app.themeMode === m
const langActive = (l: Locale): boolean => i18n.locale === l

function onClickAvatar(): void {
  submenu.value = null
  menuOpen.value = !menuOpen.value
  if (menuOpen.value) void refreshPoints()
}

function closeMenuOutside(e: MouseEvent): void {
  const el = e.target as HTMLElement | null
  const root = document.querySelector('.jc-user-avatar') as HTMLElement | null
  if (root && el && !root.contains(el)) { menuOpen.value = false; submenu.value = null }
}

function onPrimaryAction(action: PrimaryAction): void {
  if (action === 'login') { menuOpen.value = false; void auth.startLogin(); return }
  if (action === 'cancel-login') { menuOpen.value = false; void auth.cancelLogin(); return }
  if (action === 'logout') {
    menuOpen.value = false
    void ElMessageBox.confirm(t('user.logoutConfirm'), t('user.logoutConfirmTitle'), {
      confirmButtonText: t('common.ok'),
      cancelButtonText: t('common.cancel'),
      type: 'warning' as const,
    }).then(() => auth.logout()).catch(() => { /* cancel */ })
    return
  }
  // 设置：走统一入口，与顶部菜单 / 原生菜单 / Ctrl+, 弹出完全一致的窗口
  if (action === 'settings') { menuOpen.value = false; void openSettings(); return }
  if (action === 'profile') { menuOpen.value = false; void invoke('open_profile'); return }
  // 邀请得积分: 依旧打开个人中心, 但用 menu 参数直达邀请面板
  if (action === 'invite') {
    menuOpen.value = false
    void invoke('open_profile', { menu: 'invite' })
    return
  }
  if (action === 'lang' || action === 'theme') {
    submenu.value = submenu.value === action ? null : action
  }
}

function onSelectTheme(m: ThemeMode): void {
  app.setTheme(m)
  app.randomizeAccent()
  ElMessage.success({
    message: h('div', null, [
      t('theme.' + m) + ' · ',
      h('span', { style: { color: app.accentColor } }, t('theme.accent.switchHint') + ' ' + app.accentColor)
    ])
  })
  submenu.value = null
  menuOpen.value = false
}

function onSelectLang(l: Locale): void {
  i18n.setLocale(l)
  ElMessage.success(t(l === 'en' ? 'user.langSwitch.successEn' : 'user.langSwitch.success'))
  submenu.value = null
  menuOpen.value = false
}

/** 模板里访问 user 字段时做一下 TS 窄化辅助 */
const userMeta = computed<{ username: string; phone: string | null } | null>(() => {
  if (auth.state.status === 'authenticated') {
    return { username: auth.state.user.username, phone: (auth.state.user.phone as string) || null }
  }
  return null
})

onMounted(() => {
  void auth.subscribeEvents()
  void auth.load()
  document.addEventListener('click', closeMenuOutside as (e: Event) => void)
})
onBeforeUnmount(() => {
  auth.unsubscribeEvents()
  document.removeEventListener('click', closeMenuOutside as (e: Event) => void)
})
</script>

<template>
  <div :class="['jc-user-avatar', { 'is-collapsed': props.collapsed }]">
    <div class="avatar-row" @click.stop="onClickAvatar">
      <el-tooltip :content="displayName" placement="right" :show-after="150" :hide-after="0" :show-arrow="false">
        <button class="avatar-btn"
          :class="{ 'is-authenticated': isLoggedIn, 'is-logging-in': auth.isLoggingIn }"
          :title="displayName">
          <img v-if="avatarUrl" :src="avatarUrl" alt="avatar" />
          <span v-else-if="auth.isLoggingIn" class="loading-dot" />
          <svg v-else viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="1.6">
            <circle cx="12" cy="8" r="4" />
            <path d="M4 20c0-3.3 3.6-6 8-6s8 2.7 8 6" stroke-linecap="round" />
          </svg>
        </button>
      </el-tooltip>
      <div v-if="!props.collapsed" class="avatar-label" :class="{ 'muted': !isLoggedIn }">{{ labelText }}</div>
    </div>

    <transition name="fade">
      <div v-if="menuOpen" class="user-menu" @click.stop>
        <div class="user-info">
          <div class="user-name-row">
            <div class="user-name">{{ displayName }}</div>
            <span v-if="isLoggedIn && points !== null" class="points-chip" title="AI 助手可用积分">积分 {{ points }}</span>
          </div>
          <div class="user-meta" v-if="isLoggedIn">
            {{ userMeta!.username }}
          </div>
          <div class="user-meta" v-else>{{ t('user.notLoggedIn') }}</div>
        </div>

        <button v-if="isLoggedIn" class="menu-item" @click.stop="onPrimaryAction('invite')">
          <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
            <path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" />
            <circle cx="9" cy="7" r="4" />
            <path d="M19 8v6" />
            <path d="M22 11h-6" />
          </svg>
          <span>{{ t('user.menu.invite') }}</span>
          <span class="menu-item-right">
            <span class="tag-muted">{{ t('user.menu.invite.hint') }}</span>
          </span>
        </button>

        <div class="menu-item has-submenu"
          @click.stop="onPrimaryAction('lang')">
          <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="9"/>
            <path d="M3 12h18"/>
            <path d="M12 3a15 15 0 0 1 0 18"/>
            <path d="M12 3a15 15 0 0 0 0 18"/>
          </svg>
          <span>{{ t('user.menu.lang') }}</span>
          <span class="menu-item-right">
            <span v-if="langActive('zh-CN')" class="tag-muted">中文</span>
            <span v-else class="tag-muted">EN</span>
            <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2"><path d="M9 18l6-6-6-6" stroke-linecap="round" stroke-linejoin="round"/></svg>
          </span>
          <transition name="fade">
            <div v-if="submenu === 'lang'" class="submenu-panel submenu-right">
              <button class="submenu-item" :class="{ active: langActive('zh-CN') }" @click.stop="onSelectLang('zh-CN')">
                <span>{{ t('user.submenu.zhCN') }}</span>
                <span v-if="langActive('zh-CN')" class="check">✓</span>
              </button>
              <button class="submenu-item" :class="{ active: langActive('en') }" @click.stop="onSelectLang('en')">
                <span>{{ t('user.submenu.en') }}</span>
                <span v-if="langActive('en')" class="check">✓</span>
              </button>
            </div>
          </transition>
        </div>

        <div class="menu-item has-submenu"
          @click.stop="onPrimaryAction('theme')">
          <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
            <path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z" />
          </svg>
          <span>{{ t('user.menu.theme') }}</span>
          <span class="menu-item-right">
            <span class="accent-dot" :style="{ background: app.accentColor }" />
            <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2"><path d="M9 18l6-6-6-6" stroke-linecap="round" stroke-linejoin="round"/></svg>
          </span>
          <transition name="fade">
            <div v-if="submenu === 'theme'" class="submenu-panel submenu-right">
              <button class="submenu-item" :class="{ active: themeActive('light') }" @click.stop="onSelectTheme('light')">
                <span>{{ t('theme.light') }}</span>
                <span v-if="themeActive('light')" class="check">✓</span>
              </button>
              <button class="submenu-item" :class="{ active: themeActive('dark') }" @click.stop="onSelectTheme('dark')">
                <span>{{ t('theme.dark') }}</span>
                <span v-if="themeActive('dark')" class="check">✓</span>
              </button>
              <button class="submenu-item" :class="{ active: themeActive('system') }" @click.stop="onSelectTheme('system')">
                <span>{{ t('theme.system') }}</span>
                <span v-if="themeActive('system')" class="check">✓</span>
              </button>
              <button class="submenu-item" :class="{ active: themeActive('eye-care') }" @click.stop="onSelectTheme('eye-care')">
                <span>{{ t('theme.eyeCare') }}</span>
                <span v-if="themeActive('eye-care')" class="check">✓</span>
              </button>
            </div>
          </transition>
        </div>

        <button class="menu-item" @click.stop="onPrimaryAction('settings')">
          <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="3"/>
            <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33h0a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51h0a1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82v0a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>
          </svg>
          <span>{{ t('user.menu.settings') }}</span>
        </button>

        <button v-if="isLoggedIn" class="menu-item" @click.stop="onPrimaryAction('profile')">
          <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
            <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"/>
            <circle cx="12" cy="7" r="4"/>
          </svg>
          <span>{{ t('user.menu.profile') }}</span>
        </button>

        <div class="menu-divider" />

        <button v-if="auth.isLoggingIn" class="menu-item" @click.stop="onPrimaryAction('cancel-login')">
          <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
            <line x1="18" y1="6" x2="6" y2="18" />
            <line x1="6" y1="6" x2="18" y2="18" />
          </svg>
          <span>{{ t('user.menu.cancelLogin') }}</span>
        </button>
        <button v-else-if="!isLoggedIn" class="menu-item" @click.stop="onPrimaryAction('login')">
          <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
            <path d="M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4"/>
            <polyline points="10 17 15 12 10 7"/>
            <line x1="15" y1="12" x2="3" y2="12"/>
          </svg>
          <span>{{ t('user.menu.login') }}</span>
        </button>
        <button v-else class="menu-item menu-item-danger" @click.stop="onPrimaryAction('logout')">
          <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
            <path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/>
            <polyline points="16 17 21 12 16 7"/>
            <line x1="21" y1="12" x2="9" y2="12"/>
          </svg>
          <span>{{ t('user.menu.logout') }}</span>
        </button>
      </div>
    </transition>

    <div v-if="auth.errorMsg" class="auth-error-tip">
      <span class="auth-error-text">{{ auth.errorMsg }}</span>
      <button class="auth-error-close" @click.stop="auth.clearError()" title="关闭">&times;</button>
    </div>
  </div>
</template>

<style scoped>
.jc-user-avatar { position: relative; display: flex; align-items: center; gap: 10px; }
.avatar-row { display: flex; align-items: center; gap: 10px; cursor: pointer; user-select: none; }
.avatar-btn { width: 36px; height: 36px; border-radius: 50%; border: 1px solid var(--jc-border);
  background: var(--jc-bg-soft); color: var(--jc-text-muted); display: flex; align-items: center; justify-content: center;
  padding: 0; overflow: hidden; transition: border-color 0.15s, color 0.15s; }
.avatar-btn:hover { border-color: var(--jc-accent); color: var(--jc-accent); }
.avatar-btn.is-authenticated { border-color: var(--jc-border); }
.avatar-btn.is-logging-in { cursor: progress; }
.avatar-btn img { width: 100%; height: 100%; object-fit: cover; border-radius: 50%; }
.loading-dot { width: 14px; height: 14px; border: 2px solid var(--jc-border-strong); border-top-color: var(--jc-accent);
  border-radius: 50%; animation: spin 0.8s linear infinite; }
@keyframes spin { to { transform: rotate(360deg); } }

.avatar-label { font-size: 13px; color: var(--jc-text); font-weight: 600; line-height: 1.2; padding-right: 8px; max-width: 120px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.avatar-label.muted { color: var(--jc-text-muted); font-weight: 500; }

.user-menu { position: absolute; bottom: 56px; left: 0; min-width: 210px;
  background: var(--jc-bg-panel); border: 1px solid var(--jc-border); border-radius: 8px;
  box-shadow: var(--jc-shadow); padding: 6px; z-index: 200; }
.user-info { padding: 6px 10px 8px; border-bottom: 1px solid var(--jc-border); margin-bottom: 4px; }
.user-name-row { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
.user-name { font-size: 13px; color: var(--jc-text); font-weight: 600; }
.points-chip { font-size: 11px; color: var(--jc-accent); background: var(--jc-bg-soft);
  border: 1px solid var(--jc-accent); border-radius: 10px; padding: 1px 8px; white-space: nowrap; }
.user-meta { font-size: 11px; color: var(--jc-text-muted); margin-top: 2px; }
.menu-item { position: relative; width: 100%; display: flex; align-items: center; gap: 10px; text-align: left;
  background: transparent; border: 0; color: var(--jc-text-soft); padding: 8px 10px; font-size: 13px;
  border-radius: 5px; cursor: pointer; }
.menu-item:hover { background: var(--jc-bg-hover); color: var(--jc-accent); }
.menu-item-danger:hover { color: var(--jc-danger); }
.menu-divider { height: 1px; background: var(--jc-border); margin: 4px 4px; }
.menu-item-right { margin-left: auto; display: inline-flex; align-items: center; gap: 6px; color: var(--jc-text-muted); }
.tag-muted { font-size: 11px; color: var(--jc-text-muted); background: var(--jc-bg-soft); border: 1px solid var(--jc-border); border-radius: 4px; padding: 1px 6px; }
.accent-dot { width: 10px; height: 10px; border-radius: 50%; border: 1px solid var(--jc-border); }

.submenu-panel { position: absolute; left: calc(100% + 4px); top: 4px; min-width: 170px;
  background: var(--jc-bg-panel); border: 1px solid var(--jc-border); border-radius: 8px; padding: 6px;
  box-shadow: var(--jc-shadow); z-index: 120; }
.submenu-item { width: 100%; display: flex; align-items: center; justify-content: space-between; text-align: left;
  background: transparent; border: 0; color: var(--jc-text-soft); padding: 7px 10px; font-size: 13px;
  border-radius: 5px; cursor: pointer; }
.submenu-item:hover { background: var(--jc-bg-hover); color: var(--jc-accent); }
.submenu-item.active { color: var(--jc-accent); font-weight: 600; }
.submenu-item .check { font-size: 12px; color: var(--jc-accent); }

.auth-error-tip { position: absolute; bottom: 56px; left: 0; min-width: 220px;
  background: rgba(127, 29, 29, 0.95); color: #fecaca; font-size: 11px; padding: 6px 10px;
  border-radius: 4px; border: 1px solid rgba(248, 113, 113, 0.4); z-index: 150;
  display: flex; align-items: center; justify-content: space-between; gap: 8px; }
.auth-error-text { flex: 1; word-break: break-all; }
.auth-error-close { flex-shrink: 0; width: 18px; height: 18px; border: 0; background: transparent;
  color: #fecaca; font-size: 16px; line-height: 1; cursor: pointer; border-radius: 3px; padding: 0; }
.auth-error-close:hover { background: rgba(248, 113, 113, 0.3); }

.fade-enter-active, .fade-leave-active { transition: opacity 0.12s, transform 0.12s; }
.fade-enter-from, .fade-leave-to { opacity: 0; transform: translateY(4px); }
</style>
