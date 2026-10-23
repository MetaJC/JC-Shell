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
  <div class="rc-form" @focusin="onFocusin" @focusout="onFocusout">
    <div class="rc-field">
      <label>{{ t('portForward.field.sshHost') }}</label>
      <el-input :model-value="model.sshHost" size="small" @update:model-value="patch({ sshHost: $event })" />
    </div>
    <div class="rc-field">
      <label>{{ t('portForward.field.sshPort') }}</label>
      <el-input
        :model-value="sshPortText"
        inputmode="numeric"
        :maxlength="5"
        size="small"
        @update:model-value="(v: string) => onSshPortInput(v)"
      />
    </div>
    <div class="rc-field">
      <label>{{ t('portForward.field.sshUser') }}</label>
      <el-input :model-value="model.sshUser" size="small" @update:model-value="patch({ sshUser: $event })" />
    </div>
    <div class="rc-field">
      <label>{{ t('portForward.field.sshAuthType') }}</label>
      <el-radio-group
        :model-value="model.sshAuthType"
        size="small"
        @update:model-value="patch({ sshAuthType: $event as 'password' | 'key' })"
      >
        <el-radio-button value="password">{{ t('portForward.auth.password') }}</el-radio-button>
        <el-radio-button value="key">{{ t('portForward.auth.key') }}</el-radio-button>
      </el-radio-group>
    </div>
    <div v-if="model.sshAuthType !== 'key'" class="rc-field">
      <label>{{ t('portForward.field.sshPassword') }}</label>
      <el-input
        :model-value="model.sshPassword"
        :type="passwordVisible ? 'text' : 'password'"
        size="small"
        @update:model-value="patch({ sshPassword: $event })"
      >
        <template #suffix>
          <el-icon class="rc-eye" @click="togglePasswordVisible">
            <View v-if="!passwordVisible" />
            <Hide v-else />
          </el-icon>
        </template>
      </el-input>
    </div>
    <div v-else class="rc-field">
      <label>{{ t('portForward.field.sshKey') }}</label>
      <div class="rc-key-row">
        <el-input
          :model-value="model.sshKeyName || model.sshKeyPath || ''"
          size="small"
          :placeholder="model.sshKeyContent ? t('portForward.auth.key') : t('portForward.btn.browse')"
          readonly
        />
        <el-button size="small" @click="pickKey">{{ t('portForward.btn.browse') }}</el-button>
      </div>
    </div>
    <div class="rc-btn-row">
      <el-button
        size="small" type="primary" plain
        :disabled="!filled" :loading="testing"
        @click="runTest"
      >{{ t('portForward.remoteClient.testConn') }}</el-button>
      <el-button size="small" @click="$emit('pick')">{{ t('portForward.btn.pickSession') }}</el-button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { ElMessage } from 'element-plus'
import { View, Hide } from '@element-plus/icons-vue'
import { useI18nStore } from '@/stores/i18n'
import { invoke, isTauriEnv } from '@/api'
import { ensureUnlocked } from '@/utils/securityPassword'
import { probeRemoteClient, sshErrorKey, type RemoteClientSsh } from '@/stores/portForward'

const props = withDefaults(
  defineProps<{
    model: RemoteClientSsh
    /** 该客户端向哪个地址发起转发探测（左侧连回环，右侧连监听地址） */
    bindAddr?: string
    /** 服务器监听端口（探测转发通道用） */
    remotePort?: number
  }>(),
  { bindAddr: '127.0.0.1', remotePort: 0 },
)
const emit = defineEmits<{
  (e: 'update', patch: Partial<RemoteClientSsh>): void
  (e: 'pick'): void
  (e: 'focus'): void
  (e: 'blur'): void
  (e: 'test', state: 'ok' | 'fail' | 'idle'): void
}>()
const t = useI18nStore().t

/** SSH 密码是否明文显示（默认隐藏） */
const passwordVisible = ref(false)

/** SSH 端口显示文本：值恒被钳制到 [1, 65535]，空/非法按默认 22 显示 */
const sshPortText = computed(() => {
  const n = Number(props.model.sshPort)
  if (!Number.isFinite(n) || n <= 0) return '22'
  return String(Math.min(n, 65535))
})

/**
 * SSH 端口「强制阻止」输入：逐键即时处理，超界 / 超长字符直接丢弃不上屏。
 * - 只保留数字，且按住 5 位上限截断（65535 为最大 5 位）→ 超出的位根本输不进去；
 * - 解析后值 > 65535 立即变为 65535 → 输入 70000 实时显示成 65535；
 * 因 el-input 为受控组件，过滤后的值写回即同步刷新界面，无需等失焦。
 */
function onSshPortInput(raw: string): void {
  const digits = String(raw ?? '').replace(/\D/g, '').slice(0, 5)
  let n = digits ? Number(digits) : 22
  if (n < 1) n = 22
  if (n > 65535) {
    n = 65535
    ElMessage.warning(t('portForward.tip.portRange'))
  }
  patch({ sshPort: n })
}

/**
 * 切换 SSH 密码明文可见性。由隐藏切换为显示前，需按用户设置的保护范围验证安全密码
 * （未开启安全密码 / 未勾选「查看会话密码」时直接放行）。
 */
async function togglePasswordVisible(): Promise<void> {
  if (!passwordVisible.value) {
    if (!(await ensureUnlocked('view_password'))) return
  }
  passwordVisible.value = !passwordVisible.value
}

// 切换认证方式后回到隐藏态，避免明文长期暴露
watch(() => props.model.sshAuthType, () => { passwordVisible.value = false })

/** 焦点进入/离开整个表单时通知父组件（表单内部字段间切换不触发），用于联动高亮架构图节点 */
function onFocusin(e: FocusEvent): void {
  const root = e.currentTarget as HTMLElement
  if (e.relatedTarget && root.contains(e.relatedTarget as Node)) return
  emit('focus')
}
function onFocusout(e: FocusEvent): void {
  const root = e.currentTarget as HTMLElement
  if (e.relatedTarget && root.contains(e.relatedTarget as Node)) return
  emit('blur')
}

/** 表单是否已填写完整（决定「测试连接」是否可点击） */
const filled = computed(() => {
  const m = props.model
  if (!m.sshHost.trim() || !m.sshUser.trim()) return false
  return m.sshAuthType === 'key' ? !!(m.sshKeyContent || m.sshKeyPath) : !!m.sshPassword
})

/** 单台客户端连接测试进行中（按钮 loading 用） */
const testing = ref(false)
// 内容一旦改动，之前的测试结论即失效
watch(() => props.model, () => emit('test', 'idle'))

/** 单独测试这台远程客户端的 SSH 连接，结果上抛（外层卡片据此显示悬浮绿/红态） */
async function runTest(): Promise<void> {
  if (testing.value || !filled.value) return
  testing.value = true
  try {
    const res = await probeRemoteClient(props.model, props.bindAddr, props.remotePort)
    if (res.sshOk) {
      emit('test', 'ok')
      ElMessage.success(t('portForward.remoteClient.probeOk'))
    } else {
      emit('test', 'fail')
      // 原始报错只进控制台（便于排查），用户界面只给友好提示
      console.warn('[portForward] 远程客户端 SSH 探测失败原始报错:', res.sshErr)
      ElMessage.error(t('portForward.remoteClient.probeFail', { reason: t(sshErrorKey(res.sshErr)) }))
    }
  } catch (e) {
    emit('test', 'fail')
    console.warn('[portForward] 远程客户端 SSH 探测失败原始报错:', e)
    ElMessage.error(t('portForward.remoteClient.probeFail', { reason: t(sshErrorKey(String((e as Error)?.message ?? e))) }))
  } finally {
    testing.value = false
  }
}

function patch(p: Partial<RemoteClientSsh>): void {
  emit('update', p)
}

async function pickKey(): Promise<void> {
  if (!isTauriEnv()) return
  try {
    const path = await invoke<string | null>('pick_key_file')
    if (!path) return
    const data = await invoke<{ content: string; name: string }>('read_key_file', { path })
    emit('update', {
      sshKeyPath: path,
      sshKeyContent: data.content,
      sshKeyName: data.name || path.split(/[\\/]/).pop() || '',
    })
  } catch (e) {
    console.warn('[portForward] 读取私钥文件失败:', e)
  }
}
</script>

<style scoped>
.rc-form {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
/* 「测试连接 + 选择会话」并排 */
.rc-btn-row {
  display: flex;
  gap: 8px;
}
.rc-btn-row .el-button {
  flex: 1;
  margin-left: 0;
}
.rc-field {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.rc-field label {
  font-size: 11px;
  color: var(--jc-text-muted);
}
.rc-key-row {
  display: flex;
  gap: 6px;
}
.rc-key-row .el-input {
  flex: 1;
  min-width: 0;
}
/* 密码可见性切换的小眼睛 */
.rc-eye {
  cursor: pointer;
  color: var(--jc-text-muted);
  transition: color 0.15s ease;
}
.rc-eye:hover {
  color: var(--jc-accent);
}

/* 控件统一加高到 36px，与会话弹窗（会话列表）的表单输入框/按钮对齐：
 * 本组件控件默认 size="small"（约 24px），通过覆盖实际尺寸加高 */
.rc-form :deep(.el-input__wrapper) {
  min-height: 36px;
}
.rc-form :deep(.el-input-number) {
  height: 36px;
}
.rc-form :deep(.el-radio-button__inner) {
  height: 36px;
  line-height: 36px;
}
.rc-form :deep(.el-button) {
  height: 36px;
}
</style>
