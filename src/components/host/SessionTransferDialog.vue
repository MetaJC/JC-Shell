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
    v-model="visible"
    :title="isExport ? '导出会话' : '导入会话'"
    :width="isExport ? '520px' : '780px'"
    append-to-body
    :close-on-click-modal="false"
    class="jc-dialog"
  >
    <!-- 导入方式切换：移动端没有「本机已安装客户端」可扫描，只保留「从文件导入」，切换器整体隐藏 -->
    <el-radio-group v-if="!isExport && !isMobile" v-model="importTab" size="small" class="mb-3">
      <el-radio-button value="scan">一键导入</el-radio-button>
      <el-radio-button value="file">从文件导入</el-radio-button>
    </el-radio-group>

    <!-- ===== 一键导入：自动扫描已安装客户端（桌面端专有） ===== -->
    <div v-if="!isExport && !isMobile && importTab === 'scan'">
      <div class="flex items-center justify-between mb-2">
        <span class="text-xs text-[var(--jc-text-soft)]">自动扫描本机已安装的 SSH 客户端</span>
        <el-button size="small" :loading="scanning" @click="runScan">重新扫描</el-button>
      </div>

      <el-table
        v-if="clients.length > 0"
        :data="clients"
        size="small"
        max-height="200"
        @selection-change="onClientSel"
      >
        <el-table-column type="selection" width="38" :selectable="selectableClient" />
        <el-table-column prop="name" label="客户端" width="100" />
        <el-table-column prop="path" label="路径" min-width="200" show-overflow-tooltip />
        <el-table-column prop="note" label="说明" min-width="180" show-overflow-tooltip />
      </el-table>
      <div v-else-if="!scanning" class="text-xs text-[var(--jc-text-muted)] py-6 text-center">
        未检测到可导入的客户端（Xshell / MobaXterm / Electerm / Termius / FinalShell）
      </div>

      <el-button
        class="mt-2"
        size="small"
        type="primary"
        :disabled="selectedClients.length === 0"
        :loading="parsing"
        @click="parseClients"
      >
        解析选中（{{ selectedClients.length }}）
      </el-button>
    </div>

    <!-- ===== 从文件导入（移动端唯一的导入方式，故不与 scan 分支互斥判断） ===== -->
    <div v-else-if="!isExport">
      <div class="flex gap-2">
        <el-button size="small" type="primary" plain :loading="parsing" @click="chooseFile">选择文件…</el-button>
        <!-- 移动端 WebView 选不了目录（Android SAF 目录树需原生侧实现），故隐藏 -->
        <el-button v-if="!isMobile" size="small" plain :loading="parsing" @click="chooseFolder">选择文件夹…</el-button>
      </div>
      <p class="text-xs text-[var(--jc-text-muted)] mt-2 leading-relaxed">
        支持：Xshell 会话文件（.xsh）、MobaXterm 配置（MobaXterm.ini）、Electerm 数据（.json）、
        Termius / FinalShell 等客户端导出的 JSON 备份，以及 jc-shell 自己的导出文件。
      </p>
    </div>

    <!-- ===== 导出 ===== -->
    <div v-else>
      <p class="text-xs text-[var(--jc-text-soft)] mb-2">
        导出「{{ currentNsName }}」命名空间下的全部会话（含分组层级）。
      </p>
      <el-checkbox v-model="includePasswords" size="small">包含密码（加密后写入文件）</el-checkbox>
      <p class="text-xs text-[var(--jc-text-muted)] mt-1">不勾选时导出文件不含密码，便于安全地分享/备份。</p>
    </div>

    <!-- ===== 预览（一键导入 / 从文件导入 共用） ===== -->
    <template v-if="!isExport && sessions.length > 0">
      <el-divider content-position="left" class="my-3">
        预览（共 {{ sessions.length }} 条，已选 {{ selectedSessions.length }} 条）
      </el-divider>

      <el-alert
        v-if="hasExternal"
        type="warning"
        :closable="false"
        show-icon
        class="mb-2"
        title="外部客户端的密码均为密文，不会迁移；导入后请手动补全密码"
      />

      <el-table :data="sessions" size="small" max-height="260" @selection-change="onSessionSel">
        <el-table-column type="selection" width="38" />
        <el-table-column prop="name" label="名称" min-width="130" show-overflow-tooltip />
        <el-table-column label="地址" min-width="150">
          <template #default="{ row }">{{ row.host }}:{{ row.port }}</template>
        </el-table-column>
        <el-table-column prop="username" label="用户" width="90" />
        <el-table-column label="认证" width="70">
          <template #default="{ row }">{{ row.authType === 'key' ? '密钥' : '密码' }}</template>
        </el-table-column>
        <el-table-column label="目录" min-width="130" show-overflow-tooltip>
          <template #default="{ row }">{{ row.folderPath.join(' / ') || '—' }}</template>
        </el-table-column>
      </el-table>

      <div class="flex items-center gap-3 mt-3 flex-wrap">
        <span class="text-xs text-[var(--jc-text-soft)]">导入到</span>
        <!-- teleported=false：下拉浮层渲染在弹窗内部，避免被 el-dialog 的层级盖住 -->
        <el-select v-model="targetNs" size="small" style="width: 180px" :teleported="false">
          <el-option v-for="ns in hostStore.namespaces" :key="ns.id" :label="ns.name" :value="ns.id" />
        </el-select>
        <span class="text-xs text-[var(--jc-text-soft)]">重名会话</span>
        <el-radio-group v-model="conflict" size="small">
          <el-radio-button value="skip">跳过</el-radio-button>
          <el-radio-button value="overwrite">覆盖</el-radio-button>
          <el-radio-button value="duplicate">保留两者</el-radio-button>
        </el-radio-group>
      </div>
    </template>

    <template #footer>
      <el-button size="small" @click="visible = false">关闭</el-button>
      <el-button
        v-if="isExport"
        size="small"
        type="primary"
        :loading="exporting"
        @click="doExport"
      >
        导出到文件…
      </el-button>
      <el-button
        v-else
        size="small"
        type="primary"
        :disabled="selectedSessions.length === 0"
        :loading="importing"
        @click="doImport"
      >
        导入（{{ selectedSessions.length }}）
      </el-button>
    </template>
  </el-dialog>
</template>

<script setup lang="ts">
/**
 * 会话导入 / 导出弹窗
 * 作者：JC
 *
 * 工具栏上是两个独立入口，由 view 决定本弹窗显示哪一侧：
 *   1. 导入会话（view=import）：含「一键导入（自动扫描已安装客户端）」与
 *      「从文件导入（手动选文件/目录）」两个子页；
 *   2. 导出会话（view=export）：导出当前命名空间整棵树。
 * 两种导入入口共用「预览 → 选目标命名空间 → 冲突策略 → 落库」这套流程。
 */
import { ref, computed, watch } from 'vue'
import { ElMessage } from 'element-plus'
import { useHostStore } from '@/stores/host'
import { handleApiError } from '@/utils/errorHandler'
import { ensureUnlocked, type ScopeKey } from '@/utils/securityPassword'
import type { DetectedClient, ExportSourceNode, NormSession, ParseResult, ConflictStrategy } from '@/utils/sessionImport'
import {
  scanInstalledClients,
  collectFromClient,
  collectFromFile,
  collectFromFileText,
  collectFromFolder,
  pickImportFile,
  pickImportFolder,
  importSessions,
  exportSessionsToFile,
} from '@/utils/sessionImport'
import { isMobileWebview } from '@/utils/platform'
import { pickLocalFileText } from '@/utils/localFilePick'

const props = withDefaults(
  defineProps<{
    modelValue: boolean
    /** 入口视图：'import' = 只做导入（含「一键导入 / 从文件导入」两个子页），'export' = 只做导出 */
    view?: 'import' | 'export'
  }>(),
  { view: 'import' },
)
const emit = defineEmits<{ (e: 'update:modelValue', v: boolean): void }>()

const visible = computed({
  get: () => props.modelValue,
  set: (v: boolean) => emit('update:modelValue', v),
})

const hostStore = useHostStore()

/** 是否移动端：选文件走 WebView 选择器（原生 rfd 对话框移动端不可用） */
const isMobile = isMobileWebview()

/** 导出视图下不做扫描/导入相关的事 */
const isExport = computed(() => props.view === 'export')
/** 导入的两个子页：一键扫描 / 从文件选择。
 *  移动端没有「本机已安装客户端」可扫描，默认直接落在「从文件导入」 */
const importTab = ref<'scan' | 'file'>(isMobile ? 'file' : 'scan')
const scanning = ref(false)
const parsing = ref(false)
const importing = ref(false)
const exporting = ref(false)

const clients = ref<DetectedClient[]>([])
const selectedClients = ref<DetectedClient[]>([])
const sessions = ref<NormSession[]>([])
const selectedSessions = ref<NormSession[]>([])

const targetNs = ref(hostStore.currentNamespaceId)
const conflict = ref<ConflictStrategy>('skip')
const includePasswords = ref(false)

const currentNsName = computed(
  () => hostStore.namespaces.find((n) => n.id === targetNs.value)?.name ?? '当前',
)
/** 是否含外部客户端来源（用于提示密码不会迁移） */
const hasExternal = computed(() => sessions.value.some((s) => s.source !== 'jc'))

function onClientSel(rows: DetectedClient[]) {
  selectedClients.value = rows
}
/** 加密数据库类（如 Termius）无法直接解析，禁止勾选，避免点了没结果 */
function selectableClient(row: DetectedClient): boolean {
  return row.kind !== 'db'
}
function onSessionSel(rows: NormSession[]) {
  selectedSessions.value = rows
}

/** 合并解析结果：会话累加，警告提示给用户 */
function mergeResult(r: ParseResult) {
  sessions.value = [...sessions.value, ...r.sessions]
  if (r.warnings.length > 0) {
    ElMessage.warning(r.warnings.slice(0, 3).join('；'))
  }
  if (r.sessions.length === 0 && r.warnings.length === 0) {
    ElMessage.info('未识别到会话')
  }
}

async function runScan() {
  scanning.value = true
  try {
    clients.value = await scanInstalledClients()
  } catch (e) {
    handleApiError(e, '扫描已安装客户端')
  } finally {
    scanning.value = false
  }
}

async function parseClients() {
  parsing.value = true
  sessions.value = []
  try {
    for (const c of selectedClients.value) {
      try {
        mergeResult(await collectFromClient(c))
      } catch (e) {
        handleApiError(e, `解析 ${c.name}`)
      }
    }
  } finally {
    parsing.value = false
  }
}

async function chooseFile() {
  // 移动端：rfd 原生对话框不可用（pick_import_file 在 #[cfg(mobile)] 返回 None），
  // 改走 WebView 文件选择器：内容已在 JS 侧，直接交给解析器，无需本地路径
  if (isMobile) {
    const [picked] = await pickLocalFileText({ accept: '.xsh,.json,.ini,.txt,.conf' })
    if (!picked) return
    parsing.value = true
    sessions.value = []
    try {
      mergeResult(await collectFromFileText(picked.text, picked.name))
    } catch (e) {
      handleApiError(e, '读取会话文件')
    } finally {
      parsing.value = false
    }
    return
  }
  const path = await pickImportFile('选择会话文件', ['xsh', 'json', 'ini', 'txt']).catch(() => null)
  if (!path) return
  parsing.value = true
  sessions.value = []
  try {
    mergeResult(await collectFromFile(path))
  } catch (e) {
    handleApiError(e, '读取会话文件')
  } finally {
    parsing.value = false
  }
}

async function chooseFolder() {
  const dir = await pickImportFolder('选择会话目录').catch(() => null)
  if (!dir) return
  parsing.value = true
  sessions.value = []
  try {
    mergeResult(await collectFromFolder(dir))
  } catch (e) {
    handleApiError(e, '读取会话目录')
  } finally {
    parsing.value = false
  }
}

async function doImport() {
  if (selectedSessions.value.length === 0) return
  importing.value = true
  try {
    const sum = await importSessions(selectedSessions.value, {
      namespaceId: targetNs.value,
      conflict: conflict.value,
    })
    ElMessage.success(
      `导入完成：新增 ${sum.created} 个会话，更新 ${sum.updated} 个，跳过 ${sum.skipped} 个，新建 ${sum.folders} 个分组`,
    )
    sessions.value = []
    selectedSessions.value = []
    visible.value = false
  } catch (e) {
    handleApiError(e, '导入会话')
  } finally {
    importing.value = false
  }
}

async function doExport() {
  // 安全验证（先验证再选保存路径）：
  //  - 勾选「包含密码」→ 无论其它保护范围如何，只要开启了安全密码就必须验证（export_password 始终开启）
  //  - 未勾选 → 取决于「导出会话数据」保护范围是否开启
  const scope: ScopeKey = includePasswords.value ? 'export_password' : 'export_data'
  if (!(await ensureUnlocked(scope))) return
  exporting.value = true
  try {
    const nodes = hostStore.treeData as unknown as ExportSourceNode[]
    const ok = await exportSessionsToFile({
      nodes,
      namespaceName: currentNsName.value,
      includePasswords: includePasswords.value,
    })
    if (ok) ElMessage.success('会话已导出')
  } catch (e) {
    handleApiError(e, '导出会话')
  } finally {
    exporting.value = false
  }
}

// 打开时重置状态并自动扫描；切到「一键导入」且还没扫过时也扫一次
function reset() {
  // 移动端只有「从文件导入」一种方式，不能落回 scan，否则会渲染出空的扫描子页
  importTab.value = isMobile ? 'file' : 'scan'
  sessions.value = []
  selectedSessions.value = []
  selectedClients.value = []
  targetNs.value = hostStore.currentNamespaceId
}
watch(visible, (v) => {
  if (v) {
    reset()
    // 导出视图不需要扫描已安装客户端；移动端没有「一键导入」入口，同样不扫描
    if (!isExport.value && !isMobile) void runScan()
  }
})
watch(importTab, (m) => {
  if (m === 'scan' && clients.value.length === 0) void runScan()
})
</script>
