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
 * 启动弹窗（小弹窗模式 SIZE_SMALL）
 * 作者：JC
 *
 * 渲染在左侧会话面板（TerminalPage 左 aside）底部：
 *   - 宽度 = 会话面板宽度（随面板拖拽同步）
 *   - 距底边距离恒定（贴底）
 *   - 高度向上延伸（max-height 限高，内容滚动）
 * 大弹窗（SIZE_LARGE）仍由 LaunchDialog 全屏渲染，两者按 dialogSize 互斥。
 */
import { computed } from 'vue'
import { useLaunchDialogStore, SIZE_SMALL, SIZE_LARGE } from '@/stores/launchDialog'
import { useI18nStore } from '@/stores/i18n'

const store = useLaunchDialogStore()
const i18n = useI18nStore()

const visible = computed(() => store.visible && (store.info?.dialogSize || SIZE_LARGE) === SIZE_SMALL)
const title = computed(() => store.info?.title?.trim() || i18n.t('launchDialog.defaultTitle') || '公告')
const contentHtml = computed(() => store.info?.contentHtml || '')
</script>

<template>
  <transition name="lp-fade">
    <div v-if="visible" class="launch-pop" @click.stop>
      <div class="launch-pop-header">
        <span class="launch-pop-title">{{ title }}</span>
        <button class="launch-pop-close" aria-label="关闭" @click.stop="store.closeDialog()">×</button>
      </div>
      <div class="launch-pop-body">
        <!-- 内容为后台管理端渲染后落库的 HTML（管理员配置，可信来源） -->
        <div v-if="contentHtml" class="ld-content" v-html="contentHtml" />
        <div v-else class="launch-pop-empty">暂无内容</div>
      </div>
    </div>
  </transition>
</template>

<style scoped>
.launch-pop {
  position: absolute;
  left: 0;
  right: 0;
  bottom: 0;
  z-index: 30;
  display: flex;
  flex-direction: column;
  max-height: 70%;
  background: var(--jc-bg-panel);
  /* 浮层感：四周描边 + 上方投影（底边贴窗，向下投影被裁掉无所谓） */
  border: 1px solid var(--jc-border);
  border-bottom: none;
  border-radius: 10px 10px 0 0;
  box-shadow: 0 -12px 32px rgba(0, 0, 0, 0.45), 0 0 0 1px rgba(0, 0, 0, 0.15);
  overflow: hidden;
}
.launch-pop-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-shrink: 0;
  padding: 10px 12px;
  border-bottom: 1px solid var(--jc-border);
  background: var(--jc-bg-soft);
  font-size: 14px;
  font-weight: 600;
  color: var(--jc-text);
}
.launch-pop-title { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.launch-pop-close {
  flex-shrink: 0;
  margin-left: 8px;
  background: transparent;
  border: none;
  color: var(--jc-text-muted);
  font-size: 20px;
  line-height: 1;
  width: 24px;
  height: 24px;
  border-radius: 4px;
  cursor: pointer;
}
.launch-pop-close:hover { background: var(--jc-bg-soft); color: var(--jc-text); }
.launch-pop-body { padding: 12px; overflow: auto; min-height: 0; }
.launch-pop-empty { padding: 16px 0; text-align: center; color: var(--jc-text-muted); font-size: 13px; }

.lp-fade-enter-active, .lp-fade-leave-active { transition: opacity 0.15s ease, transform 0.15s ease; }
.lp-fade-enter-from, .lp-fade-leave-to { opacity: 0; transform: translateY(8px); }
</style>
