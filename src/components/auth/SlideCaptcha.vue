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
 * 滑动拼图验证码弹窗 (AJ-Captcha 适配版，APP 内手机号登录专用)
 * 坐标加密（AES-ECB）已由 Rust 端 phone_captcha_check 完成，本组件只负责
 * 拉取拼图、采集滑块位置、把 x 坐标交给后端校验并取回 captchaToken。
 * 作者：JC
 */
import { ref, watch, onBeforeUnmount } from 'vue'
import { invoke } from '@/api'

interface CaptchaImage {
  originalImageBase64: string
  jigsawImageBase64: string
  token: string
  secretKey: string
}

const props = defineProps<{ visible: boolean }>()
const emit = defineEmits<{
  (e: 'update:visible', v: boolean): void
  (e: 'success', payload: { captchaToken: string }): void
  (e: 'close'): void
}>()

const containerRef = ref<HTMLDivElement | null>(null)
const sliderX = ref(0)
const dragging = ref(false)
const startMouseX = ref(0)
const loading = ref(false)
const imageLoading = ref(false)
const image = ref<CaptchaImage | null>(null)
const errorTip = ref('')
const shakeKey = ref(0)
const refreshIconKey = ref(0)
let refreshTimer: number | null = null

const SLIDER_BAR_SIZE = 44
const BG_WIDTH = 310
const SLIDER_WIDTH = 44

const close = () => {
  if (refreshTimer) {
    clearTimeout(refreshTimer)
    refreshTimer = null
  }
  loading.value = false
  dragging.value = false
  emit('update:visible', false)
}

async function fetchImage() {
  imageLoading.value = true
  errorTip.value = ''
  try {
    const data = await invoke<CaptchaImage>('phone_captcha_get')
    image.value = data
    sliderX.value = 0
  } catch (e: any) {
    errorTip.value = e?.message || '拉取验证码失败'
  } finally {
    imageLoading.value = false
    refreshIconKey.value++
  }
}

function onPointerDown(e: PointerEvent) {
  if (imageLoading.value || loading.value || !image.value) return
  dragging.value = true
  startMouseX.value = e.clientX
  ;(e.target as HTMLElement).setPointerCapture(e.pointerId)
}

function onPointerMove(e: PointerEvent) {
  if (!dragging.value || !image.value || !containerRef.value) return
  const dx = e.clientX - startMouseX.value
  const max = BG_WIDTH - SLIDER_WIDTH
  const x = Math.max(0, Math.min(dx, max))
  sliderX.value = x
}

async function onPointerUp(e: PointerEvent) {
  if (!dragging.value || !image.value) return
  dragging.value = false
  ;(e.target as HTMLElement).releasePointerCapture?.(e.pointerId)
  await submitCheck()
}

async function submitCheck() {
  if (!image.value) return
  loading.value = true
  errorTip.value = ''
  try {
    const captchaToken = await invoke<string>('phone_captcha_check', {
      token: image.value.token,
      secretKey: image.value.secretKey,
      x: Math.round(sliderX.value),
    })
    emit('success', { captchaToken })
    emit('update:visible', false)
    return
  } catch (e: any) {
    errorTip.value = e?.message || '验证失败，请重试'
    shakeKey.value++
    sliderX.value = 0
    refreshTimer = window.setTimeout(() => {
      refreshTimer = null
      fetchImage()
    }, 600)
  } finally {
    loading.value = false
  }
}

watch(
  () => props.visible,
  (v) => {
    if (v) {
      fetchImage()
    } else {
      image.value = null
      sliderX.value = 0
      errorTip.value = ''
    }
  }
)

onBeforeUnmount(() => {
  if (refreshTimer) clearTimeout(refreshTimer)
})
</script>

<template>
  <Teleport to="body">
    <Transition name="cap-fade">
      <div v-if="visible" class="cap-mask" @mousedown.self="close">
        <div class="cap-dialog" role="dialog" aria-label="滑动验证">
          <button class="cap-close" type="button" aria-label="关闭" @mousedown.stop @click="close">
            <svg viewBox="0 0 14 14" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" style="pointer-events: none;">
              <path d="M1 1l12 12M13 1L1 13" />
            </svg>
          </button>

          <div class="cap-title">安全验证</div>
          <div class="cap-subtitle">请拖动滑块完成拼图</div>

          <div class="cap-stage" :key="shakeKey" :class="{ shake: shakeKey > 0 }">
            <div ref="containerRef" class="cap-image-box">
              <div v-if="imageLoading && !image" class="cap-image-loading">加载中...</div>
              <template v-else-if="image">
                <img :src="'data:image/png;base64,' + image.originalImageBase64" class="cap-bg" draggable="false" />
                <img
                  :src="'data:image/png;base64,' + image.jigsawImageBase64"
                  class="cap-slider-img"
                  draggable="false"
                  :style="{ left: sliderX + 'px' }"
                />
              </template>
              <button class="cap-refresh" type="button" aria-label="刷新" @click="fetchImage">
                <svg :key="refreshIconKey" viewBox="0 0 16 16" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M2 8a6 6 0 0 1 10.5-4M14 8a6 6 0 0 1-10.5 4" />
                  <path d="M12.5 1.5v3h-3M3.5 14.5v-3h3" />
                </svg>
              </button>
            </div>
          </div>

          <div class="cap-bar">
            <div class="cap-track" :class="{ active: dragging }">
              <div class="cap-track-fill" :style="{ width: sliderX + 'px' }"></div>
              <div class="cap-track-tip">
                <template v-if="loading">校验中...</template>
                <template v-else-if="imageLoading">加载中...</template>
                <template v-else>按住滑块向右拖动</template>
              </div>
              <div
                class="cap-handle"
                :class="{ disabled: loading || imageLoading || !image }"
                :style="{ transform: 'translateX(' + sliderX + 'px)', width: SLIDER_BAR_SIZE + 'px', height: SLIDER_BAR_SIZE + 'px' }"
                @pointerdown="onPointerDown"
                @pointermove="onPointerMove"
                @pointerup="onPointerUp"
                @pointercancel="onPointerUp"
              >
                <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M9 6l-4 6 4 6M15 6l4 6-4 6" />
                </svg>
              </div>
            </div>
          </div>

          <div v-if="errorTip" class="cap-error">{{ errorTip }}</div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>
