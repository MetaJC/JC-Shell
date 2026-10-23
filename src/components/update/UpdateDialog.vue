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
    <Transition name="ud-fade">
      <div
        v-if="show"
        class="ud-mask"
        @mousedown.self="onMaskClick"
      >
        <div class="ud-window" :class="{ 'ud-mandatory': isMandatory }">
          <div class="ud-header">
            <span class="ud-title">{{ dialogTitle }}</span>
            <button
              v-if="!isMandatory && !installing"
              class="ud-close"
              aria-label="关闭"
              @click="doClose"
            >×</button>
          </div>

          <div class="ud-body">
            <div v-if="checking && !latest" class="ud-state">
              <div class="ud-spinner" />
              <div class="ud-state-text">正在检查更新…</div>
            </div>

            <div v-else-if="!latest && !errMsg && !checking" class="ud-state">
              <div class="ud-icon ud-ok">✓</div>
              <div class="ud-state-title">已是最新版本</div>
              <div class="ud-state-sub">当前版本 {{ currentVersion }}，暂无新版本</div>
            </div>

            <div v-else-if="!latest && errMsg" class="ud-state">
              <div class="ud-icon ud-err">!</div>
              <div class="ud-state-title">检查更新失败</div>
              <div class="ud-state-sub">{{ errMsg }}</div>
            </div>

            <template v-else-if="latest">
              <div class="ud-row">
                <span class="ud-label">当前</span>
                <span class="ud-ver">{{ currentVersion }}</span>
                <span class="ud-arrow">→</span>
                <span class="ud-label">最新</span>
                <span class="ud-ver ud-ver-new">{{ latest.versionCode }}</span>
                <span v-if="isMandatory" class="ud-tag ud-tag-danger">强制</span>
                <span v-if="ready" class="ud-tag ud-tag-ok">已下载</span>
                <span v-if="ignoredNow" class="ud-tag ud-tag-info">已忽略</span>
              </div>
              <div v-if="latest.publishTime" class="ud-pub">发布时间：{{ latest.publishTime }}</div>

              <!-- 下载中显示进度条 -->
              <div v-if="downloading" class="ud-progress-box">
                <div class="ud-progress">
                  <div class="ud-progress-bar" :style="{ width: progress + '%' }" />
                  <div class="ud-progress-text">{{ progress }}%</div>
                </div>
                <div class="ud-hint">正在后台下载；可关闭此窗口，下载完成后会自动安装并重启</div>
              </div>

              <!-- 手动更新：下载完成即自动安装重启，这里给出明确提示 -->
              <div v-if="installing" class="ud-alert">
                正在安装并重启，应用即将自动关闭并重新打开…
              </div>

              <div v-if="ignoredNow" class="ud-alert">
                已忽略 v{{ latest.versionCode }} 的更新提示，下个版本发布后将自动恢复检查。
              </div>

              <div v-if="latest.changelog && !ignoredNow && !downloading" class="ud-changelog">
                <div class="ud-changelog-title">更新日志</div>
                <div class="ud-changelog-body" v-html="renderedChangelog" />
              </div>

              <div v-if="errMsg" class="ud-err-line">{{ errMsg }}</div>
            </template>
          </div>

          <div class="ud-footer">
            <template v-if="installing">
              <div class="ud-footer-left" />
              <div class="ud-footer-right">
                <span class="ud-hint">正在安装并重启…</span>
              </div>
            </template>

            <template v-else-if="!latest">
              <div class="ud-footer-left" />
              <div class="ud-footer-right">
                <button class="ud-btn" :disabled="checking" @click="doCheck">重新检查</button>
                <button class="ud-btn ud-btn-primary" :disabled="checking" @click="doClose">关闭</button>
              </div>
            </template>

            <template v-else-if="ignoredNow">
              <div class="ud-footer-left" />
              <div class="ud-footer-right">
                <button class="ud-btn" @click="doClose">关闭</button>
                <button class="ud-btn" @click="doUnignore">恢复检查</button>
                <button v-if="ready" class="ud-btn ud-btn-primary" @click="doInstall">立即安装并重启</button>
              </div>
            </template>

            <template v-else-if="isMandatory">
              <div class="ud-footer-left" />
              <div class="ud-footer-right">
                <template v-if="!ready && !downloading">
                  <button class="ud-btn ud-btn-primary" @click="doDownload">立即下载</button>
                </template>
                <template v-else-if="downloading">
                  <button class="ud-btn" @click="doCancel">取消下载</button>
                </template>
                <template v-else>
                  <button class="ud-btn ud-btn-primary" @click="doInstall">立即安装并重启</button>
                </template>
              </div>
            </template>

            <template v-else-if="isStartupMode">
              <div class="ud-footer-left">
                <label class="ud-checkbox ud-checkbox-inline" v-if="!ready && !downloading">
                  <input type="checkbox" v-model="ignoreThis" />
                  <span>忽略此版本</span>
                </label>
              </div>
              <div class="ud-footer-right">
                <template v-if="downloading">
                  <button class="ud-btn" @click="doCancel">取消下载</button>
                </template>
                <template v-else-if="ready">
                  <button class="ud-btn" @click="doClose">稍后再说</button>
                  <button class="ud-btn ud-btn-primary" @click="doInstall">立即安装并重启</button>
                </template>
                <template v-else>
                  <button class="ud-btn" @click="doClose">稍后再说</button>
                  <button class="ud-btn ud-btn-primary" :disabled="ignoreThis" @click="doDownload">
                    立即更新
                  </button>
                </template>
              </div>
            </template>

            <template v-else>
              <div class="ud-footer-left">
                <label class="ud-checkbox ud-checkbox-inline" v-if="!downloading">
                  <input type="checkbox" v-model="ignoreThis" />
                  <span>忽略此版本</span>
                </label>
              </div>
              <div class="ud-footer-right">
                <template v-if="downloading">
                  <button class="ud-btn" @click="doCancel">取消下载</button>
                </template>
                <template v-else-if="ready">
                  <button class="ud-btn" @click="doClose">稍后再说</button>
                  <button class="ud-btn ud-btn-primary" @click="doInstall">立即安装并重启</button>
                </template>
                <template v-else>
                  <button class="ud-btn" @click="doClose">稍后再说</button>
                  <button class="ud-btn ud-btn-primary" @click="doDownload">立即更新</button>
                </template>
              </div>
            </template>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * 更新弹窗（自研实现，Teleport 到 body）
 * 作者：JC
 */
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue'
import { useUpdateStore } from '@/stores/update'

const updateStore = useUpdateStore()

// 本地 ref（显式 watch 同步 store）
const show = ref(false)
const latest = ref<any>(null)
const errMsg = ref<string | null>(null)
const statusRef = ref<string>('IDLE')
const ignoredNow = ref(false)
const progress = ref(0)
const currentVersion = ref(__APP_VERSION__)
const platform = ref('WINDOWS_X64')
const isMandatory = ref(false)
const ready = ref(false)
const downloading = ref(false)
const installing = ref(false)
const checking = ref(false)
const isStartupMode = ref(false)

/** 同步 store 状态到本地 */
function syncFromStore() {
  show.value = updateStore.dialogVisible
  latest.value = updateStore.latestInfo
  errMsg.value = updateStore.errorMsg
  statusRef.value = updateStore.status
  ignoredNow.value = updateStore.currentIgnored
  progress.value = updateStore.downloadProgress
  currentVersion.value = updateStore.currentVersion
  platform.value = updateStore.platform
  isStartupMode.value = updateStore.startupCheckMode === 'startup'
  isMandatory.value = !!latest.value?.isMandatory
  ready.value = statusRef.value === 'DOWNLOADED' && !!updateStore.downloadInfo
  downloading.value = statusRef.value === 'DOWNLOADING'
  installing.value = statusRef.value === 'INSTALLING'
  checking.value = statusRef.value === 'CHECKING'
}

watch(
  () => [
    updateStore.dialogVisible,
    updateStore.latestInfo,
    updateStore.errorMsg,
    updateStore.status,
    updateStore.currentIgnored,
    updateStore.downloadProgress,
    updateStore.downloadInfo,
  ],
  () => { syncFromStore() },
  { deep: true, immediate: true }
)

const ignoreThis = ref(false)

const dialogTitle = computed(() => {
  if (installing.value) return '正在安装更新'
  if (checking.value) return '正在检查更新'
  if (!latest.value) return errMsg.value ? '检查更新失败' : '检查更新'
  if (ignoredNow.value) return `已忽略 v${latest.value.versionCode}`
  if (isMandatory.value) return '强制更新'
  if (ready.value) return '新版本已就绪'
  return `发现新版本 v${latest.value.versionCode}`
})

const renderedChangelog = ref('')

function renderChangelog(md: string): string {
  const esc = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
  const lines = esc(md).split(/\r?\n/)
  let html = '', inCode = false, inUl = false
  const closeUl = () => { if (inUl) { html += '</ul>'; inUl = false } }
  for (const raw of lines) {
    const line = raw.replace(/\s+$/, '')
    if (line.startsWith('`')) {
      closeUl()
      if (inCode) html += '</code></pre>'; else html += '<pre><code>'
      inCode = !inCode
      continue
    }
    if (inCode) { html += esc(raw) + '\n'; continue }
    if (line.startsWith('### ')) { closeUl(); html += '<h5>' + esc(line.slice(4).trim()) + '</h5>' }
    else if (line.startsWith('## ')) { closeUl(); html += '<h4>' + esc(line.slice(3).trim()) + '</h4>' }
    else if (line.startsWith('# ')) { closeUl(); html += '<h3>' + esc(line.slice(2).trim()) + '</h3>' }
    else if (/^\s*[-*]\s+/.test(line)) {
      if (!inUl) { html += '<ul>'; inUl = true }
      html += '<li>' + esc(line.replace(/^\s*[-*]\s+/, '')) + '</li>'
    } else if (line.trim() === '') { closeUl(); html += '<br/>' }
    else { closeUl(); html += '<p>' + esc(line) + '</p>' }
  }
  closeUl()
  return html
}

async function doCheck() { await updateStore.checkUpdate('manual') }
async function doDownload() { await updateStore.startDownload() }
async function doCancel() { await updateStore.cancelDownload() }
async function doInstall() { await updateStore.installNow() }
function doUnignore() {
  if (latest.value) updateStore.unignoreVersion(latest.value.versionCode)
}
function doClose() {
  // 关闭时持久化"忽略此版本"勾选
  if (ignoreThis.value && latest.value && !ignoredNow.value) {
    updateStore.ignoreVersion(latest.value.versionCode)
    // 启动模式：5min 后台自动下载
    if (isStartupMode.value) {
      updateStore.scheduleBackgroundDownload(latest.value.versionCode, 5 * 60 * 1000)
    }
  } else if (isStartupMode.value && !ready.value && !downloading.value && !ignoredNow.value) {
    // 没勾忽略但点了稍后更新 → 5min 后自动下载
    if (latest.value) {
      updateStore.scheduleBackgroundDownload(latest.value.versionCode, 5 * 60 * 1000)
    }
  }
  ignoreThis.value = false
  updateStore.closeDialog()
}
function onMaskClick() {
  // 下载中允许关闭窗口（下载/安装会继续在后台进行）；安装中与强制更新不允许
  if (isMandatory.value || installing.value) return
  doClose()
}

watch(() => latest.value?.changelog, (md) => {
  if (md) renderedChangelog.value = renderChangelog(md)
}, { immediate: true })

watch(show, (v) => {
  if (!v) ignoreThis.value = false
})

function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape' && show.value && !isMandatory.value && !installing.value) {
    doClose()
  }
}
onMounted(() => window.addEventListener('keydown', onKey))
onBeforeUnmount(() => window.removeEventListener('keydown', onKey))
</script>

<style scoped>
.ud-mask {
  position: fixed;
  inset: 0;
  z-index: 999999;
  background: rgba(0, 0, 0, 0.55);
  display: flex;
  align-items: center;
  justify-content: center;
}
.ud-window {
  width: 540px;
  max-width: 92vw;
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
.ud-mandatory { border-color: var(--el-color-danger, #f56c6c); }
.ud-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 14px 18px;
  border-bottom: 1px solid var(--jc-border, #3a3a40);
  font-size: 15px;
  font-weight: 600;
}
.ud-close {
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
.ud-close:hover { background: var(--jc-bg-soft, #2a2a30); color: var(--jc-text, #e8e6e0); }

.ud-body {
  flex: 1;
  min-height: 0;
  padding: 16px 18px;
  overflow: auto;
}
.ud-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  padding: 30px 0;
  gap: 8px;
}
.ud-state-text { color: var(--jc-text-muted, #999); }
.ud-state-title { font-size: 16px; font-weight: 600; }
.ud-state-sub { color: var(--jc-text-muted, #999); font-size: 13px; }
.ud-icon {
  width: 48px; height: 48px;
  border-radius: 50%;
  display: flex; align-items: center; justify-content: center;
  font-size: 28px; font-weight: 700; color: #fff;
}
.ud-ok { background: var(--el-color-success, #67c23a); }
.ud-err { background: var(--el-color-danger, #f56c6c); }
.ud-spinner {
  width: 36px; height: 36px;
  border: 3px solid var(--jc-border, #3a3a40);
  border-top-color: var(--jc-accent, #409eff);
  border-radius: 50%;
  animation: ud-spin 0.9s linear infinite;
}
@keyframes ud-spin { to { transform: rotate(360deg); } }

.ud-row {
  display: flex; align-items: center; gap: 8px; flex-wrap: wrap;
  font-size: 14px;
  padding-bottom: 12px;
  border-bottom: 1px solid var(--jc-border, #3a3a40);
  margin-bottom: 12px;
}
.ud-label { color: var(--jc-text-muted, #999); font-size: 12px; }
.ud-arrow { color: var(--jc-text-muted, #999); }
.ud-ver { font-size: 14px; }
.ud-ver-new { color: var(--jc-accent, #409eff); font-weight: 600; font-size: 15px; }
.ud-tag {
  font-size: 11px; padding: 2px 6px; border-radius: 3px;
  border: 1px solid;
}
.ud-tag-danger { color: var(--el-color-danger, #f56c6c); border-color: var(--el-color-danger, #f56c6c); }
.ud-tag-ok { color: var(--el-color-success, #67c23a); border-color: var(--el-color-success, #67c23a); }
.ud-tag-info { color: var(--jc-text-muted, #999); border-color: var(--jc-text-muted, #999); }
.ud-pub { color: var(--jc-text-muted, #999); font-size: 12px; margin-bottom: 8px; }

.ud-progress-box {
  padding: 12px;
  background: var(--jc-bg-soft, #2a2a30);
  border-radius: 6px;
  margin-bottom: 12px;
}
.ud-progress {
  position: relative;
  height: 16px;
  background: var(--jc-bg-hover, #333);
  border-radius: 8px;
  overflow: hidden;
}
.ud-progress-bar {
  height: 100%;
  background: var(--jc-accent, #409eff);
  transition: width 0.2s;
}
.ud-progress-text {
  position: absolute;
  inset: 0;
  display: flex; align-items: center; justify-content: center;
  font-size: 12px; font-weight: 600;
  color: var(--jc-text, #e8e6e0);
}
.ud-hint { margin-top: 6px; font-size: 12px; color: var(--jc-text-muted, #999); text-align: center; }

.ud-alert {
  background: var(--jc-bg-soft, #2a2a30);
  border-left: 3px solid var(--jc-accent, #409eff);
  padding: 8px 12px;
  border-radius: 4px;
  margin-bottom: 12px;
  font-size: 13px;
  color: var(--jc-text-muted, #999);
}

.ud-changelog {
  max-height: 220px;
  overflow: auto;
  border: 1px solid var(--jc-border, #3a3a40);
  border-radius: 6px;
  padding: 10px 14px;
  margin-bottom: 8px;
}
.ud-changelog-title { font-size: 12px; color: var(--jc-text-muted, #999); margin-bottom: 6px; }
.ud-changelog-body :deep(h3), .ud-changelog-body :deep(h4), .ud-changelog-body :deep(h5) {
  margin: 6px 0 4px; font-size: 14px;
}
.ud-changelog-body :deep(p) { margin: 4px 0; font-size: 13px; line-height: 1.6; }
.ud-changelog-body :deep(li) { font-size: 13px; margin-left: 18px; line-height: 1.8; list-style: disc; }
.ud-changelog-body :deep(pre) {
  background: var(--jc-bg-hover, #333); padding: 8px 10px; border-radius: 4px; font-size: 12px; overflow-x: auto;
}
.ud-err-line { margin-top: 8px; color: var(--el-color-danger, #f56c6c); font-size: 12px; }

.ud-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 12px 18px;
  border-top: 1px solid var(--jc-border, #3a3a40);
  background: var(--jc-bg-soft, #2a2a30);
}
.ud-footer-left {
  display: flex;
  align-items: center;
  flex: 1;
  min-width: 0;
}
.ud-footer-right {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}
.ud-checkbox-right { margin-left: 8px; }
.ud-checkbox {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  cursor: pointer;
  user-select: none;
  font-size: 13px;
  color: var(--jc-text-muted, #999);
}
.ud-checkbox input { cursor: pointer; }

.ud-btn {
  height: 32px;
  padding: 0 14px;
  border: 1px solid var(--jc-border, #3a3a40);
  background: transparent;
  color: var(--jc-text, #e8e6e0);
  border-radius: 5px;
  font-size: 13px;
  cursor: pointer;
  transition: background 0.15s, border-color 0.15s;
}
.ud-btn:hover:not(:disabled) {
  background: var(--jc-bg-hover, #333);
  border-color: var(--jc-accent, #409eff);
}
.ud-btn:disabled { opacity: 0.5; cursor: not-allowed; }
.ud-btn-primary {
  background: var(--jc-accent, #409eff);
  border-color: var(--jc-accent, #409eff);
  color: #fff;
}
.ud-btn-primary:hover:not(:disabled) {
  filter: brightness(1.1);
  background: var(--jc-accent, #409eff);
}

.ud-fade-enter-active, .ud-fade-leave-active { transition: opacity 0.18s; }
.ud-fade-enter-from, .ud-fade-leave-to { opacity: 0; }
</style>
