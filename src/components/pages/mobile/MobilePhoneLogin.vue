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
 * 移动端 APP 内手机号登录向导（滑块验证码 → 短信验证码 → 一步登录）
 * 仅当 CAP.appPhoneLogin() 为真（移动端 WebView）时由「我的」页登录按钮拉起；
 * 桌面端仍走 OAuth 浏览器跳转，本组件不会被渲染，逻辑互不干扰。
 * 作者：JC
 */
import { ref, reactive, computed, watch, onBeforeUnmount } from 'vue'
import { ElMessage } from 'element-plus'
import { invoke } from '@/api'
import { useAuthStore } from '@/stores/auth'
import { useConsentStore } from '@/stores/consent'
import { useI18nStore } from '@/stores/i18n'
import SlideCaptcha from '@/components/auth/SlideCaptcha.vue'
import InAppPage from '@/components/legal/InAppPage.vue'

const props = defineProps<{ visible: boolean }>()
const emit = defineEmits<{
  (e: 'update:visible', v: boolean): void
  (e: 'close'): void
}>()

const auth = useAuthStore()
const consent = useConsentStore()
const i18n = useI18nStore()
const t = i18n.t

// 用户协议 / 隐私政策门禁：登录与获取验证码前必须勾选同意
const uaUrl = computed(() => consent.config?.userAgreementUrl || null)
const ppUrl = computed(() => consent.config?.privacyPolicyUrl || null)

// 用户协议 / 隐私政策：在 APP 内以全屏覆盖层打开（不跳转系统浏览器）
const legalView = reactive({ visible: false, url: '', title: '' })

async function openLegalLink(url: string | null, title: string) {
  if (!url) {
    ElMessage.warning(t('consent.linkMissing'))
    return
  }
  // 追加客户端防护放行参数（WebView 无法自定义请求头），避免被自家 WAF 误拦成 403
  try {
    legalView.url = await invoke<string>('legal_page_url', { url })
  } catch {
    legalView.url = url
  }
  legalView.title = title
  legalView.visible = true
}

const phone = ref('')
const code = ref('')
const captchaVisible = ref(false)
const captchaToken = ref('')
const sending = ref(false)
const logging = ref(false)
const countdown = ref(0)
const phoneError = ref('')
const codeError = ref('')
const agreed = ref(false)
const agreedError = ref('')

let countdownTimer: number | null = null

const validPhone = computed(() => /^1\d{10}$/.test(phone.value))
const canSendCode = computed(() => validPhone.value && countdown.value === 0 && !sending.value)

function startCountdown(seconds: number) {
  if (countdownTimer) clearInterval(countdownTimer)
  countdown.value = seconds
  countdownTimer = window.setInterval(() => {
    countdown.value -= 1
    if (countdown.value <= 0) {
      if (countdownTimer) clearInterval(countdownTimer)
      countdownTimer = null
    }
  }, 1000)
}

function onPhoneInput() {
  phone.value = phone.value.replace(/\D/g, '').slice(0, 11)
  phoneError.value = ''
}

function onAgreeChange() {
  if (agreed.value) {
    agreedError.value = ''
    consent.markAgreed()
  }
}

function close() {
  emit('update:visible', false)
  emit('close')
}

function onGetCode() {
  if (!agreed.value) {
    agreedError.value = t('user.phoneLogin.agreeRequired')
    return
  }
  if (!validPhone.value) {
    phoneError.value = t('user.phoneLogin.invalidPhone')
    return
  }
  // 先过滑块验证码，成功后在 onCaptchaSuccess 里发短信
  captchaVisible.value = true
}

async function onCaptchaSuccess(payload: { captchaToken: string }) {
  captchaToken.value = payload.captchaToken
  captchaVisible.value = false
  sending.value = true
  try {
    const cd = await invoke<number>('phone_sms_send', {
      phone: phone.value,
      captchaToken: payload.captchaToken,
    })
    startCountdown(Math.max(1, Number(cd) || 60))
    ElMessage.success(t('user.phoneLogin.codeSent'))
  } catch (e: any) {
    ElMessage.error(e?.message || t('user.phoneLogin.sendFail'))
  } finally {
    sending.value = false
  }
}

async function onLogin() {
  if (!agreed.value) {
    agreedError.value = t('user.phoneLogin.agreeRequired')
    return
  }
  if (!validPhone.value) {
    phoneError.value = t('user.phoneLogin.invalidPhone')
    return
  }
  if (!/^\d{4,8}$/.test(code.value)) {
    codeError.value = t('user.phoneLogin.invalidCode')
    return
  }
  logging.value = true
  try {
    // 随登录上送已同意的协议版本（经同意门禁采集）；无则不上送，由后端按"未同意"拦截
    const agreed = consent.pending ?? (consent.config
      ? { userAgreementVersion: consent.config.userAgreementVersion, privacyPolicyVersion: consent.config.privacyPolicyVersion }
      : null)
    const args: Record<string, unknown> = { phone: phone.value, code: code.value }
    // 注意：Tauri 命令参数键名用 camelCase（与 Rust snake_case 参数自动映射），
    // 写成 snake_case 会被静默当作未传 → 后端判定"未同意协议"而拦截登录。
    if (agreed?.userAgreementVersion) args.agreedUserAgreementVersion = agreed.userAgreementVersion
    if (agreed?.privacyPolicyVersion) args.agreedPrivacyPolicyVersion = agreed.privacyPolicyVersion
    await invoke('phone_login', args)
    // 登录态由 Rust 端持久化（移动端仅文件备份）；重新拉取一次以同步全局 store
    await auth.load()
    close()
  } catch (e: any) {
    ElMessage.error(e?.message || t('user.phoneLogin.loginFail'))
  } finally {
    logging.value = false
  }
}

watch(
  () => props.visible,
  (v) => {
    if (!v) {
      if (countdownTimer) clearInterval(countdownTimer)
      countdownTimer = null
      captchaVisible.value = false
    }
  }
)

onBeforeUnmount(() => {
  if (countdownTimer) clearInterval(countdownTimer)
})
</script>

<template>
  <Teleport to="body">
  <div v-if="visible" class="m-subpage m-phonelogin">
    <div class="m-pl-head">
      <button class="m-pl-back" type="button" aria-label="返回" @click="close">‹</button>
      <div class="m-pl-title">{{ t('user.phoneLogin.title') }}</div>
      <div class="m-pl-spacer"></div>
    </div>

    <div class="m-pl-body">
      <div class="m-pl-field">
        <label class="m-pl-label">{{ t('user.phoneLogin.phone') }}</label>
        <div class="m-pl-inputrow">
          <input
            v-model="phone"
            class="m-pl-input"
            type="tel"
            inputmode="numeric"
            maxlength="11"
            :placeholder="t('user.phoneLogin.phonePlaceholder')"
            @input="onPhoneInput"
          />
        </div>
        <div v-if="phoneError" class="m-pl-err">{{ phoneError }}</div>
      </div>

      <div class="m-pl-field">
        <label class="m-pl-label">{{ t('user.phoneLogin.code') }}</label>
        <div class="m-pl-inputrow">
          <input
            v-model="code"
            class="m-pl-input"
            type="text"
            inputmode="numeric"
            maxlength="6"
            :placeholder="t('user.phoneLogin.codePlaceholder')"
            @input="codeError = ''"
          />
          <button
            class="m-pl-codebtn"
            type="button"
            :disabled="!canSendCode || !agreed"
            @click="onGetCode"
          >
            {{ countdown > 0 ? `${countdown}s` : sending ? '...' : t('user.phoneLogin.getCode') }}
          </button>
        </div>
        <div v-if="codeError" class="m-pl-err">{{ codeError }}</div>
      </div>

      <div class="m-pl-consent">
        <label class="m-pl-consent__row">
          <input type="checkbox" v-model="agreed" class="m-pl-consent__cb" @change="onAgreeChange" />
          <span class="m-pl-consent__text">
            {{ t('consent.agreePrefix') }}
            <a :href="uaUrl || '#'" class="m-pl-consent__link" @click.prevent="openLegalLink(uaUrl, t('consent.userAgreement'))">{{ t('consent.userAgreement') }}</a>
            {{ t('consent.agreeAnd') }}
            <a :href="ppUrl || '#'" class="m-pl-consent__link" @click.prevent="openLegalLink(ppUrl, t('consent.privacyPolicy'))">{{ t('consent.privacyPolicy') }}</a>
          </span>
        </label>
        <div v-if="agreedError" class="m-pl-err">{{ agreedError }}</div>
      </div>

      <button class="m-pl-submit" type="button" :disabled="logging || !agreed" @click="onLogin">
        {{ logging ? '...' : t('user.phoneLogin.submit') }}
      </button>

      <div class="m-pl-tip">{{ t('user.phoneLogin.hint') }}</div>
    </div>

    <SlideCaptcha v-model:visible="captchaVisible" @success="onCaptchaSuccess" />
  </div>
  </Teleport>

  <!-- APP 内打开用户协议 / 隐私政策（不跳转系统浏览器） -->
  <InAppPage v-model:visible="legalView.visible" :url="legalView.url" :title="legalView.title" />
</template>

<style scoped>
.m-pl-consent {
  margin: 14px 0 4px;
}
.m-pl-consent__row {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  font-size: 13px;
  line-height: 1.5;
  color: var(--jc-text-soft, #606266);
  cursor: pointer;
}
.m-pl-consent__cb {
  margin-top: 2px;
  width: 16px;
  height: 16px;
  flex: none;
  accent-color: var(--jc-accent, #3b82f6);
}
.m-pl-consent__text {
  flex: 1;
}
.m-pl-consent__link {
  color: var(--jc-accent, #3b82f6);
  text-decoration: none;
}
.m-pl-consent__link:hover {
  text-decoration: underline;
}
</style>
