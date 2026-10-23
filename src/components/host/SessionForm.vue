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
  <div class="space-y-5">
    <!-- 上级文件夹：新建与编辑都可调整，选择「无」表示该会话直接放在顶级目录 -->
    <div>
      <label class="text-xs text-[var(--jc-text-soft)]">上级分组</label>
      <el-select
        v-model="sessionForm.parentId"
        placeholder="选择上级分组"
        clearable
        filterable
        :teleported="false"
        size="small"
        class="w-full mt-1 tall-field"
      >
        <el-option label="无（顶级目录）" :value="null" />
        <el-option
          v-for="folder in folderOptions"
          :key="folder.id"
          :label="getFolderLabel(folder)"
          :value="folder.id"
        />
      </el-select>
    </div>

    <!-- 会话名称 -->
    <FloatField label="会话名称" hint="请输入会话名称" :value="sessionForm.name" required>
      <el-input v-model="sessionForm.name" size="small" placeholder="" />
    </FloatField>

    <!-- 主机地址 + 端口 -->
    <div class="grid grid-cols-3 gap-2">
      <FloatField
        class="col-span-2"
        label="主机地址"
        hint="如：localhost"
        :value="sessionForm.host"
        required
      >
        <el-input v-model="sessionForm.host" size="small" placeholder="" />
      </FloatField>
      <FloatField label="端口" :value="sessionForm.port">
        <!-- controls 固定到右侧上下堆叠：左侧留空，浮动标签才能骑在输入框左上角边框上 -->
        <el-input-number
          v-model="sessionForm.port"
          size="small"
          :min="1"
          :max="65535"
          controls-position="right"
          class="w-full"
        />
      </FloatField>
    </div>

    <!-- 认证方式：用单选按钮替代 el-select，避免下拉浮层在弹窗内被遮挡/裁剪 -->
    <div>
      <label class="text-xs text-[var(--jc-text-soft)] block">认证方式</label>
      <el-radio-group v-model="sessionForm.authType" size="small" class="mt-1">
        <el-radio-button value="password">密码</el-radio-button>
        <el-radio-button value="key">密钥</el-radio-button>
      </el-radio-group>
    </div>

    <!-- 密码认证 -->
    <template v-if="sessionForm.authType === 'password'">
      <div class="grid grid-cols-2 gap-2">
        <FloatField label="用户名" hint="root" :value="sessionForm.username" required>
          <el-input v-model="sessionForm.username" size="small" placeholder="" />
        </FloatField>
        <FloatField label="密码" hint="请输入密码" :value="sessionForm.password" required>
          <el-input
            v-model="sessionForm.password"
            :type="passwordVisible ? 'text' : 'password'"
            size="small"
            placeholder=""
          >
            <template #suffix>
              <el-icon class="cursor-pointer hover:text-blue-400" @click="togglePasswordVisible('password')">
                <View v-if="!passwordVisible" />
                <Hide v-else />
              </el-icon>
            </template>
          </el-input>
        </FloatField>
      </div>
    </template>

    <!-- 密钥认证 -->
    <template v-else-if="sessionForm.authType === 'key'">
      <FloatField label="用户名" hint="root" :value="sessionForm.username" required>
        <el-input v-model="sessionForm.username" size="small" placeholder="" />
      </FloatField>
      <FloatField
        label="密钥文件"
        :hint="sessionForm.keyContent ? '已托管，删除原文件也不影响连接' : '点击「浏览...」选择私钥文件'"
        :value="sessionForm.keyName || sessionForm.keyPath || ''"
        required
      >
        <div class="flex gap-2">
          <el-input
            :model-value="sessionForm.keyName || sessionForm.keyPath || ''"
            size="small"
            placeholder=""
            readonly
            class="flex-1"
          />
          <el-button class="jc-key-btn" type="primary" plain @click="selectKeyFile">浏览...</el-button>
          <el-button v-if="sessionForm.keyContent" class="jc-key-btn" plain @click="clearKeyFile">清除</el-button>
        </div>
      </FloatField>
      <FloatField label="私钥口令" hint="加密私钥必填，无口令可留空" :value="sessionForm.passphrase">
        <el-input
          v-model="sessionForm.passphrase"
          :type="passphraseVisible ? 'text' : 'password'"
          size="small"
          placeholder=""
        >
          <template #suffix>
            <el-icon class="cursor-pointer hover:text-blue-400" @click="togglePasswordVisible('passphrase')">
              <View v-if="!passphraseVisible" />
              <Hide v-else />
            </el-icon>
          </template>
        </el-input>
      </FloatField>
    </template>
  </div>
</template>

<script setup lang="ts">
/**
 * 会话表单（新建 / 编辑共用）
 * 作者：JC
 *
 * 桌面端由 HostListPanel 的 el-dialog 包裹使用；移动端由 MobileSessionEditPage 作为整页承载。
 * 组件只负责表单本身（不含底部操作按钮）—— 把「测试连接 / 保存 / 保存并连接 / 取消」的
 * 触发权交给外层，桌面与移动端各自的按钮布局/语义不同（移动端多了「保存并连接」）。
 *
 * 对外暴露方法：testConnection / save / saveAndConnect（via defineExpose）；
 * 对外事件：saved（已保存的 HostItem）/ save-and-connect（已保存的 HostItem）/ cancel。
 */
import { ref, watch, onMounted } from 'vue'
import { ElMessage } from 'element-plus'
import { View, Hide } from '@element-plus/icons-vue'
import { invoke } from '@/api'
import { handleApiError } from '@/utils/errorHandler'
import { resolveSshProxy } from '@/utils/sshProxy'
import { ensureUnlocked } from '@/utils/securityPassword'
import { useHostStore, type HostItem } from '@/stores/host'
import { useSettingsStore } from '@/stores/settings'
import { isMobileWebview } from '@/utils/platform'
import { pickLocalFileText } from '@/utils/localFilePick'
import FloatField from '@/components/common/FloatField.vue'

const props = withDefaults(defineProps<{
  /** 新建 / 编辑 */
  mode: 'create' | 'edit'
  /** 编辑时的初始会话（为其深拷贝填充表单）；新建时为 null */
  host?: HostItem | null
  /** 新建时的默认上级文件夹 id（从某文件夹「新建会话」进来） */
  defaultParentId?: string | null
}>(), { host: null, defaultParentId: null })

const emit = defineEmits<{
  (e: 'saved', host: HostItem): void
  (e: 'save-and-connect', host: HostItem): void
}>()

const hostStore = useHostStore()
const prefs = useSettingsStore()

interface SessionFormValue {
  parentId: string | null
  name: string
  host: string
  port: number
  authType: 'password' | 'key'
  username: string
  password: string
  keyPath: string
  keyContent: string
  keyName: string
  passphrase: string
}

function defaultSessionForm(): SessionFormValue {
  return {
    parentId: props.defaultParentId || null,
    name: '',
    host: '',
    port: prefs.settings.defaultPort || 22,
    authType: (prefs.settings.defaultAuth === 'key' ? 'key' : 'password') as 'password' | 'key',
    username: prefs.settings.defaultUser || 'root',
    password: '',
    keyPath: '',
    keyContent: '',
    keyName: '',
    passphrase: ''
  }
}

/** 编辑态：把 HostItem 翻成表单值（addr 拆 host:port，config 取认证信息） */
function formFromHost(h: HostItem): SessionFormValue {
  const cfg = h.config
  const parts = (h.addr || '').split('@')
  const hostPart = parts.length >= 2 ? parts[1] : h.addr
  const portPart = hostPart ? hostPart.split(':')[1] : undefined
  return {
    parentId: h.parentId ?? null,
    name: h.name,
    host: hostPart ? hostPart.split(':')[0] : '',
    port: portPart ? parseInt(portPart) || 22 : 22,
    authType: (cfg?.authType as 'password' | 'key') || 'password',
    username: cfg?.username || 'root',
    password: cfg?.password || '',
    keyPath: cfg?.keyPath || '',
    keyContent: cfg?.keyContent || '',
    keyName: cfg?.keyName || '',
    passphrase: cfg?.passphrase || ''
  }
}

const sessionForm = ref<SessionFormValue>(defaultSessionForm())
const passwordVisible = ref(false)
const passphraseVisible = ref(false)
const testingConnection = ref(false)

/** 每次打开（挂载）按 props 重置表单：编辑回填 host，新建用默认值（含默认上级） */
function resetForm() {
  sessionForm.value = props.mode === 'edit' && props.host ? formFromHost(props.host) : defaultSessionForm()
  passwordVisible.value = false
  passphraseVisible.value = false
}
onMounted(resetForm)
watch(() => props.host, () => { if (props.mode === 'edit') resetForm() }, { deep: true })

// 切换认证方式时收起密码明文，避免跨类型残留
watch(() => sessionForm.value.authType, () => {
  passwordVisible.value = false
  passphraseVisible.value = false
})

// ===== 文件夹选项（选择上级） =====
const folderOptions = (() => {
  const result: HostItem[] = []
  const collect = (items: (HostItem & { children?: HostItem[] })[], depth: number) => {
    items.forEach(item => {
      if (item.type === 'folder') {
        result.push(item)
        if (item.children) collect(item.children, depth + 1)
      }
    })
  }
  collect(hostStore.treeData as any, 0)
  return result
})()

function getFolderLabel(folder: HostItem): string {
  const parentIds: string[] = []
  let current: HostItem | undefined = folder
  while (current?.parentId) {
    const parent = hostStore.hosts.find(h => h.id === current!.parentId)
    if (parent) {
      parentIds.unshift(parent.name)
      current = parent
    } else break
  }
  return parentIds.length > 0 ? parentIds.join(' / ') + ' / ' + folder.name : folder.name
}

/** 切换密码/私钥口令可见性：从隐藏到显示时必须验证「查看会话密码」安全密码 */
async function togglePasswordVisible(field: 'password' | 'passphrase'): Promise<void> {
  const visibleRef = field === 'password' ? passwordVisible : passphraseVisible
  if (!visibleRef.value) {
    if (!(await ensureUnlocked('view_password'))) return
  }
  visibleRef.value = !visibleRef.value
}

/** Rust read_key_file 返回的私钥文件内容 */
interface KeyFileData { content: string; name: string }

async function selectKeyFile() {
  // 移动端：Rust 侧 rfd 原生对话框不可用（pick_key_file 在 #[cfg(mobile)] 直接返回 None），
  // 改走 WebView 的文件选择器，直接拿到文件内容 —— 表单本就支持只存内容（keyContent），
  // 连接时由 materialize_key 落盘，不依赖本地路径。
  if (isMobileWebview()) {
    const [picked] = await pickLocalFileText()
    if (!picked) return
    sessionForm.value.keyPath = ''
    sessionForm.value.keyContent = picked.text
    sessionForm.value.keyName = picked.name
    return
  }
  try {
    const path = await invoke<string | null>('pick_key_file')
    if (!path) return
    const data = await invoke<KeyFileData>('read_key_file', { path })
    sessionForm.value.keyPath = path
    sessionForm.value.keyContent = data.content
    sessionForm.value.keyName = data.name || path.split(/[\\/]/).pop() || ''
  } catch (e: any) {
    handleApiError(e, 'components/SessionForm.selectKeyFile', { prefix: '读取密钥文件失败' })
  }
}

function clearKeyFile() {
  sessionForm.value.keyContent = ''
  sessionForm.value.keyName = ''
  sessionForm.value.passphrase = ''
}

/** 表单校验：返回第一条错误文案（空串 = 通过）。保存与「测试连接」共用，避免两处规则漂移 */
function validateSessionForm(opts?: { skipName?: boolean }): string {
  const f = sessionForm.value
  if (!opts?.skipName && !f.name.trim()) return '请输入会话名称'
  if (!f.host.trim()) return '请输入主机地址'
  if (!f.username.trim()) return '请输入用户名'
  if (f.authType === 'password' && !f.password) return '请输入密码'
  if (f.authType === 'key' && !f.keyContent && !f.keyPath) return '请选择密钥文件'
  return ''
}

/** 某父级下的子项（可排除指定节点），按当前排序 —— 用于编辑时移动节点的插入位置 */
function childrenOf(parentId: string | null, excludeId?: string): HostItem[] {
  return hostStore.hosts
    .filter(h => h.namespaceId === hostStore.currentNamespaceId && h.parentId === parentId && h.id !== excludeId)
    .sort((a, b) => a.sortOrder - b.sortOrder)
}

/** 把后端连接失败原因翻成「用户能直接照着排查的具体提示」 */
function describeConnectError(raw: string): string {
  const s = String(raw || '')
  if (/用户名或密钥不匹配/.test(s)) return '认证失败：私钥不被服务器接受（请检查私钥、私钥口令与用户名权限）'
  if (/密钥认证失败/.test(s)) return '认证失败：私钥被服务器拒绝（请确认公钥已加入服务器的 authorized_keys）'
  if (/加载密钥失败/.test(s)) return '私钥读取失败：文件不是有效私钥，或私钥口令不正确'
  if (/协商 RSA 哈希算法失败/.test(s)) return '私钥类型不被服务器支持（RSA 签名算法协商失败），请改用 ed25519 私钥'
  if (/认证失败|用户名或密码错误|permission denied|authentication failed|invalid password|incorrect password/i.test(s)) {
    return '认证失败：用户名或密码错误'
  }
  if (/连接超时|timed?[\s-]*out|timeout/i.test(s)) return '连接超时：主机不可达或端口/防火墙拦截'
  if (/拒绝连接|connection refused|os error 10061/i.test(s)) return '连接被拒绝：目标端口没有服务在监听'
  if (/找不到主机|未知的名称或服务|name or service not known|could not resolve|os error 11001/i.test(s)) return '无法解析主机名：请检查主机地址 / DNS'
  if (/network is unreachable|os error 10051|os error 10065/i.test(s)) return '网络不可达：请检查网络连接'
  return s.trim() || '连接失败'
}

/** 测试连接：走真实 connect_ssh（与打开会话同一条链路，结果才可信），成功后立即回收探测会话 */
async function testConnection(): Promise<void> {
  if (testingConnection.value) return
  const invalid = validateSessionForm({ skipName: true })
  if (invalid) { ElMessage.warning(invalid); return }
  const f = sessionForm.value
  const host = f.host.trim()
  const username = f.username.trim()
  const port = f.port || 22
  testingConnection.value = true
  let sessionId = ''
  try {
    const proxy = await resolveSshProxy(prefs.settings)
    let keyPath = f.keyPath
    if (f.authType === 'key' && f.keyContent) {
      keyPath = await invoke<string>('materialize_key', { content: f.keyContent })
    }
    const timeoutSecs = Math.max(5, Math.min(prefs.settings.sshConnectTimeout || 15, 15))
    const session = await invoke<{ id: string }>('connect_ssh', {
      dto: {
        host,
        port,
        username,
        auth_type: f.authType === 'key' ? 'private_key' : 'password',
        password: f.authType === 'password' ? f.password : null,
        private_key_path: f.authType === 'key' ? keyPath : null,
        passphrase: f.authType === 'key' ? (f.passphrase || null) : null,
        cols: 80,
        rows: 24,
        connect_timeout_secs: timeoutSecs,
        keep_alive_secs: null,
        proxy_addr: proxy.addr,
        proxy_kind: proxy.kind
      }
    }, { timeoutMs: (timeoutSecs + 5) * 1000 })
    sessionId = session.id
    ElMessage.success(`连接成功：${username}@${host}:${port}`)
  } catch (e: any) {
    ElMessage.error(describeConnectError(e?.message || String(e)))
  } finally {
    if (sessionId) invoke('close_session', { sessionId }).catch(() => {})
    testingConnection.value = false
  }
}

/** 落库（新建 / 更新），返回保存后的 HostItem；校验不过返回 null */
function persist(): HostItem | null {
  const f = sessionForm.value
  const invalid = validateSessionForm()
  if (invalid) { ElMessage.warning(invalid); return null }

  const addr = `${f.username}@${f.host}:${f.port}`
  const config = {
    username: f.username, port: f.port, authType: f.authType,
    password: f.password, keyPath: f.keyPath, keyContent: f.keyContent,
    keyName: f.keyName, passphrase: f.passphrase
  }

  if (props.mode === 'edit' && props.host) {
    const id = props.host.id
    hostStore.updateHost(id, { name: f.name, addr, config })
    // 上级文件夹变更：选择「无」即 parentId=null，该会话直接放在顶级目录
    const nextParentId = f.parentId ?? null
    if (nextParentId !== (props.host.parentId ?? null)) {
      hostStore.moveNode(id, nextParentId, childrenOf(nextParentId, id).length)
    }
    const updated = hostStore.hosts.find(h => h.id === id)
    ElMessage.success('会话已更新')
    return updated || props.host
  }

  const created = hostStore.addHost({
    name: f.name,
    addr,
    namespaceId: hostStore.currentNamespaceId,
    parentId: f.parentId || null,
    type: 'host',
    config
  })
  ElMessage.success('会话已创建')
  return created
}

/** 保存（桌面「确定」/ 移动端「保存」）：落库后告知外层，由外层决定关闭弹窗或返回列表 */
function save(): void {
  const host = persist()
  if (host) emit('saved', host)
}

/** 保存并连接（移动端专属）：落库后告知外层，外层负责打开会话并切到终端页 */
function saveAndConnect(): void {
  const host = persist()
  if (host) emit('save-and-connect', host)
}

defineExpose({ testConnection, save, saveAndConnect })
</script>

<style scoped>
/* 上级文件夹下拉与浮动标签文本框高度一致（桌面 36px）。
   移动端整页由 mobile.css 第 15 节统一覆盖为更大的圆角控件 */
.tall-field :deep(.el-select__wrapper) {
  min-height: 36px;
}
</style>
