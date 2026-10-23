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
  <div class="host-list-panel h-full flex flex-col bg-[var(--jc-bg-soft)] overflow-hidden">
    <!-- 工具栏一行：按钮位置随 dock 自动排布，窄窗口时仅中间图标收纳。
         移动端：图标按钮收进右下角扇形菜单，命名空间按钮改为显示选中值的下拉框 -->
    <PanelHeader panel-id="hostList" :dock="dock" :collapsed="collapsed" @collapse="$emit('collapse')">
      <AdaptiveToolbar v-if="!collapsed && !isMobile" :buttons="toolbarButtons" />
      <!-- 移动端：命名空间下拉 + 三个纯图标按钮（管理命名空间 / 导入 / 导出），左右均匀分布。
           管理入口原来是下拉里的一个选项（藏得深），独立成按钮后一步可点；
           导入/导出原在右下角扇形菜单里，移到头部后扇形菜单只留新建/刷新 -->
      <div v-else-if="!collapsed && isMobile" class="m-ns-row">
        <el-select
          class="m-ns-select"
          :model-value="hostStore.currentNamespaceId"
          @change="onMobileNamespaceChange"
        >
          <el-option v-for="ns in hostStore.namespaces" :key="ns.id" :label="ns.name" :value="ns.id" />
        </el-select>
        <div class="m-ns-actions">
          <button class="m-ns-icon" title="管理命名空间" aria-label="管理命名空间" @click="openNamespaceManager">
            <el-icon :size="18"><Setting /></el-icon>
          </button>
          <button class="m-ns-icon" title="导入会话" aria-label="导入会话" @click="handleBlankAction('import')">
            <el-icon :size="18"><Download /></el-icon>
          </button>
          <button class="m-ns-icon" title="导出会话" aria-label="导出会话" @click="handleBlankAction('export')">
            <el-icon :size="18"><Upload /></el-icon>
          </button>
        </div>
      </div>
    </PanelHeader>

    <!-- 会话列表树（展开态） -->
    <Transition name="jc-swap">
      <div
        v-show="!collapsed"
        ref="listRef"
        class="flex-1 overflow-y-auto py-1 relative"
        @contextmenu.prevent="onListContextMenu"
        @click.self="clearSelection"
      >
      <template v-if="hostStore.treeData.length > 0">
        <HostNode
          v-for="node in hostStore.treeData"
          :key="node.id"
          :node="node"
          :level="0"
          :selected-ids="selectedIds"
          @open="onOpenHost"
          @edit="onEditNode"
          @select="onSelectNode"
          @move="onMoveNode"
          @context-menu="onNodeContextMenu"
          @new-session="onNewSessionInFolder"
        />
      </template>
      <div v-else class="text-center text-[var(--jc-text-muted)] text-xs mt-8">
        <el-icon size="28" class="mb-2 opacity-50"><FolderOpened /></el-icon>
        <div>暂无会话</div>
        <div class="mt-1 opacity-60">点击工具栏 <span class="text-blue-400">+</span> 新建会话</div>
      </div>

      <!-- 右键菜单 -->
      <div
        v-if="contextMenu.visible"
        class="fixed z-50 context-menu-menu bg-[var(--jc-bg-hover)] border border-[var(--jc-border)] rounded shadow-xl py-1 min-w-[140px]"
        :style="{ left: contextMenu.x + 'px', top: contextMenu.y + 'px' }"
        @contextmenu.prevent.stop
      >
        <!-- 多选时：批量操作菜单 -->
        <template v-if="selectedCount > 1">
          <div class="menu-item text-blue-400 hover:bg-blue-500/10" @click="onContextAction('batch-connect')">
            <el-icon><Connection /></el-icon>连接 {{ selectedCount }} 个会话
          </div>
          <div class="menu-item" @click="onContextAction('batch-copy-address')">
            <el-icon><Link /></el-icon>复制地址
          </div>
          <div class="menu-item" @click="onContextAction('batch-move')">
            <el-icon><FolderOpened /></el-icon>移动到…
          </div>
          <div class="menu-divider"></div>
          <div class="menu-item text-red-400 hover:bg-red-500/10" @click="onContextAction('batch-delete')">
            <el-icon><Delete /></el-icon>删除 {{ selectedCount }} 项
          </div>
        </template>
        <template v-else-if="contextMenu.node?.type === 'folder'">
          <div class="menu-item" @click="onContextAction('new-session')">
            <el-icon><Plus /></el-icon>新建会话
          </div>
          <div class="menu-item" @click="onContextAction('new-folder')">
            <el-icon><FolderOpened /></el-icon>新建分组
          </div>
          <div class="menu-divider"></div>
          <div class="menu-item" @click="onContextAction('edit-folder')">
            <el-icon><EditPen /></el-icon>重命名
          </div>
          <div class="menu-item text-red-400 hover:bg-red-500/10" @click="onContextAction('delete')">
            <el-icon><Delete /></el-icon>删除
          </div>
        </template>
        <!-- 会话节点 -->
        <template v-else-if="contextMenu.node">
          <div class="menu-item text-blue-400 hover:bg-blue-500/10" @click="onContextAction('connect')">
            <el-icon><Connection /></el-icon>连接
          </div>
          <div class="menu-item" @click="onContextAction('view-password')">
            <el-icon><View /></el-icon>查看密码
          </div>
          <div class="menu-divider"></div>
          <div class="menu-item" @click="onContextAction('duplicate-session')">
            <el-icon><CopyDocument /></el-icon>复制会话
          </div>
          <div class="menu-item" @click="onContextAction('copy-address')">
            <el-icon><Link /></el-icon>复制地址
          </div>
          <div class="menu-divider"></div>
          <div class="menu-item" @click="onContextAction('edit-session')">
            <el-icon><EditPen /></el-icon>编辑
          </div>
          <div class="menu-item" @click="onContextAction('rename')">
            <el-icon><Edit /></el-icon>重命名
          </div>
          <div class="menu-divider"></div>
          <div class="menu-item text-red-400 hover:bg-red-500/10" @click="onContextAction('delete')">
            <el-icon><Delete /></el-icon>删除
          </div>
        </template>
        <!-- 列表空白处：新建 / 导入导出 等常用操作 -->
        <template v-else>
          <div class="menu-item" @click="onContextAction('new-session')">
            <el-icon><Plus /></el-icon>新建会话
          </div>
          <div class="menu-item" @click="onContextAction('new-folder')">
            <el-icon><FolderOpened /></el-icon>新建分组
          </div>
          <div class="menu-divider"></div>
          <div class="menu-item" @click="onContextAction('import')">
            <el-icon><Download /></el-icon>导入会话
          </div>
          <div class="menu-item" @click="onContextAction('export')">
            <el-icon><Upload /></el-icon>导出会话
          </div>
          <div class="menu-divider"></div>
          <div class="menu-item" @click="onContextAction('namespace-manager')">
            <el-icon><Collection /></el-icon>命名空间管理
          </div>
          <div class="menu-item" @click="onContextAction('refresh')">
            <el-icon><Refresh /></el-icon>刷新
          </div>
        </template>
        </div>

      </div>
    </Transition>

    <!-- 折叠态：一级目录图标列/行（文件夹 hover/点击悬浮二级菜单，会话双击连接） -->
    <Transition name="jc-swap">
      <div
        v-if="collapsed"
      class="collapsed-strip flex-1 min-h-0 min-w-0 p-1 flex gap-1"
      :class="dock === 'bottom' ? 'flex-row overflow-x-auto items-center' : 'flex-col overflow-y-auto overflow-x-hidden items-center'"
    >
      <div
        v-for="node in (hostStore.treeData as HostTreeNode[])"
        :key="node.id"
        :data-collapsed-id="node.id"
        class="w-7 h-7 shrink-0 flex items-center justify-center rounded cursor-pointer hover:bg-[var(--jc-bg-hover)]"
        :class="{ 'bg-[var(--jc-bg-hover)]': collapsedSelectedId === node.id }"
        @click="onCollapsedIconClick(node, $event)"
        @mouseenter="onCollapsedIconHover(node, $event)"
        @mouseleave="onCollapsedIconLeave"
      >
        <el-icon :size="16" :color="iconColorFor(node)">
          <FolderOpened v-if="node.type === 'folder'" />
          <Monitor v-else />
        </el-icon>
      </div>

      <!-- 折叠态二级菜单弹层（hover/点击文件夹时显示） -->
      <div
        v-if="collapsedSubmenu.visible"
        class="fixed z-50 bg-[var(--jc-bg-hover)] border border-[var(--jc-border)] rounded shadow-xl py-1 min-w-[180px]"
        :style="{ left: collapsedSubmenu.x + 'px', top: collapsedSubmenu.y + 'px', transform: collapsedSubmenuTransform }"
        @mouseenter="cancelCollapsedLeave"
        @mouseleave="hideCollapsedSubmenu"
      >
        <div class="px-3 py-1.5 text-xs text-[var(--jc-text-soft)] border-b border-[var(--jc-border)] mb-1 flex items-center gap-1">
          <el-icon size="11" color="#f0b429"><FolderOpened /></el-icon>
          <span>{{ collapsedSubmenu.folderName }}</span>
        </div>
        <div
          v-for="child in collapsedSubmenu.children"
          :key="child.id"
          class="px-3 py-1.5 text-xs cursor-pointer hover:bg-[var(--jc-border)] flex items-center gap-2"
          :class="{ 'bg-[var(--jc-bg-hover)]': collapsedSelectedId === child.id }"
          @click="onCollapsedChildClick(child)"
          @mouseenter="onCollapsedChildHover(child, $event)"
          @mouseleave="hideCollapsedTip"
        >
          <el-icon :size="11" :color="iconColorFor(child)">
            <FolderOpened v-if="child.type === 'folder'" />
            <Monitor v-else />
          </el-icon>
          <span class="flex-1">{{ child.name }}</span>
          <span v-if="child.type === 'host'" class="text-[10px] text-[var(--jc-text-muted)]">{{ child.addr?.split('@')[1] || '' }}</span>
        </div>
        <div v-if="collapsedSubmenu.children.length === 0" class="px-3 py-2 text-xs text-[var(--jc-text-muted)] text-center">
          空分组
        </div>
      </div>

      <!-- 折叠态会话 tip（与文件夹二级菜单同风格，hover 立即出现，无原生 title 的延迟） -->
      <div
        v-if="collapsedTip.visible"
        class="fixed z-50 pointer-events-none bg-[var(--jc-bg-hover)] border border-[var(--jc-border)] rounded shadow-xl py-1.5 px-2.5 flex items-center gap-2 whitespace-nowrap"
        :style="{ left: collapsedTip.x + 'px', top: collapsedTip.y + 'px', transform: collapsedSubmenuTransform }"
      >
        <el-icon :size="12" color="var(--jc-text-muted)"><Monitor /></el-icon>
        <span class="text-xs text-[var(--jc-text)]">{{ collapsedTip.name }}</span>
        <span v-if="collapsedTip.addr" class="text-[10px] text-[var(--jc-text-muted)]">{{ collapsedTip.addr }}</span>
      </div>
    </div>
    </Transition>
    <!-- 新建 / 编辑会话弹窗（桌面端）：表单内容抽到 SessionForm，移动端改为整页（见 MobileSessionEditPage）。
         destroy-on-close 保证每次打开都重新挂载，SessionForm 按 mode/host/defaultParentId 重新初始化 -->
    <el-dialog
      v-model="showSessionDialog"
      :title="sessionDialogMode === 'create' ? '新建会话' : '编辑会话'"
      width="440"
      append-to-body
      :close-on-click-modal="true"
      destroy-on-close
      class="jc-dialog jc-session-dialog"
    >
      <SessionForm
        ref="sessionFormRef"
        :mode="sessionDialogMode"
        :host="editingNode"
        :default-parent-id="newSessionParentId"
        @saved="onSessionFormSaved"
      />
      <template #footer>
        <!-- 左「测试连接」右「取消/确定」：测试放在最左侧，避免与主操作按钮相邻误点 -->
        <div class="flex items-center justify-between">
          <el-button class="jc-test-conn-btn" @click="sessionFormRef?.testConnection()">
            测试连接
          </el-button>
          <span>
            <el-button @click="showSessionDialog = false">取消</el-button>
            <el-button type="primary" @click="sessionFormRef?.save()">确定</el-button>
          </span>
        </div>
      </template>
    </el-dialog>

    <!-- 新建分组弹窗 -->
    <el-dialog
      v-model="showFolderDialog"
      title="新建分组"
      width="360"
      append-to-body
      :close-on-click-modal="true"
      class="jc-dialog"
    >
      <div class="space-y-3">
        <div>
          <label class="text-xs text-[var(--jc-text-soft)]">分组名称 <span class="text-red-400">*</span></label>
          <el-input v-model="folderForm.name" size="small" placeholder="请输入分组名称" class="mt-1" />
        </div>
        <div>
          <label class="text-xs text-[var(--jc-text-soft)]">上级分组</label>
          <el-select
            v-model="folderForm.parentId"
            placeholder="选择上级分组（可留空）"
            clearable
            filterable
            size="small"
            class="w-full mt-1"
          >
            <el-option
              v-for="folder in folderOptions"
              :key="folder.id"
              :label="getFolderLabel(folder)"
              :value="folder.id"
            />
          </el-select>
        </div>
      </div>
      <template #footer>
        <el-button size="small" @click="showFolderDialog = false">取消</el-button>
        <el-button size="small" type="primary" @click="saveFolder">确定</el-button>
      </template>
    </el-dialog>

    <!-- 命名空间管理弹窗 -->
    <el-dialog
      v-model="showNamespaceDialog"
      title="命名空间管理"
      width="480"
      append-to-body
      class="jc-dialog"
    >
      <div class="space-y-2">
        <div
          v-for="ns in hostStore.namespaces"
          :key="ns.id"
          class="ns-row flex items-center gap-2 p-2 bg-[var(--jc-border)] rounded"
        >
          <el-icon size="14" color="#3b82f6"><Collection /></el-icon>
          <span class="ns-row__name flex-1 text-sm text-[var(--jc-text)]">{{ ns.name }}</span>
          <el-button class="ns-row__act" size="small" text :disabled="hostStore.namespaces.length <= 1" @click="renameNamespace(ns)">
            <el-icon><EditPen /></el-icon>
          </el-button>
          <el-button class="ns-row__act" size="small" text :disabled="hostStore.namespaces.length <= 1" @click="deleteNamespace(ns.id)">
            <el-icon><Delete /></el-icon>
          </el-button>
        </div>
      </div>
      <div class="ns-add mt-3 flex gap-2">
        <el-input v-model="newNamespaceName" placeholder="新命名空间名称" size="small" />
        <el-button type="primary" size="small" @click="addNamespace">添加</el-button>
      </div>
      <template #footer>
        <el-button size="small" @click="showNamespaceDialog = false">关闭</el-button>
      </template>
    </el-dialog>

    <!-- 命名空间下拉弹层（紧贴命名空间按钮下方） -->
    <div
      v-if="nsDropdownVisible && hostStore.namespaces.length > 0"
      class="fixed z-50 bg-[var(--jc-bg-hover)] border border-[var(--jc-border)] rounded shadow-xl py-1 min-w-[160px] max-h-[300px] overflow-y-auto"
      :style="{ left: nsDropdownPos.x + 'px', top: nsDropdownPos.y + 'px' }"
      @contextmenu.prevent
      @click.stop
    >
      <div
        v-for="ns in hostStore.namespaces"
        :key="ns.id"
        class="px-3 py-1.5 text-xs cursor-pointer hover:bg-[var(--jc-border)] flex items-center gap-2"
        :class="{ 'text-blue-400': ns.id === hostStore.currentNamespaceId }"
        @click="selectNamespace(ns.id)"
      >
        <el-icon size="11"><Collection /></el-icon>
        <span class="flex-1">{{ ns.name }}</span>
        <el-icon v-if="ns.id === hostStore.currentNamespaceId" size="11" color="#3b82f6"><Check /></el-icon>
      </div>
      <div class="border-t border-[var(--jc-border)] my-1"></div>
      <div class="px-3 py-1.5 text-xs cursor-pointer hover:bg-[var(--jc-border)] flex items-center gap-2" @click="openNamespaceManager(); closeNsDropdown()">
        <el-icon size="11"><Setting /></el-icon>
        <span>管理命名空间…</span>
      </div>
    </div>
    <div v-if="nsDropdownVisible" class="fixed inset-0 z-40" @click="closeNsDropdown"></div>

    <!-- 批量移动到文件夹 -->
    <el-dialog
      v-model="showMoveDialog"
      title="移动到"
      width="420"
      append-to-body
      class="jc-dialog"
      :close-on-click-modal="true"
    >
      <el-select
        v-model="moveTarget"
        filterable
        :teleported="false"
        style="width: 100%"
        placeholder="选择目标分组"
      >
        <el-option label="顶级目录" value="__root__" />
        <el-option v-for="opt in moveTargetOptions" :key="opt.id" :label="opt.label" :value="opt.id" />
      </el-select>
      <p class="text-xs text-[var(--jc-text-muted)] mt-2">将移动选中的 {{ selectedCount }} 项</p>
      <template #footer>
        <el-button size="small" @click="showMoveDialog = false">取消</el-button>
        <el-button size="small" type="primary" @click="confirmMove">确定</el-button>
      </template>
    </el-dialog>

    <!-- 移动端右下角 FAB：点开扇形菜单（新建会话/分组、刷新；导入/导出已移到头部命名空间行） -->
    <template v-if="isMobile">
      <div v-if="fanOpen" class="m-fan-mask" @click="fanOpen = false"></div>
      <div class="m-fab-wrap">
        <Transition name="m-fan">
          <div v-if="fanOpen" class="m-fan">
            <button
              v-for="(item, i) in fanItems"
              :key="item.key"
              class="m-fan__item"
              :style="fanItemStyle(i)"
              :title="item.label"
              :aria-label="item.label"
              @click="onFanAction(item)"
            >
              <el-icon :size="22"><component :is="item.icon" /></el-icon>
            </button>
          </div>
        </Transition>
        <button class="m-fab" :class="{ 'is-open': fanOpen }" @click="toggleFan">
          <el-icon :size="22"><Close v-if="fanOpen" /><Plus v-else /></el-icon>
        </button>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
/**
 * 会话列表面板（支持命名空间 + 文件夹树 + 右键菜单 + 拖拽排序）
 * 作者：JC
 */
import { ref, computed, watch, onMounted, onBeforeUnmount, markRaw } from 'vue'
import IconTerminal from '@/components/common/IconTerminal.vue'
import {
  Plus, FolderOpened, Refresh, EditPen, Delete, Connection, Collection, Edit, Check, Setting, Monitor,
  CopyDocument, Link, View, Upload, Download, Close
} from '@element-plus/icons-vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { handleApiError } from '@/utils/errorHandler'
import { copyText } from '@/utils/clipboard'
import { invoke } from '@/api'
import { ensureUnlocked } from '@/utils/securityPassword'
import { openImportSessions, openExportSessions } from '@/utils/sessionTransferDialog'
import AdaptiveToolbar from '@/components/common/AdaptiveToolbar.vue'
import type { ToolbarButton } from '@/components/common/AdaptiveToolbar.vue'
import PanelHeader from '@/components/common/PanelHeader.vue'
import HostNode from './HostNode.vue'
import { useHostStore, type HostItem } from '@/stores/host'
import { useDockStore } from '@/stores/dock'
import type { Dock } from '@/stores/layout'
import type { DropPosition } from '@/composables/useHostDrag'
import { isMobileWebview } from '@/utils/platform'
import SessionForm from '@/components/host/SessionForm.vue'

/** 树节点（treeData 运行时带 children） */
type HostTreeNode = HostItem & { children?: HostTreeNode[] }

const props = withDefaults(defineProps<{
  /** 所在 dock 方向（决定按钮排布与折叠态排布） */
  dock?: Dock
  /** 是否折叠（折叠态只显示一级条目图标） */
  collapsed?: boolean
}>(), { dock: 'left', collapsed: false })

const hostStore = useHostStore()
const dockStore = useDockStore()
const emit = defineEmits<{ (e: 'open', host: HostItem): void; (e: 'collapse'): void }>()

/** 移动端 WebView：工具栏收进扇形菜单、命名空间改为下拉、右键改长按（见 HostNode） */
const isMobile = isMobileWebview()

const listRef = ref<HTMLElement | null>(null)

// ===== 状态 =====
const showSessionDialog = ref(false)
const showFolderDialog = ref(false)
const showNamespaceDialog = ref(false)
// 导入 / 导出弹窗由 MainWindow 统一挂载，这里通过单例状态打开（顶部菜单「工具」也走同一入口）
const sessionDialogMode = ref<'create' | 'edit'>('create')
const editingNode = ref<HostItem | null>(null)
const newNamespaceName = ref('')
/** 「测试连接」按钮 loading：表单内部（SessionForm）处理，这里不再需要；保留位置说明已迁移 */

// ===== 新建 / 编辑会话表单 =====
// 表单状态与全部逻辑已抽到 SessionForm.vue（桌面弹窗与移动端整页共用）。
// 这里仅保留弹窗外壳所需引用：表单实例、新建时的默认上级、保存回调。
const sessionFormRef = ref<InstanceType<typeof SessionForm> | null>(null)
const newSessionParentId = ref<string | null>(null)
function onSessionFormSaved() {
  showSessionDialog.value = false
}

const folderForm = ref({ name: '', parentId: null as string | null })

// ===== 多选（Ctrl/Shift 批量选择，行为对齐文件管理器） =====
const selectedIds = ref<string[]>([])
/** Shift 区间选择的锚点（上一次普通/Ctrl 点击的节点） */
const selectionAnchor = ref<string | null>(null)
const selectedCount = computed(() => selectedIds.value.length)
const showMoveDialog = ref(false)
const moveTarget = ref<string>('__root__')
// 切换命名空间后原选中项已不在当前视图，清空多选
watch(() => hostStore.currentNamespaceId, () => clearSelection())

// ===== 右键菜单 =====
const contextMenu = ref({ visible: false, x: 0, y: 0, node: null as HostItem | null })

function openContextMenu(node: HostItem | null, e: MouseEvent) {
  // 移动端菜单被整体放大（字号与行距见 mobile.css 第 20 节），
  // 夹取用的菜单尺寸与边缘留白同步调整；桌面端保持原有夹取逻辑不变
  const x = isMobile
    ? Math.max(8, Math.min(e.clientX, window.innerWidth - 200))
    : Math.min(e.clientX, window.innerWidth - 160)
  const y = isMobile
    ? Math.max(8, Math.min(e.clientY, window.innerHeight - 380))
    : Math.min(e.clientY, window.innerHeight - 200)
  contextMenu.value = {
    visible: true,
    x: x + 4,
    y: y + 4,
    node
  }
}

function closeContextMenu() {
  contextMenu.value.visible = false
  contextMenu.value.node = null
}

/** 列表空白处右键：清空多选并弹出「常用操作」菜单（节点上的 contextmenu 已 .stop，不会冒泡到这里） */
function onListContextMenu(e: MouseEvent) {
  clearSelection()
  openContextMenu(null, e)
}

function onContextAction(action: string) {
  const node = contextMenu.value.node
  closeContextMenu()
  // 无命中节点 = 空白处菜单：执行面板级操作
  if (!node) {
    handleBlankAction(action)
    return
  }
  if (action.startsWith('batch-')) {
    void handleBatchAction(action)
    return
  }
  void handleContextAction(action, node)
}

/** 列表空白处右键菜单动作：新建会话/文件夹、导入导出、命名空间管理、刷新 */
function handleBlankAction(action: string): void {
  switch (action) {
    case 'new-session': openNewSession(); break
    case 'new-folder': openNewFolder(); break
    case 'import': openImportSessions(); break
    case 'export': openExportSessions(); break
    case 'namespace-manager': openNamespaceManager(); break
    case 'refresh': ElMessage.success('刷新完成'); break
  }
}

/**
 * 右键菜单动作处理。
 * 受保护操作（修改/删除/复制/重命名会话）在开启「修改删除会话」保护范围时需先验证安全密码；
 * 「查看密码」在开启「查看会话密码」保护范围时需验证。
 */
async function handleContextAction(action: string, node: HostItem) {
  // 修改/删除/复制/重命名会话：受「修改删除会话」保护范围约束
  const protectedActions = ['edit-session', 'rename', 'duplicate-session', 'delete', 'edit-folder']
  if (protectedActions.includes(action)) {
    if (!(await ensureUnlocked('edit_delete'))) return
  }

  switch (action) {
    case 'connect':
      if (node.type === 'host') onOpenHost(node)
      break
    case 'view-password':
      await viewPassword(node)
      break
    case 'edit-session':
      // 移动端改为整页（见 MobileSessionEditPage）；桌面仍用弹窗
      if (isMobile) { dockStore.pushMobileSubPage({ type: 'session-edit', hostId: node.id }); break }
      sessionDialogMode.value = 'edit'
      editingNode.value = node
      showSessionDialog.value = true
      break
    case 'rename':
      ElMessageBox.prompt('输入新名称', '重命名', { inputValue: node.name }).then(({ value }) => {
        hostStore.updateHost(node.id, { name: value })
        ElMessage.success('已重命名')
      }).catch(() => {})
      break
    case 'duplicate-session':
      duplicateSession(node)
      break
    case 'copy-address':
      copySessionAddress(node).catch(() => {})
      break
    case 'edit-folder':
      ElMessageBox.prompt('输入新名称', '重命名分组', { inputValue: node.name }).then(({ value }) => {
        hostStore.updateHost(node.id, { name: value })
        ElMessage.success('已重命名')
      }).catch(() => {})
      break
    case 'new-session':
      // 移动端改为整页（默认上级=当前文件夹）；桌面仍用弹窗
      if (isMobile) { dockStore.pushMobileSubPage({ type: 'session-edit', defaultParentId: node.id }); break }
      sessionDialogMode.value = 'create'
      editingNode.value = null
      newSessionParentId.value = node.id
      showSessionDialog.value = true
      break
    case 'new-folder':
      // 层级上限：第 3 层分组内不能再建分组（只允许放会话）
      if (!hostStore.canContainFolder(node.id)) { ElMessage.warning('目录层级最多三层'); break }
      folderForm.value = { name: '', parentId: node.id }
      showFolderDialog.value = true
      break
    case 'delete':
      ElMessageBox.confirm(
        `确定删除 "${node.name}" 吗？${node.type === 'folder' ? '（包含的所有子项将一并删除）' : ''}`,
        '删除确认', { type: 'warning' }
      ).then(() => {
        hostStore.deleteHost(node.id)
        pruneSelection()
        ElMessage.success('已删除')
      }).catch(() => {})
      break
  }
}

/** 查看会话密码：受「查看会话密码」保护范围约束，验证通过后才展示明文 */
async function viewPassword(node: HostItem): Promise<void> {
  if (node.type !== 'host') return
  if (!(await ensureUnlocked('view_password'))) return
  const cfg = node.config
  const secret = cfg?.password || cfg?.passphrase || ''
  const text = secret ? secret : '（该会话未设置密码）'
  ElMessageBox.alert(`会话「${node.name}」的密码：\n\n${text}`, '查看会话密码', {
    confirmButtonText: '知道了',
    type: 'info',
    customClass: 'jc-msgbox-monospace',
  }).catch(() => {})
}

/** 复制会话：基于当前会话创建一份配置完全相同的新会话（名称自动加「副本」后缀） */
function duplicateSession(node: HostItem): void {
  if (node.type !== 'host') return
  const copy = hostStore.addHost({
    name: `${node.name} 副本`,
    addr: node.addr,
    namespaceId: node.namespaceId,
    parentId: node.parentId,
    type: 'host',
    config: node.config ? { ...node.config } : undefined
  })
  ElMessage.success(`已复制会话：${copy.name}`)
}

/** 复制地址：把会话的连接地址写入剪贴板，并提示「已复制」 */
async function copySessionAddress(node: HostItem): Promise<void> {
  const addr = node.addr?.trim() || ''
  if (!addr) {
    ElMessage.warning('该会话没有可复制的地址')
    return
  }
  try {
    await copyText(addr)
    ElMessage.success('已复制')
  } catch {
    ElMessage.error('复制失败')
  }
}

// ===== 工具栏按钮 =====
/** 当前命名空间显示名 */
const currentNamespaceName = computed(() => {
  const ns = hostStore.namespaces.find(n => n.id === hostStore.currentNamespaceId)
  return ns?.name || '默认'
})

const toolbarButtons = computed<ToolbarButton[]>(() => [
  {
    key: 'namespace',
    icon: Collection,
    tooltip: '命名空间：' + currentNamespaceName.value,
    onClick: (e?: any) => openNamespaceDropdown(e as MouseEvent)
  },
  {
    key: 'new-session',
    icon: Plus,
    tooltip: '新建会话',
    onClick: () => openNewSession()
  },
  {
    key: 'new-folder',
    icon: FolderOpened,
    tooltip: '新建分组',
    onClick: () => openNewFolder()
  },
  {
    key: 'refresh',
    icon: Refresh,
    tooltip: '刷新',
    onClick: () => ElMessage.success('刷新完成')
  },
  {
    // 图标取「数据相对本应用的流向」：导入 = 数据进来（↓ Download），导出 = 数据送出（↑ Upload）
    key: 'import',
    icon: Download,
    tooltip: '导入会话',
    onClick: () => openImportSessions()
  },
  {
    key: 'export',
    icon: Upload,
    tooltip: '导出会话',
    onClick: () => openExportSessions()
  }
])

// ===== 命名空间下拉弹层 =====
const nsDropdownVisible = ref(false)
const nsDropdownPos = ref({ x: 0, y: 0 })
const nsButtonRef = ref<HTMLElement | null>(null)

function openNamespaceDropdown(e: MouseEvent) {
  e.stopPropagation()
  // 计算弹层位置：在按钮下方
  const btn = (e.currentTarget as HTMLElement)
  const rect = btn.getBoundingClientRect()
  nsDropdownPos.value = { x: rect.left, y: rect.bottom + 4 }
  nsDropdownVisible.value = !nsDropdownVisible.value
}

function selectNamespace(id: string) {
  hostStore.selectNamespace(id)
  nsDropdownVisible.value = false
}

function closeNsDropdown() {
  nsDropdownVisible.value = false
}

// ===== 移动端：命名空间下拉 + 右下角 FAB 扇形菜单（PC 端不渲染，见模板 v-if）=====

/** 命名空间下拉切换：选中即切换命名空间（「管理命名空间」已移到下拉右侧独立按钮） */
function onMobileNamespaceChange(id: string | number) {
  selectNamespace(String(id))
}

/** 扇形菜单展开状态 */
const fanOpen = ref(false)
function toggleFan() {
  fanOpen.value = !fanOpen.value
}

/** 扇形菜单项：与桌面工具栏按钮一一对应（除命名空间外）。
 *  导入/导出已移到头部命名空间行（纯图标按钮），这里只留新建与刷新 */
const fanItems = [
  // 「新建会话」用终端图标（>_）：语义上就是"开一个终端会话"，比加号更直观
  { key: 'new-session', icon: markRaw(IconTerminal), label: '新建会话', action: 'new-session' },
  { key: 'new-folder', icon: FolderOpened, label: '新建分组', action: 'new-folder' },
  { key: 'refresh', icon: Refresh, label: '刷新', action: 'refresh' }
] as const

/** 扇形半径与角度分布：从正左（180°）扫到正上（90°），三项等角分布。
 *  半径取「相邻菜单项刚好挨在一起」的紧凑值：
 *  圆心距 = 2·r·sin(步进角/2)，r=88、步进 45° 时圆心距 ≈ 67px（58px 圆 → 约 9px 间隙），
 *  既紧凑贴合右下角，又不会相互重叠，也不会压住 FAB（FAB 半径 26 + 项半径 29 = 55 < 88）。 */
const FAN_RADIUS = 88
const FAN_STEP = 90 / (fanItems.length - 1)

function fanItemStyle(i: number): Record<string, string> {
  const rad = (Math.PI * (180 - FAN_STEP * i)) / 180
  return {
    '--fan-dx': Math.round(Math.cos(rad) * FAN_RADIUS) + 'px',
    '--fan-dy': -Math.round(Math.sin(rad) * FAN_RADIUS) + 'px'
  }
}

/** 扇形菜单项点击：收起菜单后执行与桌面工具栏相同的动作 */
function onFanAction(item: (typeof fanItems)[number]) {
  fanOpen.value = false
  handleBlankAction(item.action)
}

// ===== 文件夹选项（用于选择上级） =====
const folderOptions = computed(() => {
  const result: HostItem[] = []
  function collect(items: (HostItem & { children?: HostItem[] })[], depth: number) {
    items.forEach(item => {
      if (item.type === 'folder') {
        result.push(item)
        if (item.children) collect(item.children, depth + 1)
      }
    })
  }
  collect(hostStore.treeData as any, 0)
  return result
})

function getFolderLabel(folder: HostItem): string {
  if (folderOptions.value.length <= 1) return folder.name
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

// ===== 操作方法 =====

function openNewSession(parentFolderId?: string) {
  // 移动端改为整页（见 MobileSessionEditPage）；桌面仍用弹窗
  if (isMobile) { dockStore.pushMobileSubPage({ type: 'session-edit', defaultParentId: parentFolderId || null }); return }
  sessionDialogMode.value = 'create'
  editingNode.value = null
  newSessionParentId.value = parentFolderId || null
  showSessionDialog.value = true
}

function openNewFolder(parentFolderId?: string) {
  folderForm.value = { name: '', parentId: parentFolderId || null }
  showFolderDialog.value = true
}


function saveFolder() {
  const f = folderForm.value
  if (!f.name.trim()) { ElMessage.warning('请输入分组名称'); return }
  // 层级上限：第 3 层分组内不能再建分组（store 返回 null 表示被拦下）
  const created = hostStore.addFolder(f.name, f.parentId)
  if (!created) { ElMessage.warning('目录层级最多三层'); return }
  showFolderDialog.value = false
  ElMessage.success('分组已创建')
}

function onOpenHost(host: HostItem) {
  hostStore.setActiveHost(host.id)
  emit('open', host)
}

function onNodeContextMenu(node: HostItem, e: MouseEvent) {
  // 文件管理器行为：右键未选中项 → 先单选它；右键已选中项 → 保留整组选择（便于批量操作）
  if (!selectedIds.value.includes(node.id)) {
    selectedIds.value = [node.id]
    selectionAnchor.value = node.id
    hostStore.setActiveHost(node.id)
  }
  openContextMenu(node, e)
}

function onNewSessionInFolder(node: HostItem) {
  if (node.type === 'folder') openNewSession(node.id)
}

/** 某父级下的子项（可排除指定节点），按当前排序 */
function childrenOf(parentId: string | null, excludeId?: string): HostItem[] {
  return hostStore.hosts
    .filter(h => h.namespaceId === hostStore.currentNamespaceId && h.parentId === parentId && h.id !== excludeId)
    .sort((a, b) => a.sortOrder - b.sortOrder)
}

/**
 * 拖拽落位：把「目标节点 + 放置位置」翻译成 (父级, 索引)。
 * - targetId 为 null：拖到列表空白/无有效目标 → 移到顶级目录末尾
 * - 索引基于「已排除被拖节点」的兄弟列表计算，与 store.moveNode 内部语义一致，
 *   否则同层内上移/下移时会因移除自身产生索引偏移。
 */
function onMoveNode(draggedId: string, targetId: string | null, position: DropPosition) {
  try {
    // 拖动的是多选中的一项时，整组一起移动（对齐文件管理器）
    const moving =
      selectedIds.value.includes(draggedId) && selectedIds.value.length > 1
        ? selectionRoots()
        : [draggedId]

    if (targetId !== null && moving.includes(targetId)) return

    let parentId: string | null
    let index: number

    if (targetId === null) {
      // 拖到列表空白/无有效目标 = 顶级目录末尾（与编辑弹窗选「无」语义一致）
      parentId = null
      index = childrenOf(null, draggedId).length
    } else if (position === 'inner') {
      parentId = targetId
      index = childrenOf(targetId, draggedId).length
    } else {
      const target = hostStore.hosts.find(h => h.id === targetId)
      if (!target) return
      parentId = target.parentId
      const sibs = childrenOf(parentId, draggedId)
      const ti = sibs.findIndex(s => s.id === targetId)
      if (ti < 0) return
      index = position === 'before' ? ti : ti + 1
    }

    for (const id of moving) {
      if (id === targetId) continue
      hostStore.moveNode(id, parentId, index)
      index++
    }
    // 移动后保持整组选中
    if (moving.length > 1) selectedIds.value = moving
  } catch (e: any) {
    handleApiError(e, 'components/HostListPanel.onMoveNode', { prefix: '移动失败' })
  }
}

// ===== 多选：Ctrl 点选 / Shift 区间 / Ctrl+A 全选 / Esc 取消 =====

/** 当前可见（已渲染）的节点 id，按视觉顺序排列 */
function visibleNodeIds(): string[] {
  const root = listRef.value
  if (!root) return []
  return Array.from(root.querySelectorAll<HTMLElement>('[data-host-node-id]'))
    .map((el) => el.dataset.hostNodeId || '')
    .filter(Boolean)
}

/**
 * Shift 区间选择：取 anchor→target 之间「当前可见顺序」的节点。
 * 直接读 DOM 顺序，天然与展开/折叠后的视觉顺序一致（折叠节点未渲染，不会误选）。
 */
function visibleRange(anchorId: string, targetId: string): string[] {
  const ids = visibleNodeIds()
  const a = ids.indexOf(anchorId)
  const b = ids.indexOf(targetId)
  if (a < 0 || b < 0) return [targetId]
  return a <= b ? ids.slice(a, b + 1) : ids.slice(b, a + 1)
}

/** 单击节点：会话单击只选中，双击才连接；Ctrl 切换、Shift 区间（对齐文件管理器） */
function onSelectNode(id: string, e?: MouseEvent) {
  const range = !!e && e.shiftKey
  const toggle = !!e && (e.ctrlKey || e.metaKey)
  let next: string[]

  if (range && selectionAnchor.value && selectedIds.value.includes(selectionAnchor.value)) {
    next = visibleRange(selectionAnchor.value, id)
  } else if (toggle) {
    const set = new Set(selectedIds.value)
    if (set.has(id)) set.delete(id)
    else set.add(id)
    next = [...set]
    selectionAnchor.value = id
  } else {
    next = [id]
    selectionAnchor.value = id
  }

  selectedIds.value = next
  hostStore.setActiveHost(next.includes(id) ? id : (next[next.length - 1] ?? null))
}

/** 清空多选（不动 activeHost，避免影响终端等外部引用） */
function clearSelection() {
  selectedIds.value = []
  selectionAnchor.value = null
}

/** 清理选中集合中已不存在的节点（单条删除/外部删除后调用） */
function pruneSelection() {
  const alive = new Set(hostStore.hosts.map((h) => h.id))
  const next = selectedIds.value.filter((id) => alive.has(id))
  if (next.length !== selectedIds.value.length) selectedIds.value = next
}

/** 全选当前可见节点 */
function selectAllVisible() {
  const ids = visibleNodeIds()
  if (ids.length === 0) return
  selectedIds.value = ids
  selectionAnchor.value = ids[0]
  hostStore.setActiveHost(ids[ids.length - 1])
}

/** 选中集合里去掉「父级也被选中」的项，避免批量操作重复处理子项 */
function selectionRoots(): string[] {
  const sel = new Set(selectedIds.value)
  return selectedIds.value.filter((id) => {
    let p = hostStore.hosts.find((h) => h.id === id)?.parentId ?? null
    while (p) {
      if (sel.has(p)) return false
      p = hostStore.hosts.find((h) => h.id === p)?.parentId ?? null
    }
    return true
  })
}

/** 某节点是否位于选中集合之下（用于过滤非法移动目标） */
function isUnderSelection(nodeId: string): boolean {
  const sel = new Set(selectedIds.value)
  let p = hostStore.hosts.find((h) => h.id === nodeId)?.parentId ?? null
  while (p) {
    if (sel.has(p)) return true
    p = hostStore.hosts.find((h) => h.id === p)?.parentId ?? null
  }
  return false
}

/** 「移动到…」可选目标：排除被选中的文件夹及其子孙，避免把文件夹移进自己里面 */
const moveTargetOptions = computed(() =>
  folderOptions.value
    .filter((f) => !selectedIds.value.includes(f.id) && !isUnderSelection(f.id))
    .map((f) => ({ id: f.id, label: getFolderLabel(f) })),
)

/** 批量操作统一入口 */
async function handleBatchAction(action: string) {
  const ids = selectedIds.value.slice()
  if (ids.length === 0) return

  if (action === 'batch-connect') {
    const hosts = ids
      .map((id) => hostStore.hosts.find((h) => h.id === id))
      .filter((h): h is HostItem => !!h && h.type === 'host')
    if (hosts.length === 0) {
      ElMessage.warning('选中的项目里没有可连接的会话')
      return
    }
    hosts.forEach((h) => onOpenHost(h))
    return
  }

  if (action === 'batch-copy-address') {
    const addrs = ids
      .map((id) => hostStore.hosts.find((h) => h.id === id))
      .filter((h): h is HostItem => !!h && h.type === 'host')
      .map((h) => `${h.config?.username ? h.config.username + '@' : ''}${h.addr}`)
    if (addrs.length === 0) {
      ElMessage.warning('选中的项目里没有会话')
      return
    }
    copyText(addrs.join('\n'))
    ElMessage.success(`已复制 ${addrs.length} 个地址`)
    return
  }

  if (action === 'batch-move') {
    moveTarget.value = '__root__'
    showMoveDialog.value = true
    return
  }

  if (action === 'batch-delete') {
    await batchDelete()
  }
}

/** 批量删除（受「修改删除会话」安全范围约束） */
async function batchDelete() {
  const ids = selectionRoots()
  if (ids.length === 0) return
  const hasFolder = ids.some((id) => hostStore.hosts.find((h) => h.id === id)?.type === 'folder')
  try {
    await ElMessageBox.confirm(
      `确定删除选中的 ${ids.length} 项吗？${hasFolder ? '（分组内的子项将一并删除）' : ''}`,
      '批量删除',
      { type: 'warning' },
    )
  } catch {
    return
  }
  ids.forEach((id) => hostStore.deleteHost(id))
  clearSelection()
  ElMessage.success(`已删除 ${ids.length} 项`)
}

/** 确认批量移动 */
async function confirmMove() {
  const ids = selectionRoots()
  if (ids.length === 0) return
  if (!(await ensureUnlocked('edit_delete'))) return
  const target = moveTarget.value === '__root__' ? null : moveTarget.value
  let moved = 0
  let skipped = 0
  for (const id of ids) {
    try {
      // 追加到目标末尾（moveNode 内部会重排 sortOrder）
      hostStore.moveNode(id, target, Number.MAX_SAFE_INTEGER)
      moved++
    } catch {
      skipped++ // 目标非法（移到自身/子孙），跳过
    }
  }
  showMoveDialog.value = false
  clearSelection()
  ElMessage.success(`已移动 ${moved} 项${skipped > 0 ? `，${skipped} 项因目标非法被跳过` : ''}`)
}

function onEditNode(node: HostItem) {
  if (node.type === 'host') {
    // 移动端改为整页（见 MobileSessionEditPage）；桌面仍用弹窗
    if (isMobile) { dockStore.pushMobileSubPage({ type: 'session-edit', hostId: node.id }); return }
    sessionDialogMode.value = 'edit'
    editingNode.value = node
    showSessionDialog.value = true
  } else {
    ElMessageBox.prompt('分组名称', '编辑分组', { inputValue: node.name }).then(({ value }) => {
      hostStore.updateHost(node.id, { name: value })
      ElMessage.success('已更新')
    }).catch(() => {})
  }
}

// ===== 命名空间操作 =====
function openNamespaceManager() {
  showNamespaceDialog.value = true
}

function addNamespace() {
  const name = newNamespaceName.value.trim()
  if (!name) { ElMessage.warning('请输入命名空间名称'); return }
  hostStore.addNamespace(name)
  newNamespaceName.value = ''
  ElMessage.success('命名空间已添加')
}

function renameNamespace(ns: { id: string; name: string }) {
  ElMessageBox.prompt('输入新名称', '重命名命名空间', {
    inputValue: ns.name, inputPattern: /.+/, inputErrorMessage: '名称不能为空'
  }).then(({ value }) => {
    hostStore.renameNamespace(ns.id, value)
    ElMessage.success('已重命名')
  }).catch(() => {})
}

function deleteNamespace(id: string) {
  if (hostStore.namespaces.length <= 1) { ElMessage.warning('至少保留一个命名空间'); return }
  ElMessageBox.confirm('删除命名空间将同时删除其中所有主机和分组，确定继续？', '删除确认', { type: 'warning' })
    .then(() => { hostStore.deleteNamespace(id); ElMessage.success('命名空间已删除') })
    .catch(() => {})
}

// ===== 折叠态：一级条目图标 + 文件夹二级悬浮菜单 =====
const collapsedSelectedId = ref<string | null>(null)
const collapsedSubmenu = ref({
  visible: false,
  x: 0,
  y: 0,
  folderName: '',
  children: [] as HostTreeNode[]
})
let collapsedLeaveTimer: ReturnType<typeof setTimeout> | null = null

/**
 * 折叠态会话 tip：与文件夹二级菜单一致，hover 立即出现。
 * 不用原生 title —— 浏览器原生提示有 ~1s 延迟，达不到「立即出现」的要求。
 */
const collapsedTip = ref({
  visible: false,
  x: 0,
  y: 0,
  name: '',
  addr: ''
})

/** 收回会话 tip（随二级菜单的收起时机一起走，避免交叉残留） */
function hideCollapsedTip(): void {
  collapsedTip.value.visible = false
}

/**
 * 折叠态单击连接去重：单击即连后，快速双击会触发两次 click，
 * 500ms 内对同一会话的第二次点击不再重复开 Tab。
 */
let lastCollapsedConnectId = ''
let lastCollapsedConnectAt = 0
function connectCollapsedOnce(nodeId: string, connect: () => void): void {
  const now = Date.now()
  if (nodeId === lastCollapsedConnectId && now - lastCollapsedConnectAt < 500) return
  lastCollapsedConnectId = nodeId
  lastCollapsedConnectAt = now
  connect()
}

function showCollapsedTip(node: HostTreeNode, target: HTMLElement): void {
  const rect = target.getBoundingClientRect()
  collapsedTip.value = {
    visible: true,
    x: props.dock === 'bottom' ? rect.left : rect.right + 4,
    y: props.dock === 'bottom' ? rect.top - 4 : rect.top,
    name: node.name,
    addr: node.type === 'host' ? (node.addr || '') : ''
  }
}

function iconColorFor(node: HostTreeNode): string {
  return node.type === 'folder' ? '#f0b429' : 'var(--jc-text-muted)'
}

/** 弹层定位：left/right dock 在图标右侧弹出；bottom dock 在图标上方弹出 */
function showCollapsedSubmenu(node: HostTreeNode, target: HTMLElement) {
  const rect = target.getBoundingClientRect()
  collapsedSubmenu.value = {
    visible: true,
    x: props.dock === 'bottom' ? rect.left : rect.right + 4,
    y: props.dock === 'bottom' ? rect.top - 4 : rect.top,
    folderName: node.name,
    children: node.children || []
  }
}

/** bottom dock 时弹层向上展开 */
const collapsedSubmenuTransform = computed(() =>
  props.dock === 'bottom' ? 'translateY(-100%)' : undefined
)

function onCollapsedIconHover(node: HostTreeNode, e: MouseEvent) {
  if (collapsedLeaveTimer) { clearTimeout(collapsedLeaveTimer); collapsedLeaveTimer = null }
  if (node.type !== 'folder') {
    // 会话：hover 立即弹 tip（对齐文件夹二级菜单的即时性）
    collapsedSubmenu.value.visible = false
    showCollapsedTip(node, e.currentTarget as HTMLElement)
    return
  }
  hideCollapsedTip()
  showCollapsedSubmenu(node, e.currentTarget as HTMLElement)
}

function onCollapsedIconLeave() {
  collapsedLeaveTimer = setTimeout(() => {
    collapsedSubmenu.value.visible = false
    collapsedTip.value.visible = false
  }, 150)
}

function cancelCollapsedLeave() {
  if (collapsedLeaveTimer) { clearTimeout(collapsedLeaveTimer); collapsedLeaveTimer = null }
}

function hideCollapsedSubmenu() {
  collapsedSubmenu.value.visible = false
  collapsedTip.value.visible = false
}

/**
 * 一级图标单击：文件夹=弹出二级菜单；会话=立即连接（单击即连，不再要求双击）。
 */
function onCollapsedIconClick(node: HostTreeNode, e: MouseEvent) {
  collapsedSelectedId.value = node.id
  if (node.type === 'folder') {
    showCollapsedSubmenu(node, e.currentTarget as HTMLElement)
  } else {
    connectCollapsedOnce(node.id, () => onOpenHost(node))
    hideCollapsedSubmenu()
  }
}

/** 二级菜单项 hover：会话项立即弹 tip */
function onCollapsedChildHover(child: HostTreeNode, e: MouseEvent) {
  showCollapsedTip(child, e.currentTarget as HTMLElement)
}

/**
 * 二级菜单项单击：会话=立即连接；文件夹=切换显示其子项。
 */
function onCollapsedChildClick(child: HostTreeNode) {
  collapsedSelectedId.value = child.id
  if (child.type === 'folder') {
    collapsedSubmenu.value = {
      ...collapsedSubmenu.value,
      folderName: child.name,
      children: child.children || []
    }
  } else {
    connectCollapsedOnce(child.id, () => onOpenHost(child))
    hideCollapsedSubmenu()
  }
}

// ===== 点击外部关闭右键菜单 =====
function handleClickOutside(e: MouseEvent) {
  if (contextMenu.value.visible) {
    const el = e.target as HTMLElement
    if (!el.closest('.context-menu-menu')) {
      closeContextMenu()
    }
  }
}

// ===== 全局快捷键：Ctrl/Cmd+A 全选可见会话、Esc 取消选择 =====
function onGlobalKeydown(e: KeyboardEvent) {
  const el = document.activeElement as HTMLElement | null
  // 输入框 / 终端里不抢快捷键
  if (el && el.closest('input, textarea, [contenteditable="true"], .xterm')) return

  const inPanel = !!el && !!listRef.value?.contains(el)
  const focusFree = !el || el === document.body

  if (e.key === 'Escape') {
    if (selectedIds.value.length > 0 && (inPanel || focusFree)) clearSelection()
    return
  }

  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'a') {
    // 仅在面板内交互过（已有选中）或焦点在面板内时才全选，避免全局误触
    if (!inPanel && selectedIds.value.length === 0) return
    e.preventDefault()
    selectAllVisible()
  }
}

// ===== 初始化 =====
onMounted(async () => {
  if (!hostStore.loaded) await hostStore.load()
  document.addEventListener('click', handleClickOutside)
  window.addEventListener('keydown', onGlobalKeydown)
})

onBeforeUnmount(() => {
  document.removeEventListener('click', handleClickOutside)
  window.removeEventListener('keydown', onGlobalKeydown)
})
</script>

<style scoped>
.menu-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 7px 12px;
  font-size: 13px;
  color: var(--jc-text);
  cursor: pointer;
  transition: background-color 0.12s;
  white-space: nowrap;
}
.menu-item:hover {
  background-color: var(--jc-border);
  color: var(--jc-text);
}
.menu-divider {
  height: 1px;
  background-color: var(--jc-border);
  margin: 4px 8px;
}

/* 普通下拉（上级分组）与浮动标签文本框高度保持一致（36px） */
.tall-field :deep(.el-select__wrapper) {
  min-height: 36px;
}

/* 密钥文件行的「浏览/清除」按钮：与同排输入框高度对齐（36px） */
.jc-key-btn {
  height: 36px;
}

</style>

<style>
.jc-dialog.el-dialog {
  background-color: var(--jc-bg-soft) !important;
  border: 1px solid var(--jc-border);
  border-radius: 8px;
}
.jc-dialog .el-dialog__header {
  border-bottom: 1px solid var(--jc-border);
  margin-right: 0;
  padding: 12px 16px;
}
.jc-dialog .el-dialog__title {
  color: var(--jc-text);
  font-weight: 600;
  font-size: 14px;
}
.jc-dialog .el-dialog__body {
  /* 同 views/MainWindow.vue：Element Plus 的 .el-dialog 自带 padding:16px，
     会把 body 内缩 16px，形成一圈只能由 .el-dialog 自身背景绘制的条带。
     弹窗高度变化后该条带会残留未重绘的空洞。用负外边距让 body 撑满整宽、
     由自身背景覆盖条带；左右内边距补回 16px，内容位置不变。 */
  margin: 0 -16px;
  padding: 16px 32px;
  color: var(--jc-text);
}
.jc-dialog .el-dialog__headerbtn .el-dialog__close {
  color: var(--jc-text-muted);
}
.jc-dialog .el-dialog__headerbtn:hover .el-dialog__close {
  color: var(--jc-text);
}
.jc-dialog .el-dialog__footer {
  border-top: 1px solid var(--jc-border);
  padding: 10px 16px;
}
/* 「测试连接」按钮：中性灰填充 + 浅色文字（固定浅色，任何主题都清晰可读），
 *  不引入新色相；底色比深色弹窗背景（--jc-bg-soft）明显亮一阶，确保按钮轮廓可见 */
.jc-dialog .jc-test-conn-btn.el-button {
  background-color: #3a3a3e;
  border-color: #4b5563;
  color: #f5f5f7;
}
.jc-dialog .jc-test-conn-btn.el-button:hover,
.jc-dialog .jc-test-conn-btn.el-button:focus,
.jc-dialog .jc-test-conn-btn.el-button:active {
  background-color: #45454a;
  border-color: #6b7280;
  color: #f5f5f7;
}
.jc-dialog .el-input__wrapper {
  background-color: var(--jc-bg) !important;
  box-shadow: 0 0 0 1px var(--jc-border) inset !important;
}
.jc-dialog .el-input__inner {
  color: var(--jc-text) !important;
}
.jc-dialog .el-select__wrapper {
  background-color: var(--jc-bg) !important;
  box-shadow: 0 0 0 1px var(--jc-border) inset !important;
}
/* 会话弹窗：输入框/下拉底色与弹窗 body 一致（对齐「端口转发」表单）。
   这是骑线浮动标签能无缝衔接的前提——标签单色背景在弹窗底色与输入框内侧
   都是同色，只在边框线处「切」出一道缺口，不会出现色块遮挡感 */
.jc-session-dialog .el-input__wrapper,
.jc-session-dialog .el-select__wrapper {
  background-color: var(--jc-bg-panel) !important;
}
.jc-dialog .el-select__placeholder,
.jc-dialog .el-select__selected-item {
  color: var(--jc-text) !important;
}
.jc-dialog .el-input-number__decrease,
.jc-dialog .el-input-number__increase {
  background-color: var(--jc-bg) !important;
  border-color: var(--jc-border) !important;
  color: var(--jc-text-muted) !important;
}

/* ElMessageBox 暗色主题 */
.el-message-box {
  background-color: var(--jc-bg-soft) !important;
  border: 1px solid var(--jc-border) !important;
}
.el-message-box__header {
  background-color: var(--jc-bg-soft) !important;
  border-bottom: 1px solid var(--jc-border);
}
.el-message-box__title {
  color: var(--jc-text) !important;
}
.el-message-box__content {
  color: var(--jc-text-soft) !important;
}
.el-message-box__message {
  color: var(--jc-text-soft) !important;
}
.el-message-box__input {
  background-color: var(--jc-bg) !important;
  color: var(--jc-text) !important;
  border: 1px solid var(--jc-border) !important;
}
.el-message-box__btns {
  border-top: 1px solid var(--jc-border);
  background-color: var(--jc-bg-soft) !important;
}
.el-message-box__headerbtn .el-message-box__close {
  color: var(--jc-text-muted) !important;
}
.el-message-box__headerbtn:hover .el-message-box__close {
  color: var(--jc-text) !important;
}
</style>
