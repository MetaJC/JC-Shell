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
  <Teleport to="body">
    <Transition name="ld-fade">
      <div v-if="visible && dialogSize === SIZE_LARGE" class="ld-mask" @mousedown.self="onMaskClick">
        <div class="ld-window">
          <div class="ld-header">
            <span class="ld-title">{{ title }}</span>
            <button class="ld-close" aria-label="关闭" @click="onClose">×</button>
          </div>

          <div class="ld-body">
            <!--
              内容为后台管理端 markdown-it 渲染后落库的 HTML（管理员配置，可信来源），
              与项目内 UpdateDialog 渲染更新日志的做法一致。
            -->
            <div v-if="contentHtml" class="ld-content" v-html="contentHtml" />
            <div v-else class="ld-empty">暂无内容</div>
          </div>

          <div class="ld-footer">
            <button class="ld-btn ld-btn-primary" @click="onClose">知道了</button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * 客户端启动弹窗
 * 作者：JC
 *
 * 内容由管理员在后台配置（Markdown），后台保存时渲染成 HTML 落库，
 * 这里直接展示 HTML，保证"后台预览效果 = 客户端展示效果"。
 *
 * 宽度限制：max-width 取 min(560px, 88vw)，确保不超过会话窗口（应用主窗口）宽度。
 * 结构与 UpdateDialog 保持一致（Teleport 到 body + 自绘遮罩）。
 */
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue'
import { useLaunchDialogStore, SIZE_LARGE } from '@/stores/launchDialog'
import { useI18nStore } from '@/stores/i18n'

const store = useLaunchDialogStore()
const i18n = useI18nStore()

const visible = ref(false)
const contentHtml = ref('')

/** 弹窗尺寸（LARGE=大弹窗 / SMALL=小弹窗由 UserAvatar 渲染），本组件只负责大弹窗 */
const dialogSize = computed(() => store.info?.dialogSize || SIZE_LARGE)

/** 未配置标题时给个兜底文案 */
const title = computed(() => store.info?.title?.trim() || i18n.t('launchDialog.defaultTitle') || '公告')

function syncFromStore() {
  visible.value = store.visible
  contentHtml.value = store.info?.contentHtml || ''
}

watch(
  () => [store.visible, store.info],
  () => { syncFromStore() },
  { deep: true, immediate: true }
)

function onClose() {
  store.closeDialog()
}

function onMaskClick() {
  onClose()
}

/** ESC 关闭 */
function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape' && visible.value) onClose()
}
onMounted(() => window.addEventListener('keydown', onKey))
onBeforeUnmount(() => window.removeEventListener('keydown', onKey))
</script>

<style scoped>
.ld-mask {
  position: fixed;
  inset: 0;
  z-index: 999999;
  background: rgba(0, 0, 0, 0.55);
  display: flex;
  align-items: center;
  justify-content: center;
}
.ld-window {
  /* 不允许超过会话窗口（应用主窗口）宽度 */
  width: 560px;
  max-width: min(560px, 88vw);
  max-height: 86vh;
  display: flex;
  flex-direction: column;
  background: var(--jc-bg, #1f1f23);
  color: var(--jc-text, #e8e6e0);
  border: 1px solid var(--jc-border, #3a3a40);
  border-radius: 10px;
  box-shadow: 0 12px 40px rgba(0, 0, 0, 0.45);
  overflow: hidden;
  font-size: 14px;
}
.ld-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 14px 18px;
  border-bottom: 1px solid var(--jc-border, #3a3a40);
  font-size: 15px;
  font-weight: 600;
}
.ld-close {
  background: transparent;
  border: none;
  color: var(--jc-text-muted, #999);
  font-size: 22px;
  line-height: 1;
  width: 28px;
  height: 28px;
  border-radius: 4px;
  cursor: pointer;
}
.ld-close:hover {
  background: var(--jc-bg-soft, #2a2a30);
  color: var(--jc-text, #e8e6e0);
}

.ld-body {
  flex: 1;
  min-height: 0;
  padding: 16px 18px;
  overflow: auto;
}
.ld-empty {
  padding: 30px 0;
  text-align: center;
  color: var(--jc-text-muted, #999);
}

.ld-footer {
  display: flex;
  justify-content: flex-end;
  padding: 12px 18px;
  border-top: 1px solid var(--jc-border, #3a3a40);
  background: var(--jc-bg-soft, #2a2a30);
}
.ld-btn {
  height: 32px;
  padding: 0 16px;
  border: 1px solid var(--jc-border, #3a3a40);
  background: transparent;
  color: var(--jc-text, #e8e6e0);
  border-radius: 5px;
  font-size: 13px;
  cursor: pointer;
}
.ld-btn:hover {
  background: var(--jc-bg-hover, #333);
  border-color: var(--jc-accent, #409eff);
}
.ld-btn-primary {
  background: var(--jc-accent, #409eff);
  border-color: var(--jc-accent, #409eff);
  color: #fff;
}
.ld-btn-primary:hover {
  filter: brightness(1.1);
  background: var(--jc-accent, #409eff);
}

.ld-fade-enter-active,
.ld-fade-leave-active {
  transition: opacity 0.18s;
}
.ld-fade-enter-from,
.ld-fade-leave-to {
  opacity: 0;
}
</style>

<style>
/* 弹窗内容（后台 Markdown 渲染后的 HTML）：暗色主题适配，图片不溢出弹窗 */
.ld-content {
  font-size: 14px;
  line-height: 1.75;
  color: var(--jc-text, #e8e6e0);
  word-break: break-word;
}
.ld-content h1,
.ld-content h2,
.ld-content h3,
.ld-content h4 {
  margin: 16px 0 8px;
  font-weight: 700;
  line-height: 1.4;
}
.ld-content h1 { font-size: 20px; }
.ld-content h2 { font-size: 18px; }
.ld-content h3 { font-size: 16px; }
.ld-content p { margin: 8px 0; }
.ld-content ul,
.ld-content ol { margin: 8px 0; padding-left: 22px; }
.ld-content li { margin: 3px 0; }
.ld-content a { color: var(--jc-accent, #409eff); }
.ld-content code {
  background: var(--jc-bg-hover, #333);
  padding: 1px 6px;
  border-radius: 4px;
  font-size: 13px;
}
.ld-content pre {
  background: var(--jc-bg-hover, #333);
  padding: 12px 14px;
  border-radius: 6px;
  overflow: auto;
}
.ld-content pre code { background: transparent; padding: 0; }
.ld-content blockquote {
  border-left: 3px solid var(--jc-accent, #409eff);
  padding: 4px 12px;
  margin: 10px 0;
  color: var(--jc-text-muted, #999);
  background: var(--jc-bg-hover, #333);
  border-radius: 0 4px 4px 0;
}
.ld-content table { border-collapse: collapse; margin: 10px 0; }
.ld-content th,
.ld-content td {
  border: 1px solid var(--jc-border, #3a3a40);
  padding: 6px 10px;
}
.ld-content img {
  max-width: 100%;
  height: auto;
  border-radius: 6px;
}
.ld-content hr {
  border: 0;
  border-top: 1px solid var(--jc-border, #3a3a40);
  margin: 14px 0;
}
</style>
