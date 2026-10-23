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
  <el-dialog
    v-model="secDlg.visible"
    :title="secDlg.title"
    width="420px"
    align-center
    :append-to-body="true"
    modal-class="sec-overlay"
    :z-index="secZIndex"
    :close-on-click-modal="false"
    @close="onClose"
    @opened="onOpened"
  >
    <div class="sec-risk">{{ secDlg.riskWarning }}</div>

    <p v-if="secDlg.message" class="sec-msg">{{ secDlg.message }}</p>

    <!-- 设置模式：分两步输入，先设密码 → 再确认 -->
    <template v-if="secDlg.mode === 'set'">
      <div v-if="phase === 'first'" class="sec-field">
        <div class="sec-label">输入安全密码</div>
        <PinInput ref="pinRef" v-model="pin1" :autofocus="true" :error="!!err" @complete="phase = 'confirm'" />
      </div>
      <div v-else class="sec-field">
        <div class="sec-label">再次输入以确认</div>
        <PinInput
          ref="pinRef"
          :key="pin2Key"
          v-model="pin2"
          :autofocus="true"
          :error="!!err"
          :success="matched"
          @complete="onConfirmComplete"
        />
        <button class="sec-back" type="button" @click="onBackToEdit">
          返回修改
        </button>
      </div>
      <div v-if="err" class="sec-err">{{ err }}</div>
    </template>

    <!-- 验证模式：单次输入 -->
    <template v-else>
      <div class="sec-field">
        <PinInput ref="pinRef" v-model="pin" :autofocus="true" :error="!!err" />
      </div>
      <div v-if="err" class="sec-err">{{ err }}</div>
    </template>

    <template #footer>
      <el-button v-if="secDlg.mode === 'verify' && secDlg.showWipe" type="danger" plain @click="onWipe">
        抹除会话数据并关闭
      </el-button>
      <el-button @click="onCancel">取消</el-button>
      <el-button v-if="secDlg.mode === 'set'" type="primary" :disabled="!matched" @click="onSetConfirm">确认设置</el-button>
      <!-- 验证模式：输满 4 位自动提交（见 watch(pin)），无需「验证」按钮 -->
    </template>
  </el-dialog>
</template>

<script setup lang="ts">
/**
 * 安全密码弹窗：set（设置两次）/ verify（验证，可选「抹除并关闭」）。
 * 由 securityDialog.ts 单例驱动，结果通过 closeSecDlg 回传。作者：JC
 */
import { ref, watch, computed } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import PinInput from '@/components/common/PinInput.vue'
import { secDlg, closeSecDlg } from '@/utils/securityDialog'
import { tracePinBlack } from '@/utils/pinBlackTrace'

const pin = ref('')
const pin1 = ref('')
const pin2 = ref('')
const err = ref('')
/** 当前可见的 PinInput 实例（set 两步 / verify 三者互斥，始终只有一个在 DOM 中） */
const pinRef = ref<InstanceType<typeof PinInput> | null>(null)
const phase = ref<'first' | 'confirm'>('first')
const pin2Key = ref(0)

/**
 * 兜底用：主窗口里由全局样式 `.el-overlay.sec-overlay` 强制置顶（覆盖 `.el-overlay` 的
 * `z-index:10000 !important`）；在未加载该样式的窗口（如独立设置窗口）里，
 * 退化为「取高于现存弹层的值」，依旧能盖住已打开的弹窗。
 */
const secZIndex = ref(2000)

/** 扫描当前所有 el-overlay，返回「最大 z-index + 1」 */
function topZIndex(): number {
  let max = 2000
  document.querySelectorAll('.el-overlay').forEach((el) => {
    const z = parseInt(window.getComputedStyle(el).zIndex || '0', 10)
    if (!Number.isNaN(z) && z > max) max = z
  })
  return max + 1
}

const matched = computed(
  () => phase.value === 'confirm'
    && pin1.value.length === 4
    && pin2.value.length === 4
    && pin1.value === pin2.value,
)

watch(
  () => secDlg.visible,
  (v) => {
    if (v) {
      // 【临时】黑屏诊断（见 utils/pinBlackTrace.ts）
      tracePinBlack('pin:open')
      // 置顶主路径是全局样式 `.el-overlay.sec-overlay`（见 MainWindow）；
      // 这里仅在未加载该样式的窗口里兜底抬高
      secZIndex.value = topZIndex()
      pin.value = ''
      pin1.value = ''
      pin2.value = ''
      err.value = ''
      phase.value = 'first'
      pin2Key.value++
    }
  },
)
watch(pin2, (v) => { if (v.length > 0) err.value = '' })
// 验证模式：输满 4 位自动提交，无需点「验证」（设置模式保持手动确认，便于二次输入校对）
watch(pin, (v) => {
  if (secDlg.mode === 'verify' && v.length === 4) onVerify()
})

function onClose() {
  closeSecDlg({ action: 'cancel', pin: '' })
}

/** 弹窗打开动画结束：此时内容已可见且无焦点，主动把光标放到第一个密码位 */
function onOpened() {
  // 【临时】黑屏诊断：此处会唤起软键盘，是黑屏最可疑的触发点
  tracePinBlack('pin:focus')
  pinRef.value?.focusFirst()
}
function onCancel() {
  closeSecDlg({ action: 'cancel', pin: '' })
}

function onSetConfirm() {
  if (!/^\d{4}$/.test(pin1.value)) {
    err.value = '安全密码必须是 4 位数字'
    return
  }
  if (pin1.value !== pin2.value) {
    err.value = '两次输入的密码不一致'
    return
  }
  err.value = ''
  closeSecDlg({ action: 'ok', pin: pin1.value })
}

/** 第二次输满 4 位：只校验，不保存。匹配变绿，不符清空重输 */
function onConfirmComplete() {
  if (pin1.value !== pin2.value) {
    err.value = '两次输入的密码不一致'
    pin2.value = ''
    pin2Key.value++
    return
  }
  err.value = ''
  // matched computed 自动为 true，输入框变绿，等用户点「确认设置」
}

/** 返回修改：清空所有输入，光标回到第一个密码位 */
function onBackToEdit() {
  pin1.value = ''
  pin2.value = ''
  err.value = ''
  phase.value = 'first'
}

function onVerify() {
  if (!/^\d{4}$/.test(pin.value)) {
    err.value = '请输入 4 位数字安全密码'
    return
  }
  err.value = ''
  closeSecDlg({ action: 'ok', pin: pin.value })
}

async function onWipe() {
  try {
    await ElMessageBox.confirm(
      '此操作将清除全部会话数据（主机与分组）并移除安全密码保护，且不可恢复。确定继续？',
      '抹除会话数据并关闭',
      { type: 'warning', confirmButtonText: '抹除并关闭', cancelButtonText: '取消' },
    )
  } catch {
    return
  }
  closeSecDlg({ action: 'wipe', pin: '' })
}
</script>

<style scoped>
.sec-risk {
  background: var(--el-color-danger-light-9);
  border: 1px solid var(--el-color-danger-light-7);
  color: var(--el-color-danger);
  font-size: 12px;
  line-height: 1.6;
  padding: 10px 12px;
  border-radius: 8px;
  margin-bottom: 14px;
}
.sec-msg { font-size: 13px; color: var(--jc-text); margin: 0 0 12px; }
.sec-field { margin-bottom: 14px; }
.sec-label { font-size: 13px; color: var(--jc-text-muted); margin-bottom: 8px; }
.sec-err { color: var(--el-color-danger); font-size: 12px; margin-bottom: 10px; }
.sec-back {
  margin-top: 8px;
  font-size: 12px;
  color: var(--el-color-primary);
  background: transparent;
  border: none;
  cursor: pointer;
  padding: 0;
}
.sec-back:hover { color: var(--el-color-primary-light-3); }
</style>
