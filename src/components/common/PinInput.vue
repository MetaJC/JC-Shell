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
  <div class="pin-input" :class="{ 'pin-error': error, 'pin-success': success }">
    <input
      v-for="(_, i) in cells"
      :key="i"
      ref="inputs"
      class="pin-cell"
      type="text"
      inputmode="numeric"
      autocomplete="off"
      maxlength="1"
      :value="displayChar(i)"
      :disabled="disabled"
      @input="onInput($event, i)"
      @keydown="onKey($event, i)"
      @paste.prevent="onPaste"
      @focus="onFocus(i)"
      @blur="onBlur"
      @beforeinput="onBeforeInput($event, i)"
      @compositionend="onCompositionEnd($event, i)"
    />
  </div>
</template>

<script setup lang="ts">
/**
 * 4 位方格数字 PIN 输入框。
 * 仅允许数字，自动跳格、支持退格/方向键/粘贴。
 * v-model 绑定 4 位数字字符串。作者：JC
 */
import { ref, computed, nextTick, onMounted, watch, PropType } from 'vue'

const props = defineProps({
  modelValue: { type: String, default: '' },
  length: { type: Number, default: 4 },
  disabled: { type: Boolean, default: false },
  autofocus: { type: Boolean, default: false },
  error: { type: Boolean, default: false },
  success: { type: Boolean, default: false },
})
const emit = defineEmits<{ (e: 'update:modelValue', v: string): void; (e: 'complete', v: string): void }>()

/** 命令式聚焦第一个密码位（供弹窗打开后调用，规避 autofocus 在隐藏时失效） */
function focusFirst() {
  focusIdx(0)
}

const inputs = ref<HTMLInputElement[]>([])
const focusedIndex = ref(-1)
const cells = computed(() => {
  const arr = props.modelValue.split('')
  const out: string[] = []
  for (let i = 0; i < props.length; i++) out.push(arr[i] || '')
  return out
})

function setVal(str: string) {
  emit('update:modelValue', str.replace(/\D/g, '').slice(0, props.length))
}
/** 显示字符：当前聚焦格显示真实数字，其余已填格用圆点掩码 */
function displayChar(i: number): string {
  const ch = props.modelValue[i] || ''
  return ch && i !== focusedIndex.value ? '•' : ch
}
function focusIdx(i: number) {
  nextTick(() => inputs.value[i]?.focus())
}
function onInput(e: Event, i: number) {
  const target = e.target as HTMLInputElement
  const digit = (target.value.match(/\d/) || ['']).pop() || ''
  // 立即纠正 DOM 显示：IME / 中文 / 字母残留直接清掉
  target.value = digit || ''
  const chars = props.modelValue.split('')
  while (chars.length < props.length) chars.push('')
  chars[i] = digit
  setVal(chars.join(''))
  if (digit && i < props.length - 1) focusIdx(i + 1)
}
function onKey(e: KeyboardEvent, i: number) {
  // 只放行数字与必要的导航/编辑键；Ctrl/Cmd 组合键用于粘贴/全选，不拦截
  if (
    !/^\d$/.test(e.key) &&
    !['Backspace', 'ArrowLeft', 'ArrowRight', 'Tab', 'Enter'].includes(e.key) &&
    !(e.ctrlKey || e.metaKey)
  ) {
    e.preventDefault()
    return
  }
  if (e.key === 'Backspace') {
    const chars = props.modelValue.split('')
    while (chars.length < props.length) chars.push('')
    if (chars[i]) {
      chars[i] = ''
      setVal(chars.join(''))
      e.preventDefault()
    } else if (i > 0) {
      focusIdx(i - 1)
    }
  } else if (e.key === 'ArrowLeft' && i > 0) {
    focusIdx(i - 1)
  } else if (e.key === 'ArrowRight' && i < props.length - 1) {
    focusIdx(i + 1)
  }
}
function onPaste(e: ClipboardEvent) {
  const text = (e.clipboardData?.getData('text') || '').replace(/\D/g, '').slice(0, props.length)
  if (!text) return
  setVal(text)
  focusIdx(Math.min(text.length, props.length - 1))
}
function onFocus(i: number) {
  focusedIndex.value = i
  const el = inputs.value[i]
  el?.select()
}
function onBlur() {
  focusedIndex.value = -1
}

/** 在输入真正进入 DOM 之前拦截：只有单个数字才允许写入，其它字符/中文一律阻止 */
function onBeforeInput(e: InputEvent, i: number) {
  // 删除、粘贴等交给现有 keydown / paste 处理
  if (['deleteContentBackward', 'deleteContentForward', 'insertFromPaste'].includes(e.inputType)) return
  const data = e.data || ''
  if (!/^\d$/.test(data)) {
    e.preventDefault()
    return
  }
}
function onCompositionEnd(e: CompositionEvent, i: number) {
  // IME 落字后若还不是数字，兜底清空本格
  const data = e.data || ''
  if (/^\d$/.test(data)) return
  const el = inputs.value[i]
  if (el) el.value = ''
  const chars = props.modelValue.split('')
  while (chars.length < props.length) chars.push('')
  chars[i] = ''
  setVal(chars.join(''))
}

watch(
  () => props.autofocus,
  (v) => { if (v) focusIdx(0) },
  { immediate: true },
)
watch(
  () => props.modelValue,
  (v) => { if (v.length === props.length) emit('complete', v) },
)
onMounted(() => { if (props.autofocus) focusIdx(0) })

defineExpose({ focusFirst })
</script>

<style scoped>
.pin-input { display: flex; gap: 12px; justify-content: center; }
.pin-cell {
  width: 48px; height: 56px;
  text-align: center;
  font-size: 24px; font-weight: 700;
  border: 2px solid var(--jc-border);
  border-radius: 10px;
  background: var(--jc-bg);
  color: var(--jc-text);
  outline: none;
  transition: border-color .15s;
}
.pin-cell:focus { border-color: var(--el-color-primary); }
.pin-cell:disabled { opacity: .6; }
.pin-error .pin-cell { border-color: var(--el-color-danger); animation: pin-shake .4s; }
.pin-success .pin-cell { border-color: var(--el-color-success); }
@keyframes pin-shake {
  0%, 100% { transform: translateX(0); }
  25% { transform: translateX(-6px); }
  75% { transform: translateX(6px); }
}
</style>
