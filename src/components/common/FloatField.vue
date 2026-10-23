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
 * 浮动标签输入容器（Material Design 风格）
 * 作者：JC
 *
 * 统一实现「新建/编辑会话」弹窗的表单交互（也可复用到其他弹窗）：
 *   - 未聚焦且为空：label 显示在输入框内部（充当 placeholder）
 *   - 获得焦点 或 已有内容：label 上浮到输入框左上角、骑在边框线上
 *   - 聚焦且为空：hint 显示在输入框内部（如「如：localhost」），输入内容后消失
 *
 * 用法：<FloatField label="主机地址" hint="如：localhost" v-model:value="form.host">
 *         <el-input v-model="form.host" placeholder="" />
 *       </FloatField>
 * 注意：内部 el-input/el-select 的原生 placeholder 请留空，由本组件统一接管两种文案。
 */
import { computed, ref } from 'vue'

const props = defineProps<{
  /** 标签文案：未聚焦且为空时在输入框内部，聚焦或有值后悬浮到边框上 */
  label: string
  /** 获得焦点且为空时显示在输入框内部的提示（输入内容后消失） */
  hint?: string
  /** 当前值：非空时 label 固定悬浮在边框上 */
  value?: string | number | null
  /** 必填标记（label 后带红色星号） */
  required?: boolean
}>()

const focused = ref(false)
const hasValue = computed(() => {
  const v = props.value
  return v !== undefined && v !== null && String(v).trim() !== ''
})
/** label 是否处于悬浮态：聚焦 或 已有内容 */
const floated = computed(() => focused.value || hasValue.value)
</script>

<template>
  <div
    class="float-field"
    :class="{ 'is-floated': floated, 'is-focused': focused }"
    @focusin="focused = true"
    @focusout="focused = false"
  >
    <label class="ff-label">
      {{ label }}<span v-if="required" class="ff-req"> *</span>
    </label>
    <slot />
    <span v-if="hint" class="ff-hint" :class="{ 'ff-hint--show': focused && !hasValue }" aria-hidden="true">
      {{ hint }}
    </span>
  </div>
</template>

<style scoped>
.float-field {
  position: relative;
  width: 100%;
}

/* ---- 浮动标签 ---- */
.ff-label {
  position: absolute;
  left: 10px;
  top: 50%;
  transform: translateY(-50%);
  max-width: calc(100% - 24px);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12px;
  line-height: 1;
  color: var(--jc-text-muted);
  pointer-events: none;
  /* 上下留 1px 内边距：容器比字形略大，既不被 overflow 裁掉字形，
     又能让背景多覆盖边框线 1px，杜绝边框「漏出」造成的遮挡感 */
  padding: 1px 4px;
  border-radius: 3px;
  background: transparent;
  transition: top 0.15s ease, font-size 0.15s ease, color 0.15s ease;
  /* 压在输入框之上（el-input 等均为定位元素，需要明确的更高层级） */
  z-index: 10;
}
.float-field.is-focused .ff-label {
  color: var(--jc-accent);
}
/* 悬浮态：骑在输入框上边框线上（文字垂直中心严格与边框同线），
   靠 z-index 压在输入框之上。
   与「端口转发」表单同一套做法：标签用单色背景，
   前提是输入框内部底色与所在弹窗底色一致（见 jc-session-dialog 的输入框底色覆盖），
   这样标签在弹窗底色与输入框内侧都无缝、只在边框线处「切」出一道缺口 */
.float-field.is-floated .ff-label {
  top: 0;
  transform: translateY(-50%);
  font-size: 11px;
  background: var(--ff-label-bg, var(--jc-bg-panel));
}
.ff-req {
  color: #f87171;
}

/* ---- 聚焦且为空时的内部提示（输入内容后消失） ---- */
.ff-hint {
  position: absolute;
  left: 11px;
  top: 50%;
  transform: translateY(-50%);
  max-width: calc(100% - 24px);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12px;
  color: var(--jc-text-muted);
  pointer-events: none;
  opacity: 0;
  transition: opacity 0.12s ease;
  z-index: 1;
}
.ff-hint--show {
  opacity: 1;
}

/* ---- 输入框增高 ----
   与「端口转发」表单保持同一套观感：内层 input 维持紧凑高度（22px），
   文字行高与选中高亮块只占输入框高度的一部分；盒子整体高度靠 wrapper 的
   上下内边距撑到约 36px，文字由 wrapper 的 align-items:center 垂直居中 */
.float-field :deep(.el-input__wrapper) {
  /* 与「端口转发」表单一致：内层 22px + 上下各 7px = 36px 总高 */
  padding-top: 7px;
  padding-bottom: 7px;
}
.float-field :deep(.el-input__inner) {
  height: 22px;
  line-height: 22px;
  /* 光标用主题色：深色底上默认光标太暗 */
  caret-color: var(--jc-accent);
}
.float-field :deep(.el-select__wrapper) {
  min-height: 36px;
}
.float-field :deep(.el-input-number) {
  height: 36px;
}
/* 数字框右侧上下箭头：各占 50% 高度、上下贴边，中间不留缝隙
   （覆盖 EP 默认的 top:1px / bottom:1px 偏移） */
.float-field :deep(.el-input-number.is-controls-right .el-input-number__increase),
.float-field :deep(.el-input-number.is-controls-right .el-input-number__decrease) {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 50%;
  line-height: 1;
}
.float-field :deep(.el-input-number.is-controls-right .el-input-number__increase) {
  top: 0;
  bottom: auto;
}
.float-field :deep(.el-input-number.is-controls-right .el-input-number__decrease) {
  top: auto;
  bottom: 0;
}
</style>
