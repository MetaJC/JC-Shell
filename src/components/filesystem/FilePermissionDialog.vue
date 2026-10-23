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
    :model-value="modelValue"
    title="编辑权限"
    width="420px"
    append-to-body
    :close-on-click-modal="false"
    @update:model-value="$emit('update:modelValue', $event)"
    @open="onOpen"
  >
    <div class="text-xs" @keydown="onKeydown">
      <div class="mb-3 text-[var(--jc-text-soft)] truncate" :title="path">{{ path }}</div>

      <!-- rwx 可视化矩阵 -->
      <table class="w-full mb-3 perm-table">
        <thead>
          <tr class="text-[var(--jc-text-muted)]">
            <th class="text-left py-1"></th>
            <th class="py-1">读 (r)</th>
            <th class="py-1">写 (w)</th>
            <th class="py-1">执行 (x)</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="(row, ri) in rows" :key="row.label">
            <td class="py-1 text-[var(--jc-text-soft)]">{{ row.label }}</td>
            <td v-for="(_, bi) in row.bits" :key="bi" class="py-1 text-center">
              <el-checkbox :model-value="row.bits[bi]" @change="(v: boolean) => setBit(ri, bi, v)" />
            </td>
          </tr>
        </tbody>
      </table>

      <!-- 特殊位 -->
      <div class="flex items-center gap-4 mb-3 text-[var(--jc-text-soft)]">
        <el-checkbox v-model="setuid" size="small">setuid</el-checkbox>
        <el-checkbox v-model="setgid" size="small">setgid</el-checkbox>
        <el-checkbox v-model="sticky" size="small">sticky</el-checkbox>
        <el-checkbox v-if="isDir" v-model="recursive" size="small">递归应用到子文件</el-checkbox>
      </div>

      <!-- 八进制 / 符号表示 -->
      <div class="flex items-center gap-3">
        <span class="text-[var(--jc-text-muted)] shrink-0">八进制</span>
        <el-input v-model="octalText" size="small" style="width: 90px" maxlength="4" @blur="applyOctal" @keyup.enter="applyOctal" />
        <span class="text-[var(--jc-text-muted)] shrink-0">符号</span>
        <code class="text-[var(--jc-text)] text-xs">{{ symbolic }}</code>
      </div>
    </div>

    <template #footer>
      <span class="text-[10px] text-[var(--jc-text-muted)] float-left leading-8">Ctrl+S 保存</span>
      <el-button size="small" @click="$emit('update:modelValue', false)">取消</el-button>
      <el-button size="small" type="primary" :loading="saving" @click="save">确定</el-button>
    </template>
  </el-dialog>
</template>

<script setup lang="ts">
/**
 * 文件权限编辑对话框（chmod 可视化）
 * 作者：JC
 * rwx 矩阵 + 特殊位（setuid/setgid/sticky）+ 八进制/符号双向同步；
 * Ctrl+S 或点击"确定"保存（set_file_permissions，SSH 走 chmod，本地 Unix 直接设置）。
 */
import { ref, computed, watch } from 'vue'
import { invoke } from '@/api'
import { ElMessage } from 'element-plus'
import { handleApiError } from '@/utils/errorHandler'

interface Props {
  modelValue: boolean
  sessionId: string
  path: string
  /** ls 风格权限串，如 -rwxr-xr-- */
  permissions: string
  isDir: boolean
}
const props = defineProps<Props>()
const emit = defineEmits<{ (e: 'update:modelValue', v: boolean): void; (e: 'saved'): void }>()

const rows = ref([
  { label: '所有者 (u)', bits: [false, false, false] },
  { label: '用户组 (g)', bits: [false, false, false] },
  { label: '其他人 (o)', bits: [false, false, false] },
])
const setuid = ref(false)
const setgid = ref(false)
const sticky = ref(false)
const recursive = ref(false)
const saving = ref(false)
const octalText = ref('')

/** 解析 ls 权限串（含 s/S/t/T 特殊位） */
function parse(perms: string) {
  const s = perms.length >= 10 ? perms.slice(1) : perms.padEnd(9, '-')
  rows.value[0].bits = [s[0] === 'r', s[1] === 'w', ['x', 's'].includes(s[2])]
  rows.value[1].bits = [s[3] === 'r', s[4] === 'w', ['x', 's'].includes(s[5])]
  rows.value[2].bits = [s[6] === 'r', s[7] === 'w', ['x', 't'].includes(s[8])]
  setuid.value = s[2] === 's' || s[2] === 'S'
  setgid.value = s[5] === 's' || s[5] === 'S'
  sticky.value = s[8] === 't' || s[8] === 'T'
}

const octal = computed(() => {
  const digit = (bits: boolean[]) => (bits[0] ? 4 : 0) + (bits[1] ? 2 : 0) + (bits[2] ? 1 : 0)
  const special = (setuid.value ? 4 : 0) + (setgid.value ? 2 : 0) + (sticky.value ? 1 : 0)
  const base = `${digit(rows.value[0].bits)}${digit(rows.value[1].bits)}${digit(rows.value[2].bits)}`
  return special ? `${special}${base}` : base
})

const symbolic = computed(() => {
  const seg = (bits: boolean[], specialC: string, hasSpecial: boolean) => {
    let x = bits[2] ? 'x' : '-'
    if (hasSpecial) x = bits[2] ? specialC.toLowerCase() : specialC.toUpperCase()
    return (bits[0] ? 'r' : '-') + (bits[1] ? 'w' : '-') + x
  }
  return seg(rows.value[0].bits, 's', setuid.value) + seg(rows.value[1].bits, 's', setgid.value) + seg(rows.value[2].bits, 't', sticky.value)
})

function syncOctalText() { octalText.value = octal.value }
watch([setuid, setgid, sticky], syncOctalText)

function setBit(ri: number, bi: number, v: boolean) {
  rows.value[ri].bits[bi] = v
  syncOctalText()
}

/** 手动输入八进制 → 反向同步矩阵 */
function applyOctal() {
  const t = octalText.value.trim().replace(/^0+(?=\d)/, '')
  if (!/^[0-7]{3,4}$/.test(t)) {
    ElMessage.warning('请输入 3~4 位八进制权限，如 755 或 4755')
    syncOctalText()
    return
  }
  const digs = t.length === 3 ? [0, ...t.split('').map(Number)] : t.split('').map(Number)
  const sp = digs[0]
  setuid.value = !!(sp & 4); setgid.value = !!(sp & 2); sticky.value = !!(sp & 1)
  for (let i = 0; i < 3; i++) {
    const d = digs[i + 1]
    rows.value[i].bits = [!!(d & 4), !!(d & 2), !!(d & 1)]
  }
}

function onOpen() {
  parse(props.permissions || '---------')
  recursive.value = false
  syncOctalText()
}

function onKeydown(e: KeyboardEvent) {
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') {
    e.preventDefault()
    save()
  }
}

async function save() {
  applyOctal()
  saving.value = true
  try {
    await invoke('set_file_permissions', {
      sessionId: props.sessionId,
      path: props.path,
      mode: octal.value,
      recursive: recursive.value
    })
    ElMessage.success(`权限已修改为 ${octal.value}`)
    emit('update:modelValue', false)
    emit('saved')
  } catch (e: any) {
    handleApiError(e, 'components/FilePermissionDialog.save', {
      prefix: '修改权限失败',
      url: `set_file_permissions/${props.path}`,
    })
  } finally {
    saving.value = false
  }
}
</script>

<style scoped>
.perm-table :deep(.el-checkbox) { height: auto; margin-right: 0; }
.perm-table :deep(.el-checkbox__label) { display: none; }
</style>
