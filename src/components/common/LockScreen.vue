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
  <div class="lock-screen" @keydown.enter="unlock">
    <div class="lock-card">
      <img class="lock-logo" src="@/assets/logo.png" alt="JC Shell Logo" />
      <div class="lock-title">{{ appName() }} 已锁定</div>
      <div class="lock-sub">{{ locked ? '账户已被锁定，请稍后再试' : '输入安全密码以解锁' }}</div>
      <PinInput
        v-model="pin"
        :autofocus="true"
        :disabled="locked || verifying"
        :error="wrong"
        class="lock-input"
        @complete="unlock"
      />
      <!-- 生物识别入口：仅已开启且本机可用时出现（桌面端恒不出现）。
           取消 / 识别失败都停留在这里，PIN 始终是可用的兜底入口 -->
      <button
        v-if="bioEnabled && !locked"
        class="lock-bio"
        type="button"
        :disabled="verifying"
        @click="unlockByBiometric"
      >
        使用指纹 / 人脸解锁
      </button>
      <div v-if="wrong" class="lock-error">密码错误，请重试</div>
      <div v-if="locked" class="lock-error">已锁定，{{ remainingText }} 后可重试</div>
      <div v-else-if="verifying" class="lock-hint">正在验证…</div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 锁屏遮罩：安全密码保护开启并命中保护范围时覆盖主窗口，验证通过后端后解锁。
 * 输入满 4 位即自动校验（PinInput 的 complete 事件），无需点按钮；回车为等价兜底。
 * 锁定状态（连续 5 次错误）由后端下发，前端只读展示倒计时；重启/关闭软件无法跳过。
 * 作者：JC
 */
import { nextTick, onMounted, onBeforeUnmount, ref } from 'vue'
import { appName } from '@/utils/edition'
import { securityStatus, securityVerify } from '@/utils/securityPassword'
import { biometricUnlock, biometricAvailability, type BiometricMode } from '@/utils/biometric'
import { tracePinBlack } from '@/utils/pinBlackTrace'
import PinInput from '@/components/common/PinInput.vue'

const emit = defineEmits<{ (e: 'unlock'): void }>()

const pin = ref('')
const wrong = ref(false)
const locked = ref(false)
/** 是否展示「指纹 / 人脸解锁」入口：已开启生物识别 **且** 本机当前确实可用 */
const bioEnabled = ref(false)
/** 用户设置的生物识别方式（决定弹哪种强度、失败后可切换哪些方式） */
const bioMode = ref<BiometricMode>('both')
/** 正在向后端校验（防止 complete 与回车重复触发） */
const verifying = ref(false)
const remaining = ref(0)
const remainingText = ref('')
let timer: ReturnType<typeof setInterval> | null = null

function fmt(sec: number): string {
  const m = Math.floor(sec / 60)
  const s = sec % 60
  return m > 0 ? `${m} 分 ${s} 秒` : `${s} 秒`
}

async function refreshLockState() {
  try {
    const st = await securityStatus()
    locked.value = st.locked
    remaining.value = st.lock_remaining_sec
    bioEnabled.value = st.biometric_enabled
    bioMode.value = st.biometric_mode
    // 开关开着不代表此刻能用（用户可能去系统里删掉了指纹 / 人脸），故再问一次系统；
    // 不可用就不显示入口，避免点了没反应。
    if (bioEnabled.value) {
      const { available } = await biometricAvailability()
      bioEnabled.value = available
    }
  } catch {
    locked.value = false
    bioEnabled.value = false
  }
}

/** 把光标放回锁屏自带的密码框（锁屏本身就是密码入口，无需另弹安全密码框） */
function focusPin() {
  nextTick(() => {
    const first = document.querySelector('.lock-input input') as HTMLInputElement | null
    first?.focus()
  })
}

/** 走系统指纹 / 人脸解锁；失败 / 取消都会回到「换一种方式」选择器，由用户决定下一步 */
async function unlockByBiometric() {
  if (!bioEnabled.value || locked.value || verifying.value) return
  verifying.value = true
  /** 'pin' 场景要聚焦密码框，但 verifying 期间它被禁用 —— 必须等解除后再聚焦 */
  let focusAfter = false
  try {
    const outcome = await biometricUnlock(`解锁 ${appName()}`, bioMode.value)
    if (outcome === 'ok') {
      emit('unlock')
      return
    }
    // 'pin'：用户明确选择了「使用安全密码」→ 聚焦锁屏自带的密码框；
    // 'cancel'：用户放弃 → 停在锁屏，不做任何额外弹窗。
    focusAfter = outcome === 'pin'
  } finally {
    verifying.value = false
  }
  if (focusAfter) focusPin()
}

async function unlock() {
  if (locked.value || verifying.value) return
  if (pin.value.length < 4) {
    wrong.value = true
    setTimeout(() => { wrong.value = false }, 500)
    return
  }
  verifying.value = true
  try {
    const r = await securityVerify(pin.value)
    if (r.ok) {
      emit('unlock')
      return
    }
    if (r.locked) {
      locked.value = true
      remaining.value = r.lock_remaining_sec
      wrong.value = false
      return
    }
    wrong.value = true
    pin.value = ''
    setTimeout(() => { wrong.value = false }, 500)
  } catch {
    wrong.value = true
    pin.value = ''
  } finally {
    verifying.value = false
  }
}

function tick() {
  if (remaining.value > 0) {
    remaining.value -= 1
    remainingText.value = fmt(remaining.value)
    if (remaining.value <= 0) {
      locked.value = false
      refreshLockState()
    }
  }
}

onMounted(async () => {
  await refreshLockState()
  remainingText.value = fmt(remaining.value)
  if (timer) clearInterval(timer)
  timer = setInterval(tick, 1000)
  // 【临时】黑屏诊断（见 utils/pinBlackTrace.ts）
  tracePinBlack('lock:mount')
  // 已开启生物识别：进入锁屏即主动弹一次系统验证（与银行类 App 的体验一致）；
  // 取消 / 失败就停在 PIN 输入上，用户也可点按钮重试。
  if (bioEnabled.value) void unlockByBiometric()
  nextTick(() => {
    tracePinBlack('lock:focus')
    const first = document.querySelector('.lock-input input') as HTMLInputElement | null
    first?.focus()
  })
})

onBeforeUnmount(() => {
  if (timer) clearInterval(timer)
})
</script>

<style scoped>
.lock-screen {
  position: fixed;
  inset: 0;
  z-index: 9999;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--jc-bg);
}
.lock-card { width: 320px; text-align: center; }
.lock-logo {
  width: 64px; height: 64px; margin: 0 auto 14px;
  border-radius: 16px; object-fit: contain;
}
.lock-title { font-size: 18px; font-weight: 700; color: var(--jc-text); }
.lock-sub { margin: 6px 0 16px; font-size: 12px; color: var(--jc-text-muted); }
.lock-input { margin-bottom: 10px; }
.lock-bio {
  width: 100%;
  height: 40px;
  margin-bottom: 10px;
  border-radius: 10px;
  border: 1px solid var(--jc-border);
  background: var(--jc-bg-soft);
  color: var(--jc-text);
  font-size: 14px;
  cursor: pointer;
}
.lock-bio:active { background: var(--jc-bg-hover); }
.lock-bio:disabled { opacity: .5; cursor: default; }
.lock-error { color: var(--el-color-danger); font-size: 12px; }
.lock-hint { color: var(--jc-text-muted); font-size: 12px; }
</style>
