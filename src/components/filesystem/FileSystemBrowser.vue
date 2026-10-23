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

﻿<template>
  <div ref="fsRootRef" class="fs-browser relative h-full flex flex-col bg-[var(--jc-bg)] overflow-hidden">
    <!-- Toolbar：按钮位置随 dock 自动排布，折叠态保留头部；分屏内可整体隐藏（hideHeader） -->
    <PanelHeader v-if="!hideHeader" :panel-id="grip ? 'fileSystem' : undefined" :dock="dock" :collapsed="collapsed" :hide-collapse="hideCollapse" @collapse="$emit('collapse')">
      <template v-if="!collapsed">
        <!-- Breadcrumb：默认面包屑模式；点击面包屑末端之后的空白处 → Windows 风格路径编辑模式 -->
        <div
          v-if="!pathEditing"
          ref="breadcrumbRef"
          class="fs-breadcrumb flex-1 flex items-center gap-0.5 px-2 text-xs overflow-x-auto scrollbar-hide min-w-0 cursor-text"
          title="点击空白处编辑路径"
          @click.self="startPathEdit"
        >
          <span
            v-for="(seg, i) in pathSegments"
            :key="i"
            class="flex items-center gap-0.5 shrink-0"
          >
            <span
              class="inline-flex items-center justify-center min-w-[24px] h-6 px-1.5 rounded cursor-pointer hover:text-blue-400 hover:bg-white/5 text-[var(--jc-text)]"
              @click="navigateTo(i)"
            >{{ seg }}</span>
            <!-- 末段箭头与空白同属"编辑触发区"，点击也进入编辑；中间箭头保持不可点 -->
            <el-icon
              size="10"
              class="text-[var(--jc-text-muted)]"
              :class="{ 'cursor-text': i === pathSegments.length - 1 }"
              @click="onBreadcrumbArrowClick(i)"
            ><ArrowRight /></el-icon>
          </span>
        </div>
        <!-- 移动端：一键复制当前路径（仅图标，PC 端不显示 —— 桌面已有右键菜单等入口） -->
        <button
          v-if="isMobile && !pathEditing"
          class="fs-copy-path shrink-0"
          title="复制当前路径"
          @click.stop="copyCurrentPath"
        >
          <el-icon :size="15"><CopyDocument /></el-icon>
        </button>
        <!-- 路径编辑模式：完整路径文本框，Enter 导航 / Esc 或失焦还原 -->
        <div v-else-if="pathEditing" class="fs-breadcrumb fs-breadcrumb--edit flex-1 min-w-0 px-1.5 flex items-center">
          <input
            ref="pathEditInputRef"
            v-model="pathEditText"
            draggable="false"
            spellcheck="false"
            class="w-full h-7 bg-[var(--jc-bg-soft)] border border-blue-500/60 rounded px-2 text-xs text-[var(--jc-text)] outline-none"
            @keydown.enter.prevent="commitPathEdit"
            @keydown.esc.prevent="cancelPathEdit"
            @keydown.stop
            @mousedown.stop
            @click.stop
            @blur="onPathEditBlur"
          />
        </div>
        <!-- 洞口吸入光效：面包屑路径过长被右侧工具栏挡住时出现 -->
        <div v-if="breadcrumbOverflow" class="suck-light" aria-hidden="true">
          <div class="suck-light__glow"></div>
        </div>

        <!-- 工具条：所有按钮统一尺寸（与 AdaptiveToolbar 风格一致），按功能用 | 分组；
             提供 toolbarTeleport 时渲染到外部容器（如 SFTP 分屏的窗格头行）；defer 确保目标元素先渲染。
             包一层 .fs-mobile-toolbar：移动端用 CSS 把它定位到面板底部（工具栏/面包屑对调，见 mobile.css 19 节） -->
        <Teleport :to="toolbarTeleport || 'body'" :disabled="!toolbarTeleport" defer>
          <div class="fs-mobile-toolbar"><AdaptiveToolbar :buttons="toolbarButtons" :stretch="false" /></div>
        </Teleport>
      </template>
    </PanelHeader>

    <!-- 折叠态：占位图标（FileSystem 固定 FolderOpened） -->
    <Transition name="jc-swap">
      <div
        v-if="collapsed"
        class="flex-1 min-h-0 min-w-0 p-1 flex gap-1 items-center justify-center"
        :class="dock === 'bottom' ? 'flex-row overflow-x-auto' : 'flex-col overflow-y-auto overflow-x-hidden'"
      >
        <div class="w-7 h-7 shrink-0 flex items-center justify-center rounded text-[var(--jc-text-muted)]" title="文件系统（已折叠）">
          <el-icon :size="16" color="#f0b429"><FolderOpened /></el-icon>
        </div>
      </div>
    </Transition>

    <!-- Content Area（空白处也可作为放置目标 → 当前目录） -->
    <Transition name="jc-swap">
      <div
        v-show="!collapsed"
        ref="fsContentRef"
      class="fs-content relative flex-1 min-h-0 overflow-hidden"
      :data-fs-dir="pathStack.length > 0 ? currentPath : undefined"
      @click.self="onContentBgClick"
      @dragover="onContainerDragOver"
      @drop="onContainerDrop"
      @scroll="onFsScrollLocal"
    >
      <!-- List View -->
      <div v-if="viewMode === 'list'" class="h-full overflow-auto" :style="{ fontSize: baseTextSize * itemZoom + 'px' }">
        <table class="min-w-full" :style="{ width: tableWidth + 'px', tableLayout: 'fixed', userSelect: resizingKey ? 'none' : undefined }">
          <thead class="sticky top-0 z-10 bg-[var(--jc-bg-soft)] text-[var(--jc-text-soft)]" :style="{ fontSize: 12 * itemZoom + 'px' }">
            <tr class="border-b border-[var(--jc-border)]">
              <th
                v-for="col in FS_COLUMNS"
                :key="col.key"
                class="relative text-left px-3 py-1.5 font-medium whitespace-nowrap"
                :class="{ 'text-right': col.align === 'right', 'cursor-pointer': !!col.sortField }"
                :style="{ width: colW(col.key) + 'px' }"
                @click="col.sortField && sortBy(col.sortField)"
              >{{ col.label }}<span
                  class="col-resize-handle"
                  @mousedown.prevent.stop="startColResize(col.key, $event)"
                  @click.stop
                ></span></th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="item in sortedChildren"
              :key="item.path"
              class="border-b border-[var(--jc-bg-hover)] hover:bg-[var(--jc-bg-hover)] cursor-pointer"
              :class="{ 'bg-blue-600/20': selectedPaths.has(item.path), 'bg-blue-600/30': dropTargetPath === item.path }"
              :draggable="canDrag"
              :data-path="item.path"
              :data-hover-click="item.type === 'dir' ? '' : undefined"
              :data-fs-dir="item.type === 'dir' ? item.path : undefined"
              @dragstart="onRowDragStart(item, $event)"
              @dragend="resetRemoteFileDrag"
              @dragover="onRowDragOver(item, $event)"
              @dragleave="onRowDragLeave(item)"
              @drop="onRowDrop(item, $event)"
              @click="onRowClick(item, $event)"
              @dblclick="isMobile ? undefined : openItem(item)"
              @contextmenu.prevent.stop="onItemContextMenu(item, $event)"
              @touchstart.passive="onRowTouchStart(item, $event)"
              @touchmove.passive="onRowTouchMove"
              @touchend="onRowTouchEnd"
              @touchcancel="onRowTouchCancel"
            >
              <td class="px-3 overflow-hidden" :style="{ paddingTop: 4 * itemZoom + 'px', paddingBottom: 4 * itemZoom + 'px' }">
                <span class="flex items-center gap-1.5 overflow-hidden">
                  <el-icon class="shrink-0" :size="baseIconSize * itemZoom" :color="iconColor(item)">
                    <Folder v-if="item.type === 'dir'" />
                    <Link v-else-if="item.type === 'symlink'" />
                    <Document v-else />
                  </el-icon>
                  <input
                    v-if="renamingPath === item.path"
                    :ref="setupRenameInput"
                    v-model="renamingOriginalName"
                    draggable="false"
                    class="flex-1 min-w-0 bg-[var(--jc-bg-soft)] border border-blue-500/60 rounded px-1 py-0 text-[var(--jc-text)] outline-none"
                    :style="{ fontSize: baseTextSize * itemZoom + 'px' }"
                    @keydown.enter.prevent="commitRename(item, renamingOriginalName)"
                    @keydown.esc.prevent="commitRename(item, renamingOriginalName)"
                    @keydown.stop
                    @mousedown.stop
                    @dragstart.prevent.stop
                    @click.stop
                    @dblclick.stop="selectRenameText"
                    @blur="onRenameBlur(item)"
                    @contextmenu.stop
                  />
                  <span v-else class="truncate text-[var(--jc-text)]" :title="item.name">{{ item.name }}</span>
                </span>
              </td>
              <td class="px-3 text-right text-[var(--jc-text-soft)] whitespace-nowrap overflow-hidden" :style="{ paddingTop: 4 * itemZoom + 'px', paddingBottom: 4 * itemZoom + 'px' }">{{ formatSize(item.size) }}</td>
              <td class="px-3 text-right text-[var(--jc-text-soft)] whitespace-nowrap overflow-hidden" :style="{ paddingTop: 4 * itemZoom + 'px', paddingBottom: 4 * itemZoom + 'px' }">{{ item.modified }}</td>
              <td class="px-3 text-[var(--jc-text-soft)] whitespace-nowrap overflow-hidden" :style="{ paddingTop: 4 * itemZoom + 'px', paddingBottom: 4 * itemZoom + 'px' }">{{ item.permissions }}</td>
              <td class="px-3 text-[var(--jc-text-soft)] whitespace-nowrap overflow-hidden" :style="{ paddingTop: 4 * itemZoom + 'px', paddingBottom: 4 * itemZoom + 'px' }">{{ item.owner }}</td>
            </tr>
            <tr v-if="currentChildren.length === 0">
              <td colspan="5" class="text-center text-[var(--jc-text-muted)] py-4" :style="{ paddingTop: 4 * itemZoom + 'px', paddingBottom: 4 * itemZoom + 'px' }">{{ loading ? '加载中...' : '空文件夹' }}</td>
            </tr>
          </tbody>
        </table>
      </div>

      <!-- Column View (macOS Finder style) -->
      <div v-else-if="viewMode === 'column'" class="h-full flex overflow-x-auto">
        <div
          v-for="(col, ci) in columnData"
          :key="ci"
          class="shrink-0 h-full overflow-y-auto border-r border-[var(--jc-border)] bg-[var(--jc-bg)]"
          :style="{ width: 200 * itemZoom + 'px', minWidth: 180 * itemZoom + 'px' }"
        >
          <div
            v-for="item in col.items"
            :key="item.path"
            class="flex items-center gap-1.5 px-2 cursor-pointer hover:bg-[var(--jc-bg-hover)]"
            :class="columnItemClass(ci, item)"
            :data-path="item.path"
            :data-hover-click="item.type === 'dir' ? '' : undefined"
            :data-fs-dir="item.type === 'dir' ? item.path : undefined"
            :draggable="canDrag"
            :style="{ paddingTop: 4 * itemZoom + 'px', paddingBottom: 4 * itemZoom + 'px', fontSize: baseTextSize * itemZoom + 'px' }"
            @click="onColumnClick(ci, item, $event)"
            @dblclick="openItem(item)"
            @contextmenu.prevent.stop="onItemContextMenu(item, $event)"
            @dragstart="onRowDragStart(item, $event)"
            @dragend="resetRemoteFileDrag"
            @dragover="onRowDragOver(item, $event)"
            @dragleave="onRowDragLeave(item)"
            @drop="onRowDrop(item, $event)"
          >
            <el-icon :size="13 * itemZoom" :color="iconColor(item)">
              <Folder v-if="item.type === 'dir'" />
              <Link v-else-if="item.type === 'symlink'" />
              <Document v-else />
            </el-icon>
            <input
              v-if="renamingPath === item.path"
              :ref="setupRenameInput"
              v-model="renamingOriginalName"
              data-rename-input
              draggable="false"
              class="flex-1 min-w-0 bg-[var(--jc-bg-soft)] border border-blue-500/60 rounded px-1 py-0 text-[var(--jc-text)] outline-none"
              @keydown.enter.prevent="commitRename(item, renamingOriginalName)"
              @keydown.esc.prevent="commitRename(item, renamingOriginalName)"
              @keydown.stop
              @mousedown.stop
              @dragstart.prevent.stop
              @click.stop
              @dblclick.stop="selectRenameText"
              @blur="onRenameBlur(item)"
              @contextmenu.stop
            />
            <span v-else class="flex-1 truncate text-[var(--jc-text)]">{{ item.name }}</span>
            <el-icon v-if="item.type === 'dir'" :size="10 * itemZoom" class="text-[var(--jc-text-muted)]"><ArrowRight /></el-icon>
          </div>
          <div v-if="col.items.length === 0" class="text-center text-[var(--jc-text-muted)] py-4" :style="{ fontSize: baseTextSize * itemZoom + 'px' }">
            {{ loading ? '加载中...' : '空' }}
          </div>
        </div>
      </div>

      <!-- Tree View -->
      <div v-else-if="viewMode === 'tree'" class="h-full overflow-auto py-1" :style="{ fontSize: baseTextSize * itemZoom + 'px' }">
        <div v-if="rootChildren.length === 0 && !(kind === 'local' && localRootsLoaded && pathStack.length === 0)" class="text-center text-[var(--jc-text-muted)] py-8">
          {{ loading ? '加载中...' : '空文件夹' }}
        </div>
        <FsTreeNode
          v-else
          :nodes="(kind === 'local' && isWindows && pathStack.length === 0 ? mappedRootChildren : rootChildren)"
          :level="0"
          :selected-path="selectedPaths.values().next().value ?? ''"
          :current-path="currentPath"
          :active-dir-paths="activeDirPaths"
          :zoom="itemZoom"
          :can-drag="canDrag"
          @select="selectItem"
          @toggle="toggleTreeNode"
          @open="openFromTree"
          @contextmenu="onTreeContextMenu"
          @row-drag-start="onRowDragStart"
          @row-drag-over="onRowDragOver"
          @row-drag-leave="onRowDragLeave"
          @row-drop="onRowDrop"
          @row-drag-end="resetRemoteFileDrag"
        />
      </div>

      <!-- Icon View -->
      <div v-else class="h-full overflow-auto p-3" :style="{ gap: 3 * itemZoom * 4 + 'px', padding: 3 * itemZoom * 4 + 'px' }">
        <div class="grid" :style="{ gridTemplateColumns: 'repeat(auto-fill, minmax(' + (80 * itemZoom) + 'px, 1fr))' }">
          <div
            v-for="item in currentChildren"
            :key="item.path"
            class="flex flex-col items-center gap-1 rounded cursor-pointer hover:bg-[var(--jc-bg-hover)]"
            :class="{ 'bg-blue-600/20': selectedPaths.has(item.path), 'bg-blue-600/30': dropTargetPath === item.path }"
            :data-path="item.path"
            :data-fs-dir="item.type === 'dir' ? item.path : undefined"
            :draggable="canDrag"
            :style="{ padding: 8 * itemZoom + 'px' }"
            @dragstart="onRowDragStart(item, $event)"
            @dragend="resetRemoteFileDrag"
            @dragover="onRowDragOver(item, $event)"
            @dragleave="onRowDragLeave(item)"
            @drop="onRowDrop(item, $event)"
            @click="onRowClick(item, $event)"
            @dblclick="openItem(item)"
            @contextmenu.prevent.stop="onItemContextMenu(item, $event)"
          >
            <el-icon :size="40 * itemZoom" :color="iconColor(item)">
              <Folder v-if="item.type === 'dir'" />
              <Link v-else-if="item.type === 'symlink'" />
              <Document v-else />
            </el-icon>
            <span class="text-[var(--jc-text)] text-center break-all line-clamp-2" :style="{ fontSize: 12 * itemZoom + 'px' }">{{ item.name }}</span>
          </div>
          <div v-if="currentChildren.length === 0" class="col-span-full text-center text-[var(--jc-text-muted)] py-8" :style="{ fontSize: 14 * itemZoom + 'px' }">
            {{ loading ? '加载中...' : '空文件夹' }}
          </div>
        </div>
        </div>

        <!-- 文件列表加载过渡：只盖「列表内容区」，不盖头部/底部工具栏
             （否则点工具栏按钮时整屏被覆盖，像没反应或整页在刷新）。
             仅「显式加载」（初始 / 切换目录 / 手动刷新）时出现，淡入淡出 -->
        <Transition name="jc-fs-fade">
          <div v-if="overlayLoading" class="fs-loading" role="status" aria-live="polite">
            <el-icon class="fs-loading__spin" :size="26"><Loading /></el-icon>
            <span class="fs-loading__text">加载中...</span>
          </div>
        </Transition>
      </div>
    </Transition>

    <!-- Status Bar（传输总进度：透明绿铺满背景；右侧详情按钮弹出传输队列） -->
    <div v-show="!collapsed" class="fs-status relative shrink-0 h-6 px-3 flex items-center justify-between text-[10px] text-[var(--jc-text-muted)] border-t border-[var(--jc-border)] bg-[var(--jc-bg-soft)] overflow-hidden">
      <div
        v-if="transferSummary.count > 0"
        class="absolute inset-y-0 left-0 bg-green-500/25 transition-[width] duration-200 pointer-events-none"
        :class="{ 'animate-pulse': transferSummary.percent < 0 }"
        :style="{ width: transferSummary.percent >= 0 ? transferSummary.percent + '%' : '100%' }"
      ></div>
      <span v-if="loading" class="relative z-10">加载中...</span>
      <span v-else-if="loadError" class="relative z-10 text-red-400">加载失败: {{ loadError }}</span>
      <!-- 移动端这一行由底部面包屑取代，隐藏路径文字（加载中 / 失败提示仍保留） -->
      <span v-else class="fs-status__path relative z-10">{{ currentPath }} · {{ currentChildren.length }} 项<span v-if="selectedPaths.size > 0"> · 已选 {{ selectedPaths.size }} 项</span></span>
      <div class="relative z-10 flex items-center gap-1.5">
        <span v-if="selectedItem">{{ formatSize(selectedItem.size) }} · {{ selectedItem.permissions }}</span>
        <!-- 传输状态文字 + 详情图标作为整体可点击：切换传输队列弹窗 -->
        <button
          class="transfer-toggle flex items-center gap-1.5 rounded px-1 h-5 transition-colors"
          :class="sessionTransfers.length ? 'cursor-pointer hover:bg-white/10' : 'cursor-default opacity-60'"
          :disabled="!sessionTransfers.length"
          title="传输队列"
          @click="sessionTransfers.length && (showQueue = !showQueue)"
        >
          <span v-if="transferSummary.count > 0" class="text-green-400">
            {{ transferSummary.count }} 个传输中{{ transferSummary.percent >= 0 ? ' ' + transferSummary.percent + '%' : '' }}
          </span>
          <el-icon size="12" :class="sessionTransfers.length ? 'text-gray-400' : 'text-gray-600'"><Files /></el-icon>
        </button>
      </div>
    </div>

    <!-- 传输队列弹窗：覆盖 fs 内容区，自顶向下滑出 -->
    <TransferQueuePanel
      v-if="showQueue"
      :session-id="sessionId"
      :style="queuePanelStyle"
      @close="onLocalQueueClose"
      @open-dir="openTransferDir"
    />

    <!-- 右键菜单 -->
    <FsContextMenu
      v-if="ctxMenu"
      :x="ctxMenu.x"
      :y="ctxMenu.y"
      :items="ctxMenuItems"
      @select="onMenuSelect"
      @close="ctxMenu = null"
    />

    <!-- 权限编辑对话框 -->
    <FilePermissionDialog
      v-if="permTarget"
      v-model="permDialogVisible"
      :session-id="sessionId"
      :path="permTarget.path"
      :permissions="permTarget.permissions"
      :is-dir="permTarget.type === 'dir'"
      @saved="refreshCurrentDir"
    />
  </div>
</template>

<script setup lang="ts">
/**
 * 文件系统浏览器
 * 作者：JC
 * 统一目录树状态（rootChildren + pathStack），列表/分栏/树形/图标四视图共享同一份数据；
 * 与终端双向联动：fs 导航 → 发送 cd 到终端；终端 cd → 通过 /proc/<pid>/cwd 感知并同步 fs。
 */
import { ref, computed, watch, nextTick, h, defineComponent, onMounted, onBeforeUnmount, onActivated, onDeactivated } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { ElMessage, ElMessageBox } from 'element-plus'
import { handleApiError, friendlyError, friendlyText } from '@/utils/errorHandler'
import { isUiFrozen, onUiThaw } from '@/utils/uiFreeze'
import PanelHeader from '@/components/common/PanelHeader.vue'
import AdaptiveToolbar from '@/components/common/AdaptiveToolbar.vue'
import type { ToolbarButton } from '@/components/common/AdaptiveToolbar.vue'
import { ArrowLeft, ArrowRight, Top, Connection, FolderOpened, Files, Refresh, DocumentAdd, Plus, Minus, Delete, Upload } from '@element-plus/icons-vue'
import { Folder, Document, Link, Download, EditPen, CopyDocument, Lock, Box, FolderAdd, Loading } from '@element-plus/icons-vue'
import { formatSize } from '@/utils/format'
import { isMobileWebview } from '@/utils/platform'
import FsTreeNode from './FsTreeNode.vue'
import TransferQueuePanel from './TransferQueuePanel.vue'
import FsContextMenu, { type FsMenuItem } from './FsContextMenu.vue'
import FilePermissionDialog from './FilePermissionDialog.vue'
import { useEditorStore } from '@/stores/editor'
import { copyText } from '@/utils/clipboard'
import { iconColor, type FileNode } from './fs-types'
import type { Dock } from '@/stores/layout'
import { useTransferStore, type TransferItem } from '@/stores/transfer'
import {
  setRemoteFileDrag, resetRemoteFileDrag, getRemoteFilePayload, transferRemoteFile,
  registerOsDropZone, unregisterOsDropZone, uploadFileBytes
} from '@/composables/useFileDrag'
import { type FsSyncPayload, type FsToolbarSync } from '@/composables/useTermSync'

interface Props {
  connected?: boolean
  hostname?: string
  sessionId?: string
  /** 会话类型：local 走本机文件读取（list_local_dir），ssh 走 ls -la */
  kind?: 'local' | 'ssh'
  /** 所在 dock 方向（决定头部按钮排布） */
  dock?: Dock
  /** 是否折叠 */
  collapsed?: boolean
  /** 是否显示拖拽手柄（分屏单元内嵌时不显示，避免误触发全局面板移动） */
  grip?: boolean
  /** 隐藏头部工具条（分屏内节省空间，Alt 切换） */
  hideHeader?: boolean
  /** 隐藏「联动终端」按钮（FTP 模式为独立 SFTP 文件管理器，无需联动终端） */
  hideSync?: boolean
  /** 工具条 Teleport 目标选择器：提供时工具条渲染到外部容器（如 FTP 分屏窗格头） */
  toolbarTeleport?: string
  /** 隐藏头部折叠/展开按钮（FTP 分屏内嵌时无意义） */
  hideCollapse?: boolean
}

const props = withDefaults(defineProps<Props>(), { connected: false, hostname: '', sessionId: '', kind: 'ssh', dock: 'bottom', collapsed: false, grip: true, hideHeader: false, hideSync: false, toolbarTeleport: '', hideCollapse: false })
const emit = defineEmits<{
  (e: 'collapse'): void
  (e: 'fs-scroll', sx: number, sy: number): void
  (e: 'fs-toolbar', payload: FsToolbarSync): void
}>()

/** 是否移动端：工具栏按钮集合、行点击语义、列宽默认值等都按平台分流 */
const isMobile = isMobileWebview()
/** 视图模式：移动端恒为列表视图（视图切换按钮已按需求移除，
 *  「单击打开 / 长按菜单」也只在列表行上实现） */
const viewMode = ref<'list' | 'column' | 'tree' | 'icon'>('list')
const sortField = ref<'name' | 'size' | 'modified'>('name')
const sortAsc = ref(true)
const loading = ref(false)
const loadError = ref('')
/** 全屏 loading 覆盖层状态：仅「显式加载」（初始 / 导航 / 手动刷新）时显示；静默自动刷新不触发。
 *  直接在加载开始/结束时同步增删计数，而不是用 watch(loading) 驱动 ——
 *  本地或已缓存目录的列表常在同一个 tick 内 true→false，watcher 只会看到最终的 false，
 *  过渡根本不会渲染（这正是「只有第一次启动 / 只有慢加载才看得到 loading」的原因）。 */
const overlayLoading = ref(false)
/** 可见加载的最短展示时长：即使列表 1ms 返回，也让过渡完整播一遍 */
const OVERLAY_MIN_MS = 350
let overlayCount = 0
let overlayFirstShownAt = 0
let overlayHideTimer: ReturnType<typeof setTimeout> | null = null
/** 组件是否处于可见（被 keep-alive 激活）状态 */
let pageVisible = true
/** 不可见期间发生并完成的可见加载：等切回可见时补播一次 loading */
let overlayReplayPending = false
/** 开始一次「可见加载」：计数累加，并取消待执行的收起定时器 */
function beginOverlayLoading() {
  if (overlayHideTimer) { clearTimeout(overlayHideTimer); overlayHideTimer = null }
  if (overlayCount === 0) overlayFirstShownAt = Date.now()
  overlayCount++
  overlayLoading.value = true
}
/** 结束一次「可见加载」：全部结束后按最短展示时长收起；
 *  若结束时组件不可见，则标记待补播（keep-alive 后台加载完成时用户看不到过渡）。 */
function endOverlayLoading() {
  overlayCount = Math.max(0, overlayCount - 1)
  if (overlayCount > 0) return
  if (!pageVisible) overlayReplayPending = true
  const remain = OVERLAY_MIN_MS - (Date.now() - overlayFirstShownAt)
  if (overlayHideTimer) clearTimeout(overlayHideTimer)
  // 收起前：先等 Vue 把本次加载的列表刷进 DOM（nextTick），再等一帧确保真正绘制到屏幕，最后才淡出。
  // 否则会出现「列表数据已就绪、但还没画出来，loading 就先消失」的观感。
  overlayHideTimer = setTimeout(() => {
    nextTick(() => {
      requestAnimationFrame(() => { overlayLoading.value = false; overlayHideTimer = null })
    })
  }, Math.max(0, remain))
}
const baseTextSize = 12
const baseIconSize = 14
const ZOOM_STEP = 0.25
const ZOOM_MIN = 1
const ZOOM_MAX = 2
const ZOOM_DEFAULT = 1.25
const itemZoom = ref(ZOOM_DEFAULT)
function zoomIn() { itemZoom.value = Math.min(ZOOM_MAX, Math.round((itemZoom.value + ZOOM_STEP) * 100) / 100) }
function zoomOut() { itemZoom.value = Math.max(ZOOM_MIN, Math.round((itemZoom.value - ZOOM_STEP) * 100) / 100) }
const itemZoomLabel = computed(() => Math.round(itemZoom.value * 100) + '%')

// ===== 列表视图列定义与列宽（支持拖拽调节，持久化） =====
interface FsColumn { key: 'name' | 'size' | 'modified' | 'permissions' | 'owner'; label: string; defaultWidth: number; minWidth: number; align?: 'right'; sortField?: 'name' | 'size' | 'modified' }
const FS_COLUMNS: FsColumn[] = [
  { key: 'name', label: '名称', defaultWidth: 220, minWidth: 80, sortField: 'name' },
  { key: 'size', label: '大小', defaultWidth: 90, minWidth: 60, align: 'right', sortField: 'size' },
  { key: 'modified', label: '修改时间', defaultWidth: 150, minWidth: 90, align: 'right', sortField: 'modified' },
  { key: 'permissions', label: '权限', defaultWidth: 100, minWidth: 60 },
  { key: 'owner', label: '所有者', defaultWidth: 90, minWidth: 60 },
]
const COL_WIDTHS_KEY = 'jc_fs_col_widths'
/**
 * 移动端默认列宽：手机屏只有 ~360px，桌面默认的名称列 220px 会把首屏占掉一大半，
 * 只剩「名称 + 大小」两列可看。这里把名称列收到 132px（约占屏宽 36%，即 1/3~2/5），
 * 其余列同步收窄，于是首屏就能直接看到「名称 + 大小 + 修改时间」。
 * 移动端不支持拖动列宽（拖拽是 mousedown 实现），故不走本地持久化，固定用这套值。
 */
const MOBILE_COL_WIDTHS: Record<string, number> = { name: 132, size: 72, modified: 112, permissions: 78, owner: 78 }
const colWidths = ref<Record<string, number>>((() => {
  const def: Record<string, number> = {}
  for (const c of FS_COLUMNS) {
    def[c.key] = isMobile ? (MOBILE_COL_WIDTHS[c.key] ?? c.defaultWidth) : c.defaultWidth
  }
  if (isMobile) return def
  try {
    const saved = JSON.parse(localStorage.getItem(COL_WIDTHS_KEY) || '{}') as Record<string, number>
    for (const c of FS_COLUMNS) {
      const v = saved[c.key]
      if (typeof v === 'number' && v >= c.minWidth) def[c.key] = v
    }
  } catch { /* ignore */ }
  return def
})())
const resizingKey = ref('')
function colW(key: string): number { return Math.round((colWidths.value[key] ?? 100) * itemZoom.value) }
const tableWidth = computed(() => FS_COLUMNS.reduce((sum, c) => sum + colW(c.key), 0))
function startColResize(key: string, e: MouseEvent) {
  resizingKey.value = key
  const startX = e.clientX
  const startW = colWidths.value[key] ?? 100
  const min = FS_COLUMNS.find(c => c.key === key)?.minWidth ?? 60
  const onMove = (ev: MouseEvent) => {
    colWidths.value[key] = Math.max(min, Math.round((startW + (ev.clientX - startX) / itemZoom.value)))
  }
  const onUp = () => {
    document.removeEventListener('mousemove', onMove)
    document.removeEventListener('mouseup', onUp)
    resizingKey.value = ''
    try { localStorage.setItem(COL_WIDTHS_KEY, JSON.stringify(colWidths.value)) } catch { /* ignore */ }
  }
  document.addEventListener('mousemove', onMove)
  document.addEventListener('mouseup', onUp)
}

// ===== 统一目录树状态 =====
/** 根目录（逻辑根）的直接子节点；懒加载每个 dir 的 children
 *  - SSH：pathStack 空 -> 物理 / 内容，rootChildren 即 / 下一级
 *  - Local：pathStack 空 -> 驱动器列表，rootChildren 是驱动器 FileNode 映射
 *  - 通用：其他 pathStack 级数逐级懒加载
 */
const rootChildren = ref<FileNode[]>([])
const rootLoaded = ref(false)
/** 本地会话：驱动器列表 + 初始家目录 + 路径分隔符（Windows \ / Unix /） */
const localRoots = ref<string[]>([])
const localRootsLoaded = ref(false)
const localHome = ref('')
const localSep = ref('/')
const isWindows = ref(false)
/** 当前路径分段：
 *  - Windows：['D:', 'Users', 'JC']；空数组 = 驱动器列表
 *  - Unix/SSH：['etc', 'nginx']；空数组 = /
 */
const pathStack = ref<string[]>([])
const historyBack = ref<string[][]>([])
const historyForward = ref<string[][]>([])
/** 选中项的完整路径（多选） */
const selectedPaths = ref(new Set<string>())
/** Shift 区间选的 anchor（Windows 资源管理器风格）
 *  - 普通单击 / Ctrl+单击时更新为被点击项
 *  - Shift+单击 / Ctrl+Shift+单击时不更新（保持独立）
 *  - 单击空白处时清空
 *  - 切换目录时清空（由 currentChildren 监听处理） */
const shiftAnchorPath = ref('')
/** 行内重命名：被命中的行 path（空 = 无重命名） */
const renamingPath = ref('')
/** 已执行过「进入重命名即自动全选」的目标 path：重命名结束后清空，
 *  保证输入过程中 ref 回调若被重复触发也不会再次全选，打断用户输入 */
const renameSelectAppliedFor = ref('')
/** 重命名输入框的初始值（用于取消时回退） */
const renamingOriginalName = ref('')
/** 行内重命名 input DOM（仅列表视图用） */
const renameInputRef = ref<HTMLInputElement | null>(null)
/** 面包屑溢出时在右侧显示洞口吸入光效 */
const breadcrumbRef = ref<HTMLElement | null>(null)
const breadcrumbOverflow = ref(false)
let breadcrumbObserver: ResizeObserver | null = null
function checkBreadcrumbOverflow() {
  const el = breadcrumbRef.value
  if (!el) { breadcrumbOverflow.value = false; return }
  breadcrumbOverflow.value = el.scrollWidth > el.clientWidth + 1
}
/** 防止新建按钮被快速双击/快捷键重复触发导致创建多个同名项 */
const creatingInline = ref(false)
/** 防止 Enter 提交 + blur 提交 + 导航 finish 并发导致重命名/删除执行两次 */
const committingRename = ref(false)

/** 本地分段 -> 绝对路径
 *  - Windows 盘符（D:）→ 拼回 D:\...；其余段接在盘符后
 *  - Unix（Mac/Linux）→ 第一段视为根下的目录，前面补 /；
 *    旧版把所有分段当作"盘符 + 子目录"拼接，会把 ['Users','jc'] 拼成 'Usersjc'，导致 list_local_dir 失败
 */
function joinSegmentsLocal(segs: string[]): string {
  if (segs.length === 0) return ''
  const [first, ...rest] = segs
  if (first.endsWith(':')) {
    const drive = first + localSep.value
    if (rest.length === 0) return drive
    return rest.reduce((p, seg) => p + seg + localSep.value, drive).replace(new RegExp(`\\${localSep.value}$`), '')
  }
  return localSep.value + segs.join(localSep.value)
}
/** 本地绝对路径 -> 分段 */
function splitPathLocal(abs: string): string[] {
  if (!abs) return []
  // 剥离 Windows 长路径前缀（\\?\ 或 \\.\），避免 sysinfo 返回带前缀的 cwd 解析出错
  let s = abs
  if (s.startsWith('\\\\?\\')) s = s.slice(4)
  else if (s.startsWith('\\\\.\\')) s = s.slice(4)
  const norm = s.replace(/\\/g, '/').replace(/\/+$/, '')
  if (!norm) return []
  const parts = norm.split('/').filter(p => p)
  if (parts.length > 0 && parts[0].endsWith(':')) return parts
  return parts
}
/** SSH 绝对路径 -> 分段 */
function splitPathSsh(abs: string): string[] {
  const trimmed = abs.replace(/\/+$/, '')
  if (trimmed === '' || trimmed === '/') return []
  return trimmed.split('/').filter(p => p)
}
/** 本地路径拼接（对单个父/子） */
function joinLocal(parent: string, name: string): string {
  const sep = localSep.value || (isWindows.value ? '\\' : '/')
  return parent.endsWith(sep) ? parent + name : parent + sep + name
}

/** currentPath：展示 & 用于发送 cd 命令 */
const currentPath = computed(() => {
  if (pathStack.value.length === 0) {
    // Windows：此电脑是盘符列表的逻辑视图；Unix（macOS/Linux）只有 '/' 一个根，此电脑即 /
    if (props.kind === 'local') return isWindows.value ? (localRoots.value.length ? '此电脑' : '…') : '/'
    return '/'
  }
  if (props.kind === 'local') return joinSegmentsLocal(pathStack.value)
  return '/' + pathStack.value.join('/')
})

/** 逻辑根的驱动器 FileNode 映射 */
const mappedRootChildren = computed<FileNode[]>(() => {
  return localRoots.value.map(drive => {
    const d = drive.replace(/\\$/, '').replace(/\/$/, '')
    const sep = isWindows.value ? '\\' : '/'
    return {
      name: d, type: 'dir', size: 0, modified: '', permissions: '', owner: '', group: '',
      path: d + sep, expanded: false, loaded: false, children: undefined
    }
  })
})

/** 当前路径对应的树节点 */
const currentNodeInTree = computed<FileNode | null>(() => {
  let children: FileNode[] = rootChildren.value
  let node: FileNode | null = null
  for (const seg of pathStack.value) {
    const next = children.find(c => c.name === seg && c.type === 'dir')
    if (!next) return null
    node = next
    children = next.children ?? []
  }
  return node
})

/** 当前目录直接子节点（pathStack 空用 rootChildren；本地空再映射） */
const currentChildren = computed<FileNode[]>(() => {
  if (pathStack.value.length === 0) {
    // Windows：此电脑 = 驱动器列表；Unix（macOS/Linux）：此电脑即 / 本身，直接展示根目录内容
    if (props.kind === 'local') return isWindows.value ? mappedRootChildren.value : rootChildren.value
    return rootChildren.value
  }
  return currentNodeInTree.value?.children ?? []
})

const sortedChildren = computed(() => {
  const arr = [...currentChildren.value]
  arr.sort((a, b) => {
    if (a.type === 'dir' && b.type !== 'dir') return -1
    if (a.type !== 'dir' && b.type === 'dir') return 1
    let cmp = 0
    if (sortField.value === 'name') cmp = a.name.localeCompare(b.name)
    else if (sortField.value === 'size') cmp = a.size - b.size
    else cmp = a.modified.localeCompare(b.modified)
    return sortAsc.value ? cmp : -cmp
  })
  return arr
})

const pathSegments = computed(() => {
  if (props.kind === 'local') {
    if (pathStack.value.length === 0) return ['此电脑']
    return ['此电脑', ...pathStack.value]
  }
  const segs = ['/']
  for (const p of pathStack.value) segs.push(p)
  return segs
})

// ===== 地址栏：Windows 风格路径编辑模式 =====
/** 是否处于路径文本框编辑态（点击面包屑末端空白处进入） */
const pathEditing = ref(false)
const pathEditText = ref('')
const pathEditInputRef = ref<HTMLInputElement | null>(null)
/** 已提交导航标记：Enter 提交后元素随之移除会触发 blur，需跳过"取消"逻辑 */
let pathEditCommitted = false

/** 进入路径编辑态：预填完整绝对路径并全选 */
function startPathEdit(): void {
  // "此电脑"是逻辑视图，没有可编辑的绝对路径
  if (props.kind === 'local' && pathStack.value.length === 0) return
  pathEditCommitted = false
  const p = currentPath.value
  const sep = props.kind === 'local' ? (localSep.value || (isWindows.value ? '\\' : '/')) : '/'
  pathEditText.value = p.endsWith(sep) ? p : p + sep
  pathEditing.value = true
  nextTick(() => {
    const el = pathEditInputRef.value
    if (!el) return
    el.focus()
    el.select()
  })
}

/** 面包屑箭头点击：仅末段箭头进入编辑（与空白处同一触发区），中间箭头不响应 */
function onBreadcrumbArrowClick(idx: number): void {
  if (idx === pathSegments.value.length - 1) startPathEdit()
}

/** 提交编辑：按绝对路径导航（入历史栈并同步终端） */
async function commitPathEdit(): Promise<void> {
  pathEditCommitted = true
  const raw = pathEditText.value.trim()
  pathEditing.value = false
  if (!raw) return
  historyBack.value.push([...pathStack.value])
  historyForward.value.length = 0
  await navigateToPath(raw)
  syncToTerminal(currentPath.value)
}

/** 移动端：面包屑右侧一键复制当前路径（「此电脑」逻辑视图无路径时忽略） */
function copyCurrentPath(): void {
  if (props.kind === 'local' && pathStack.value.length === 0) return
  copyText(currentPath.value)
    .then(() => ElMessage.success('已复制路径'))
    .catch(() => { /* 剪贴板写入失败忽略 */ })
}

/** Esc / 失焦：放弃编辑，还原为面包屑 */
function cancelPathEdit(): void {
  if (pathEditCommitted) return
  pathEditing.value = false
}
function onPathEditBlur(): void {
  cancelPathEdit()
}

const selectedItem = computed(() => currentChildren.value.find(c => selectedPaths.value.has(c.path)) ?? null)

/** 当前路径链上所有已打开目录的绝对路径（用于树形视图高亮：最里层高亮，父级低亮） */
const activeDirPaths = computed<string[]>(() => {
  if (pathStack.value.length === 0) return []
  if (props.kind === 'local') {
    const paths: string[] = []
    for (let i = 0; i < pathStack.value.length; i++) {
      paths.push(joinSegmentsLocal(pathStack.value.slice(0, i + 1)))
    }
    return paths
  }
  const paths: string[] = []
  let acc = ''
  for (let i = 0; i < pathStack.value.length; i++) {
    acc = acc + '/' + pathStack.value[i]
    paths.push(acc)
  }
  return paths
})

// ===== 数据加载 =====
/** 合法 `ls -l` 权限串：类型位 + 9 位权限位（可带 ACL 后缀 . + @）。
 *  必须严格校验，否则 `ls` 的报错文本（如 `ls: cannot access '/root': No such file or directory`
 *  以 `l` 开头、按空格切分后段数 ≥9）会被误当成 symlink 行，解析出名为 "directory" 的幽灵条目。 */
const LS_PERM_RE = /^[dlcbps-][rwxsStT-]{9}[.+@]?$/
function parseLsLine(line: string, parentPath: string): FileNode | null {
  if (!line) return null
  const parts = line.split(/\s+/)
  if (parts.length < 9) return null
  const permissions = parts[0]
  if (!LS_PERM_RE.test(permissions)) return null
  const typeChar = permissions[0]
  const type: FileNode['type'] = typeChar === 'd' ? 'dir' : typeChar === 'l' ? 'symlink' : 'file'
  const size = parseInt(parts[4]) || 0
  const name = parts.slice(8).join(' ')
  if (!name) return null
  const month = parts[5] || ''
  const day = parts[6] || ''
  const timeOrYear = parts[7] || ''
  const modified = `${month} ${day} ${timeOrYear}`
  const childPath = parentPath === '/' ? '/' + name : parentPath + '/' + name
  return {
    name, type, size, modified,
    permissions, owner: parts[2] || '', group: parts[3] || '',
    path: childPath, expanded: false, loaded: false
  }
}

/** 判断 SFTP 列目录错误是否「不值得重试」：
 *  - 路径不存在 / 无权限：重试无意义，直接回退；
 *  - 建 SFTP 子系统超时（后端 20s）或子系统不可用：重试会成倍放大等待，也直接回退。
 *  仅「通道刚建立、瞬时读失败」这类错误才做短重试。 */
function isPermanentFsError(e: any): boolean {
  const msg = (e?.message || String(e || '')).toLowerCase()
  return /no such file|not found|permission denied|not a directory|is a directory|超时|timeout|子系统不可用|subsystem|unsupported/.test(msg)
}

/** Rust 侧 list_remote_dir 返回的结构化目录项（SFTP read_dir，非 ls 文本） */
interface RemoteDirEntry {
  name: string
  entry_type: 'dir' | 'file' | 'symlink'
  size: number
  modified: string
  permissions: string
  uid: number
  gid: number
}

/**
 * 拉取目录子项。返回 null 表示"失败"，由调用方决定是否清空 + 提示错误：
 *   - 显式导航（silent 未传）：失败 → 返回 null，同时 loadError 被设为友好提示；
 *     调用方一般 `node.children = result ?? []` 实现"清空 + 报错"。
 *   - 自动刷新（silent: true）：失败 → 返回 null，且**不**设 loadError；
 *     调用方保持旧 children + 旧错误状态，下一次 5s tick 成功时自然恢复。
 *
 * SSH 会话优先走 SFTP 的 list_remote_dir（不经 shell，不与终端 exec 通道争用，
 * 跑部署脚本/大文件上传时不再周期性失败）；SFTP 不可用再回退 exec 的 `ls -la`。
 */
async function fetchChildren(absPath: string, opts?: { silent?: boolean }): Promise<FileNode[] | null> {
  if (props.kind === 'local') return fetchChildrenLocal(absPath)
  // ① 首选：SFTP 结构化列出。
  // 会话刚建立时 SFTP 子系统可能尚未就绪（首次 list_dir 会瞬时失败）。用户正看着页面时（非静默）
  // 给一个小幅重试窗口吸收这种瞬时失败；窗口刻意保持短（≤1.25s），避免把每次加载都拖慢。
  // 静默的自动刷新不重试（后台失败由下一轮 tick 自然恢复）。
  const maxAttempts = opts?.silent ? 1 : 5
  const retryDelay = 250
  let sftpErr: any = null
  for (let attempt = 0; attempt < maxAttempts; attempt++) {
    try {
      const entries = await invoke<RemoteDirEntry[]>('list_remote_dir', {
        sessionId: props.sessionId,
        path: absPath
      })
      const children: FileNode[] = (entries || []).map(e => ({
        name: e.name,
        type: e.entry_type,
        size: e.size,
        modified: e.modified || '',
        permissions: e.permissions || '',
        owner: String(e.uid ?? ''),
        group: String(e.gid ?? ''),
        path: joinFs(absPath, e.name),
        expanded: false,
        loaded: false
      }))
      loadError.value = ''
      return children
    } catch (e: any) {
      sftpErr = e
      // 路径不存在 / 无权限 / 子系统不可用等：重试无意义，直接走兜底，避免无谓等待
      if (isPermanentFsError(e)) break
      if (attempt < maxAttempts - 1) await new Promise(r => setTimeout(r, retryDelay))
    }
  }
  // SFTP 通道不可用（服务端未开启子系统 / 超时）：继续走 exec 兜底
  console.warn('[fs] list_remote_dir 不可用，回退 ls -la:', sftpErr)
  // ② 兜底：exec 跑 ls -la 并解析文本
  try {
    const output = await invoke<string>('exec_remote_command', {
      dto: { session_id: props.sessionId, command: `ls -la "${absPath}"` }
    })
    const lines = output.split('\n').filter(l => l.trim())
    const listStartIdx = lines.findIndex(l => l.startsWith('total'))
    const fileLines = listStartIdx >= 0 ? lines.slice(listStartIdx + 1) : lines.filter(l => l.match(/^[dlcbps-]/))
    const children: FileNode[] = []
    for (const line of fileLines) {
      const node = parseLsLine(line.trim(), absPath)
      if (node && node.name !== '.' && node.name !== '..') {
        children.push(node)
      }
    }
    // 未解析出任何条目时，需区分「空目录」与「命令报错」：
    // exec 通道把 stderr 一并回传时，报错文本不能当成"空列表"，否则会显示错误内容或误导为目录为空。
    if (children.length === 0) {
      const errLine = lines.find(l =>
        /^(ls|sh|bash|find|sudo)\b[^ ]*\s*:/.test(l.trim()) ||
        /No such file or directory|Permission denied|Not a directory|command not found/i.test(l)
      )
      if (errLine) throw new Error(errLine.trim())
    }
    loadError.value = ''
    return children
  } catch (e: any) {
    console.error('[fs] fetchChildren failed:', e)
    // 自动刷新（silent）遇到瞬时失败：保留旧 children 与 loadError，不打扰用户；
    // 显式导航失败：置 loadError 并返回 null，由调用方决定是否清空 + 提示。
    if (opts?.silent) return null
    loadError.value = friendlyError(e)
    return null
  }
}

/** 本地会话：直接读本机文件系统（后端 list_local_dir） */
interface LocalEntry {
  name: string
  type: 'dir' | 'file' | 'symlink'
  size: number
  modified: string
  permissions: string
  owner: string
}

async function fetchChildrenLocal(absPath: string): Promise<FileNode[]> {
  try {
    const entries = await invoke<LocalEntry[]>('list_local_dir', { path: absPath })
    loadError.value = ''
    return entries.map(e => ({
      name: e.name, type: e.type, size: e.size, modified: e.modified,
      permissions: e.permissions, owner: e.owner, group: '',
      path: joinLocal(absPath, e.name), expanded: false, loaded: false
    }))
  } catch (e: any) {
    loadError.value = friendlyError(e)
    console.error('[fs] fetchChildrenLocal failed:', e)
    return []
  }
}

/** 确保本地逻辑根（驱动器列表 + 家目录 + 分隔符）已确定 */
async function ensureLocalRootsAndHome(): Promise<void> {
  if (localRootsLoaded.value) return
  const [roots, home] = await Promise.all([
    invoke<string[]>('list_local_roots').catch(() => []),
    invoke<string>('get_local_home_dir').catch(() => '')
  ])
  localRoots.value = roots
  localHome.value = home
  isWindows.value = home.includes('\\') || (roots.length > 0 && roots[0].includes('\\'))
  localSep.value = isWindows.value ? '\\' : '/'
  localRootsLoaded.value = true
}

/** 返回 segs 中第一个「尚未加载」的层级下标（节点不存在或 loaded=false）；全部就绪返回 segs.length */
function firstUnloadedSeg(segs: string[]): number {
  let children = rootChildren.value
  for (let i = 0; i < segs.length; i++) {
    const node = children.find(c => c.name === segs[i] && c.type === 'dir')
    if (!node || !node.loaded) return i
    children = node.children ?? []
  }
  return segs.length
}

/** 确保路径上的每一级目录 children 都已加载（导航/联动统一入口）
 *  opts.silent=true 时（自动刷新调用）：拉取失败保留旧节点、不置 loadError；
 *  opts.silent 缺省（显式导航）：拉取失败把当前节点清空 + 置 loadError（由 fetchChildren 内部完成）。 */
async function ensurePathLoaded(segs: string[], opts?: { silent?: boolean }) {
  if (!props.sessionId) return
  const silent = !!opts?.silent
  loading.value = true
  if (!silent) beginOverlayLoading()
  try {
    if (props.kind === 'local') {
      await ensureLocalRootsAndHome()
      // 仅 Windows 在此电脑层展示驱动器列表。Unix（macOS/Linux）只有 '/' 一个根，
      // 此电脑即 / 本身，交给下方 Unix 分支统一加载 / 的内容。
      // 旧版对 Unix 也走这里：mappedRootChildren 把 '/' 去尾斜杠后 name 变成空串，
      // 渲染出"无名称文件夹"，双击后 pathStack=['']，面包屑出现空段、列表空白。
      if (isWindows.value && segs.length === 0) {
        if (rootChildren.value.length === 0) {
          rootChildren.value = mappedRootChildren.value.map(n => ({ ...n, children: undefined, loaded: false }))
        }
        rootLoaded.value = true
        return
      }
      if (isWindows.value) {
        // Windows：segs[0] 是盘符（mappedRootChildren 中的节点），按原逻辑逐级加载
        if (rootChildren.value.length === 0) {
          rootChildren.value = mappedRootChildren.value.map(n => ({ ...n, children: undefined, loaded: false }))
          rootLoaded.value = true
        }
        let children = rootChildren.value
        for (let i = 0; i < segs.length; i++) {
          const seg = segs[i]
          const node = children.find(c => c.name === seg && c.type === 'dir')
          if (!node) return
          if (!node.loaded) {
            let absDir: string
            if (i === 0) absDir = seg.endsWith(':') ? seg + localSep.value : seg
            else absDir = joinSegmentsLocal(segs.slice(0, i + 1))
            const fetched = await fetchChildren(absDir, opts)
            if (fetched) { node.children = fetched; node.loaded = true }
            else if (!opts?.silent) { node.children = []; node.loaded = true }
          }
          children = node.children ?? []
        }
      } else {
        // Unix（Mac/Linux）：只有 '/' 一个根，rootChildren 直接装 / 的内容（与 SSH 一致），
        // 然后按段逐级深入；旧版把 rootChildren 设为盘符节点（Mac 上是空名 '/')，
        // 后续按段查找全部失败，文件列表为空、自动刷新拼出 'Usersjc' 触发 '[object Object]' 报错
        if (!rootLoaded.value) {
          const fetched = await fetchChildren(localSep.value, opts)
          if (fetched) {
            rootChildren.value = fetched
            rootLoaded.value = true
          } else if (!opts?.silent) {
            rootChildren.value = []
            rootLoaded.value = true
          }
          // silent 失败保留原 rootChildren，rootLoaded 保持 false 以便下次重试
        }
        let children = rootChildren.value
        for (const seg of segs) {
          const node = children.find(c => c.name === seg && c.type === 'dir')
          if (!node) return
          if (!node.loaded) {
            const fetched = await fetchChildren(node.path, opts)
            if (fetched) { node.children = fetched; node.loaded = true }
            else if (!opts?.silent) { node.children = []; node.loaded = true }
          }
          children = node.children ?? []
        }
      }
    } else {
      // SSH：各级目录的绝对路径可由 pathStack 直接算出，不依赖父级列表内容，
      // 因此并行拉取 —— 深度 N 的 cwd 由「N 次串行 SFTP 往返」降为「1 次往返的时间」。
      // 逐级串行是移动端首屏慢的主因（单次往返 100~300ms，4 级目录要 1s 上下）。
      const needRoot = !rootLoaded.value
      const start = needRoot ? 0 : firstUnloadedSeg(segs)
      if (!needRoot && start >= segs.length) return   // 各级均已加载，无需请求
      const paths: string[] = []
      if (needRoot) paths.push('/')
      for (let i = start; i < segs.length; i++) paths.push('/' + segs.slice(0, i + 1).join('/'))
      const results = await Promise.all(paths.map(p => fetchChildren(p, opts)))
      let base = 0
      if (needRoot) {
        const root = results[0]
        base = 1
        if (root) { rootChildren.value = root; rootLoaded.value = true }
        else if (!opts?.silent) { rootChildren.value = []; rootLoaded.value = true }
      }
      // 先沿「已加载」的层级下探到 start 对应的父级 children。
      // needRoot=false 时 start 可能 >0（如已在 /var，再进 /var/www 只需拉 /var/www）；
      // 若此时仍把 children 初始化成 rootChildren 并从 i=start 开始查找，就会在「根目录」里找
      // 'www' 而找不到 → 误判为「该级目录已不存在」并把 pathStack 回退到上一层（表现为
      // 「在 /var 下点任意目录，闪一下又退回 /var，进不去」）。深度≥2 必现，深度1（start=0）不受影响。
      let children = rootChildren.value
      for (let i = 0; i < start; i++) {
        const node = children.find(c => c.name === segs[i] && c.type === 'dir')
        if (!node) return
        children = node.children ?? []
      }
      for (let i = start; i < segs.length; i++) {
        const node = children.find(c => c.name === segs[i] && c.type === 'dir')
        if (!node) {
          // 远端已无该级目录（被删除/改名）：回退到上一层，避免停在空白列表让用户以为没加载完
          if (!opts?.silent && i > 0) pathStack.value = pathStack.value.slice(0, i)
          return
        }
        const fetched = results[base + (i - start)]
        if (fetched) { node.children = fetched; node.loaded = true }
        else if (!opts?.silent) { node.children = []; node.loaded = true }
        children = node.children ?? []
      }
    }
  } finally {
    loading.value = false
    if (!silent) endOverlayLoading()
  }
}

// ===== 联动（终端 <-> 文件系统） =====
// FTP 模式（hideSync）为独立 SFTP 文件管理器，默认关闭联动、不启动轮询。
// 移动端「终端与 SFTP 目录联动」要求长期开启，且联动开关按钮已去掉，故恒为 true
const syncEnabled = ref(isMobile ? true : !props.hideSync)
let syncTimer: ReturnType<typeof setInterval> | null = null
/** 防止重入：终端同步触发的路径更新不应再次回传终端 */
let suppressSync = false
/** fs → 终端发出 cd 后，等待终端生效的目标路径与时间戳（防止定时器读到旧目录把 fs 拉回） */
let pendingTermPath: string | null = null
let pendingTermAt = 0
const PENDING_TIMEOUT_MS = 6000
/** 最近一次已知的终端目录：只有终端目录真实变化时才回同步 fs（避免 fs 停留在"此电脑"被拉回） */
let lastTermCwd = ''

/** 本地路径比较归一化：剥离 Windows 长路径前缀、统一分隔符、去尾部斜杠、忽略大小写（Windows 路径不区分大小写） */
function normalizeLocalPath(p: string): string {
  let s = p
  if (s.startsWith('\\\\?\\')) s = s.slice(4)
  else if (s.startsWith('\\\\.\\')) s = s.slice(4)
  return s.replace(/\//g, '\\').replace(/\\+$/, '').toLowerCase()
}
/** 路径等价判断（本地按 Windows 规则归一化；SSH 去尾部斜杠，根按 /） */
function samePath(a: string, b: string): boolean {
  if (props.kind === 'local') return normalizeLocalPath(a) === normalizeLocalPath(b)
  const norm = (s: string) => s.replace(/\/+$/, '') || '/'
  return norm(a) === norm(b)
}

/** 构造 cd 输入：本地 PTY 用 \r（PSReadLine 只认 CR 为回车，\n 会进续行导致命令不执行）；
 *  单引号引用路径，规避 PowerShell 下 `cd "D:\"` 尾反斜杠与双引号粘连的歧义 */
function buildCdInput(path: string): string {
  if (props.kind === 'local') {
    if (isWindows.value) return `cd '${path.replace(/'/g, "''")}'\r`
    return `cd '${path.replace(/'/g, "'\\''")}'\r`
  }
  return `cd '${path.replace(/'/g, "'\\''")}'\n`
}

function toggleSync() {
  syncEnabled.value = !syncEnabled.value
  if (syncEnabled.value && props.connected && props.sessionId) {
    startSyncTimer()
  } else {
    stopSyncTimer()
  }
}

function startSyncTimer() {
  stopSyncTimer()
  if (syncEnabled.value && props.connected && props.sessionId) {
    syncTimer = setInterval(() => {
      // 窗口拖拽中跳过：SSH 往返 + 可能的目录切换重渲染会阻塞窗口移动，造成掉帧
      if (isUiFrozen()) return
      if (syncEnabled.value && props.connected && props.sessionId) {
        syncCurrentDir()
      }
    }, 3000)
  }
}

function stopSyncTimer() {
  if (syncTimer) { clearInterval(syncTimer); syncTimer = null }
}

/** 联动查询进行中（SSH 每次查询都要新建 exec 连接，避免重叠） */
let isSyncing = false

/** 检测终端当前目录（交互式 shell 的真实 cwd），仅当终端目录真实变化且与 fs 不同时回同步 */
async function syncCurrentDir() {
  if (!props.sessionId || !syncEnabled.value || suppressSync || isSyncing) return
  isSyncing = true
  try {
    const output = await invoke<string>('get_shell_cwd', { sessionId: props.sessionId })
    const cwd = output.trim()
    if (!cwd) return
    // fs 刚向终端发送过 cd：宽限期内等待终端生效，避免读到旧目录把 fs 拉回
    if (pendingTermPath) {
      if (samePath(cwd, pendingTermPath)) { pendingTermPath = null; lastTermCwd = cwd; return }
      if (Date.now() - pendingTermAt < PENDING_TIMEOUT_MS) return
      pendingTermPath = null // 超时（如目标不存在）：按终端实际目录为准
    }
    // 终端目录未变化：不动 fs（用户可能故意停留在"此电脑"等逻辑视图）
    if (lastTermCwd && samePath(cwd, lastTermCwd)) return
    lastTermCwd = cwd
    if (samePath(cwd, currentPath.value)) return
    suppressSync = true
    try {
      await navigateToPath(cwd)
    } finally {
      suppressSync = false
    }
  } catch { /* PID 未知或读取失败时静默跳过 */ } finally {
    isSyncing = false
  }
}

/** 同步路径到终端（受 syncEnabled 约束；本地逻辑根不发 cd） */
function syncToTerminal(path: string) {
  if (!props.sessionId || !syncEnabled.value) return
  if (props.kind === 'local' && pathStack.value.length === 0) return
  pendingTermPath = path
  pendingTermAt = Date.now()
  lastTermCwd = path
  invoke('send_session_input', {
    dto: { session_id: props.sessionId, data: buildCdInput(path) }
  }).catch(() => {})
}

// ===== 导航 =====
async function navigateTo(idx: number) {
  await ensureRenamingFinishedBeforeAction()
  historyBack.value.push([...pathStack.value])
  historyForward.value.length = 0
  // idx=0 是 breadcrumb 首段：此电脑（本地）/ /（SSH）→ pathStack 空；
  // 其余段与 pathStack 一一对应（segments = [首段, ...pathStack]），第 idx 段 = 前 idx 级
  if (idx === 0) pathStack.value = []
  else pathStack.value = pathStack.value.slice(0, idx)
  selectedPaths.value = new Set()
  doNavigate()
}

async function goBack() {
  await ensureRenamingFinishedBeforeAction()
  const prev = historyBack.value.pop()
  if (!prev) return
  historyForward.value.push([...pathStack.value])
  pathStack.value = prev
  selectedPaths.value = new Set()
  doNavigate()
}

async function goForward() {
  await ensureRenamingFinishedBeforeAction()
  const next = historyForward.value.pop()
  if (!next) return
  historyBack.value.push([...pathStack.value])
  pathStack.value = next
  selectedPaths.value = new Set()
  doNavigate()
}

async function goUp() {
  await ensureRenamingFinishedBeforeAction()
  if (pathStack.value.length === 0) return
  historyBack.value.push([...pathStack.value])
  historyForward.value.length = 0
  pathStack.value = pathStack.value.slice(0, -1)
  selectedPaths.value = new Set()
  doNavigate()
}

/** 统一导航入口：加载当前目录并按需同步到终端 */
async function doNavigate() {
  await ensurePathLoaded(pathStack.value)
  if (!suppressSync) syncToTerminal(currentPath.value)
}

/** 跳转到指定绝对路径（终端 → fs 联动入口，不改历史、不回传终端） */
async function navigateToPath(absPath: string) {
  await ensureRenamingFinishedBeforeAction()
  if (props.kind === 'local') pathStack.value = splitPathLocal(absPath)
  else pathStack.value = splitPathSsh(absPath)
  selectedPaths.value = new Set()
  await ensurePathLoaded(pathStack.value)
}

function selectItem(item: FileNode) {
  // Windows 资源管理器行为：右键已选中的项 → 保持全部选中；右键未选中项 → 替换为单选
  if (selectedPaths.value.has(item.path)) return
  selectedPaths.value = new Set([item.path])
  shiftAnchorPath.value = item.path
}

// ===== 文件拖拽（应用内） =====
/** 已连接且有会话时允许拖出文件；行内重命名时禁止拖拽，避免 input 文本选择被行拖拽抢占 */
const canDrag = computed(() => props.connected && !!props.sessionId && !renamingPath.value)
/** 当前悬停的放置目标文件夹路径 */
const dropTargetPath = ref('')

function onRowDragStart(item: FileNode, e: DragEvent) {
  console.log('[fs-drag] onRowDragStart', props.sessionId, props.kind, item.name, 'canDrag=', canDrag.value)
  if (!canDrag.value) return
  setRemoteFileDrag(e, {
    sessionId: props.sessionId,
    path: item.path,
    type: item.type === 'dir' ? 'dir' : 'file',
    name: item.name,
    kind: props.kind
  })
}

function onRowDragOver(item: FileNode, e: DragEvent) {
  e.preventDefault()
  if (e.dataTransfer) e.dataTransfer.dropEffect = 'move'
  // 仅目录行高亮为放置目标；文件行也允许落下（落到当前目录）
  if (item.type === 'dir') dropTargetPath.value = item.path
}

function onRowDragLeave(item: FileNode) {
  if (dropTargetPath.value === item.path) dropTargetPath.value = ''
}

/** 放入行 → 目录行落到该目录，文件行落到当前目录（跨会话即上传/下载/中转） */
async function onRowDrop(item: FileNode, e: DragEvent) {
  dropTargetPath.value = ''
  const payload = getRemoteFilePayload(e)
  if (!payload || !canDrag.value) return
  e.preventDefault()
  const dstDir = item.type === 'dir' ? item.path : currentPath.value
  try {
    const dst = await transferRemoteFile(payload, props.sessionId, dstDir, props.kind)
    ElMessage.success(`已移动到 ${dst}`)
    // 刷新目标目录内容
    if (currentPath.value === dstDir) {
      await doNavigate()
    } else if (item.type === 'dir') {
      item.children = (await fetchChildren(item.path)) ?? []
      item.loaded = true
    }
  } catch (err: any) {
    handleApiError(err, 'components/FileSystemBrowser.onItemDrop', { prefix: '移动失败' })
  }
}

/** 空白区域放置 → 当前目录。无条件放行（连未连接窗格也允许），彻底杜绝"禁止符号"；
 *  是否为可用拖拽由 drop 时的 payload/canDrag 校验，OS 拖入与应用内拖拽均可放置。 */
function onContainerDragOver(e: DragEvent) {
  console.log('[fs-drag] dragover 目标', props.sessionId, props.kind)
  e.preventDefault()
  if (e.dataTransfer) e.dataTransfer.dropEffect = 'move'
}

async function onContainerDrop(e: DragEvent) {
  const payload = getRemoteFilePayload(e)
  if (!payload || !canDrag.value) return
  // 本地逻辑根（此电脑）没有实体目录，拒绝放置
  if (props.kind === 'local' && pathStack.value.length === 0) {
    ElMessage.warning('请拖入具体的文件夹')
    return
  }
  e.preventDefault()
  try {
    const dst = await transferRemoteFile(payload, props.sessionId, currentPath.value, props.kind)
    ElMessage.success(`已移动到 ${dst}`)
    await doNavigate()
  } catch (err: any) {
    handleApiError(err, 'components/FileSystemBrowser.onContainerDrop', { prefix: '移动失败' })
  }
}

// ===== 移动端触摸交互：单击打开 / 长按呼出右键菜单 =====
/** 长按判定时长（毫秒）：按住不动超过该时长即视为长按 */
const LONG_PRESS_MS = 500
/** 手指抖动容忍范围（像素）：超出即视为滚动/滑动，取消长按 */
const LONG_PRESS_MOVE_TOLERANCE = 10
let longPressTimer: ReturnType<typeof setTimeout> | null = null
let longPressStart = { x: 0, y: 0 }
let longPressItem: FileNode | null = null
let longPressFired = false
/** 长按后的一小段时间内忽略 click（除了 preventDefault，再加一道兜底） */
let suppressClickUntil = 0

function cancelLongPress(): void {
  if (longPressTimer) {
    clearTimeout(longPressTimer)
    longPressTimer = null
  }
  longPressItem = null
}

function onRowTouchStart(item: FileNode, e: TouchEvent): void {
  if (!isMobile || e.touches.length !== 1) return
  const t = e.touches[0]
  cancelLongPress()
  longPressStart = { x: t.clientX, y: t.clientY }
  longPressItem = item
  longPressFired = false
  longPressTimer = setTimeout(() => {
    longPressTimer = null
    longPressFired = true
    suppressClickUntil = Date.now() + 400
    // 长按与桌面右键等价：先在手指位置弹出该项菜单（内部会先选中该项）
    void onItemContextMenu(item, { clientX: longPressStart.x, clientY: longPressStart.y })
  }, LONG_PRESS_MS)
}

function onRowTouchMove(e: TouchEvent): void {
  if (!longPressItem) return
  const t = e.touches[0]
  if (!t) return
  if (
    Math.abs(t.clientX - longPressStart.x) > LONG_PRESS_MOVE_TOLERANCE ||
    Math.abs(t.clientY - longPressStart.y) > LONG_PRESS_MOVE_TOLERANCE
  ) {
    cancelLongPress()
  }
}

/** 抬手：长按已触发时吞掉浏览器补发的 click（否则会立刻又命中菜单或打开该项） */
function onRowTouchEnd(e: TouchEvent): void {
  cancelLongPress()
  if (longPressFired) {
    e.preventDefault()
    longPressFired = false
  }
}

function onRowTouchCancel(): void {
  cancelLongPress()
  longPressFired = false
}

/** 行点击：移动端=单击直接打开；桌面=单击选中（Ctrl/Shift 多选/区间），detail=0 是拖拽悬停合成点击（直接进目录） */
async function onRowClick(item: FileNode, e: MouseEvent) {
  if (e.detail === 0 && item.type === 'dir') {
    openItem(item)
    return
  }
  if (isMobile) {
    // 长按刚结束：这次 click 是触摸的余波，忽略
    if (Date.now() < suppressClickUntil) return
    // 触摸端没有双击语义，改成单击即打开（文件夹进入目录，文件用内置编辑器打开）
    await openItem(item)
    return
  }
  // 点击其它项时先安全结束当前重命名（点击正在重命名的项本身则保持编辑态）
  if (renamingPath.value && renamingPath.value !== item.path) {
    await ensureRenamingFinishedBeforeAction()
  }
  handleSelectionClick(item, e)
}

/** Shift 区间选择：从 shiftAnchorPath 到 clicked 之间的所有项。
 *  Windows 行为：
 *  - 纯 Shift：区间替换，区间外选中全部丢弃
 *  - Ctrl+Shift：保留区间外选区，区间内统一切换为 clicked 项的目标状态
 *               （clicked 当前未选 → 区间内全选；clicked 当前已选 → 区间内全取消） */
function applyRangeSelection(clicked: FileNode, additive: boolean) {
  const list = currentChildren.value
  const start = list.findIndex(c => c.path === shiftAnchorPath.value)
  const end = list.findIndex(c => c.path === clicked.path)
  if (start === -1 || end === -1) return
  const [lo, hi] = start < end ? [start, end] : [end, start]
  const next = additive ? new Set(selectedPaths.value) : new Set<string>()
  const targetState = !selectedPaths.value.has(clicked.path)
  for (let i = lo; i <= hi; i++) {
    if (targetState) next.add(list[i].path)
    else next.delete(list[i].path)
  }
  selectedPaths.value = next
}

/** Windows 风格的文件项单击处理（普通/Ctrl/Shift/Ctrl+Shift） */
function handleSelectionClick(item: FileNode, e: MouseEvent) {
  // Ctrl+Shift+单击：区间切换（保留外部选区）
  if (e.ctrlKey && e.shiftKey) {
    if (!shiftAnchorPath.value || currentChildren.value.findIndex(c => c.path === shiftAnchorPath.value) === -1) {
      shiftAnchorPath.value = item.path
      selectedPaths.value = new Set([item.path])
      return
    }
    applyRangeSelection(item, true)
    return
  }
  // Shift+单击：区间替换
  if (e.shiftKey) {
    if (!shiftAnchorPath.value || currentChildren.value.findIndex(c => c.path === shiftAnchorPath.value) === -1) {
      shiftAnchorPath.value = item.path
      selectedPaths.value = new Set([item.path])
    } else {
      applyRangeSelection(item, false)
    }
    return
  }
  // Ctrl+单击：切换 + 更新 anchor
  if (e.ctrlKey || e.metaKey) {
    const next = new Set(selectedPaths.value)
    if (next.has(item.path)) next.delete(item.path)
    else next.add(item.path)
    selectedPaths.value = next
    shiftAnchorPath.value = item.path
    return
  }
  // 普通单击：单选 + 更新 anchor
  selectedPaths.value = new Set([item.path])
  shiftAnchorPath.value = item.path
}

/** 双击打开：目录进入；文件/符号链接打开文本编辑器 */
async function openItem(item: FileNode) {
  await ensureRenamingFinishedBeforeAction()
  if (item.type !== 'dir') {
    openInEditor(item)
    return
  }
  historyBack.value.push([...pathStack.value])
  historyForward.value.length = 0
  if (props.kind === 'local' && isWindows.value) {
    // Windows：此电脑层的节点是盘符，需从 name 推导（'C:' -> 'C:\'）
    if (pathStack.value.length === 0 && localRootsLoaded.value) {
      const drive = item.name.endsWith(':') ? item.name : item.name.replace(/[\\/]$/, '')
      pathStack.value = [drive]
    } else {
      pathStack.value = [...pathStack.value, item.name]
    }
  } else {
    // Unix（macOS/Linux）：此电脑层已经是 / 的真实内容，直接按名称入栈
    pathStack.value = [...pathStack.value, item.name]
  }
  selectedPaths.value = new Set()
  doNavigate()
}

// ===== 右键菜单（打开/下载/重命名/删除/复制路径/编辑权限/压缩解压） =====
const editorStore = useEditorStore()
const ctxMenu = ref<{ x: number; y: number; item: FileNode } | null>(null)
/** 目标机器压缩工具检测结果（按会话缓存，首次打开菜单时检测） */
const archiveTools = ref<string[] | null>(null)
const permDialogVisible = ref(false)
const permTarget = ref<FileNode | null>(null)

/** 路径拼接（跟随会话路径风格；根目录/盘符不重复加分隔符） */
function joinFs(dir: string, name: string): string {
  const sep = props.kind === 'local' ? (localSep.value || '\\') : '/'
  const d = dir.replace(/[\\/]+$/, '')
  // 根目录必须保留前导分隔符：dir='/' 时直接返回 name 会退化成相对路径
  // （'home' 而非 '/home'），SFTP 按会话家目录解析 → 列错目录、深层节点找不到、列表空白
  if (!d) return sep + name
  return d + sep + name
}

/** 去掉最后一个扩展名（压缩输出名：测试文件A.txt → 测试文件A） */
function baseNameNoExt(name: string): string {
  const idx = name.lastIndexOf('.')
  return idx > 0 ? name.slice(0, idx) : name
}

/** 压缩包扩展名（长后缀在前，保证 stripArchiveExt 优先匹配复合扩展；含 ZIP 系衍生格式 jar/war/ear/apk） */
const ARCHIVE_EXTS = ['.tar.gz', '.tar.bz2', '.tar.xz', '.tar.zst', '.tgz', '.tbz2', '.txz', '.tzst', '.tar', '.zip', '.jar', '.war', '.ear', '.apk', '.7z', '.rar', '.gz', '.xz', '.bz2', '.zst']
/** 是否压缩包（决定"解压"菜单可用性） */
function isArchiveName(name: string): boolean {
  const lower = name.toLowerCase()
  return ARCHIVE_EXTS.some(e => lower.endsWith(e))
}
/** 去掉压缩包扩展名（含 .tar.gz 复合扩展）→ 解压目标文件夹名 */
function stripArchiveExt(name: string): string {
  const lower = name.toLowerCase()
  for (const e of ARCHIVE_EXTS) {
    if (lower.endsWith(e)) return name.slice(0, name.length - e.length)
  }
  return baseNameNoExt(name)
}

async function ensureArchiveTools(): Promise<string[]> {
  if (archiveTools.value) return archiveTools.value
  try {
    archiveTools.value = await invoke<string[]>('detect_archive_tools', { sessionId: props.sessionId })
  } catch {
    archiveTools.value = []
  }
  return archiveTools.value
}

function hasTool(name: string): boolean {
  const tools = archiveTools.value ?? []
  return tools.some(t => t === name || t.endsWith('/' + name) || t.endsWith('\\' + name) || t.endsWith('\\' + name + '.exe'))
}

/** 压缩格式可用性（tar 系假定目标机自带 tar；zip/7z/rar/zstd 按工具检测；Windows 本地 zip 走 PowerShell 兜底） */
function compressFormats(item: FileNode): FsMenuItem[] {
  const win = props.kind === 'local' && isWindows.value
  const has7z = hasTool('7z') || hasTool('7za')
  const base = item.type === 'dir' ? item.name : baseNameNoExt(item.name)
  const defs: { fmt: string; verb: string; ok: boolean; hint?: string }[] = [
    { fmt: 'tar', verb: '打包为', ok: true },
    { fmt: 'tar.gz', verb: '压缩为', ok: true },
    { fmt: 'tar.bz2', verb: '压缩为', ok: win || hasTool('bzip2') },
    { fmt: 'tar.xz', verb: '压缩为', ok: win || hasTool('xz') },
    { fmt: 'tar.zst', verb: '压缩为', ok: win || hasTool('zstd') },
    { fmt: 'zip', verb: '压缩为', ok: win || hasTool('zip') || has7z },
    { fmt: '7z', verb: '压缩为', ok: has7z, hint: '需安装 7-Zip' },
    { fmt: 'rar', verb: '压缩为', ok: hasTool('rar'), hint: '需安装 WinRAR 命令行' },
  ]
  return defs.map(d => ({
    key: 'compress:' + d.fmt,
    label: `${d.verb} "${base}.${d.fmt}"${d.ok || !d.hint ? '' : `（${d.hint}）`}`,
    disabled: !d.ok
  }))
}

function buildMenuItems(item: FileNode): FsMenuItem[] {
  const isArc = item.type !== 'dir' && isArchiveName(item.name)
  const win = props.kind === 'local' && isWindows.value
  return [
    { key: 'open', label: item.type === 'dir' ? '打开' : '打开（文本编辑器）', icon: FolderOpened },
    { key: 'download', label: '下载', icon: Download },
    { key: 'rename', label: '重命名', icon: EditPen },
    { key: 'delete', label: '删除', icon: Delete, danger: true },
    { key: 'copyDir', label: '复制所在路径', icon: CopyDocument },
    { key: 'copyPath', label: '复制文件路径', icon: CopyDocument },
    { key: 'chmod', label: '编辑权限', icon: Lock, disabled: win },
    {
      key: 'compress', label: '压缩', icon: Box,
      children: compressFormats(item)
    },
    {
      key: 'extract', label: '解压', icon: FolderAdd, disabled: !isArc,
      children: isArc ? [
        { key: 'extractHere', label: '直接解压' },
        { key: 'extractTo', label: `解压到 "${stripArchiveExt(item.name)}"` },
      ] : []
    },
  ]
}

const ctxMenuItems = computed<FsMenuItem[]>(() => ctxMenu.value ? buildMenuItems(ctxMenu.value.item) : [])

/** 弹出某项的右键菜单。pos 只用到坐标，故鼠标事件与触摸坐标都可直接传入 */
async function onItemContextMenu(item: FileNode, pos: { clientX: number; clientY: number }) {
  if (!props.connected || !props.sessionId) return
  selectItem(item)
  // 先检测压缩工具再弹菜单（结果按会话缓存，仅首次有延迟）
  await ensureArchiveTools()
  ctxMenu.value = { x: pos.clientX, y: pos.clientY, item }
}

function onTreeContextMenu(node: FileNode, ev: MouseEvent) {
  onItemContextMenu(node, ev)
}

function onMenuSelect(key: string) {
  const item = ctxMenu.value?.item
  if (!item) return
  if (key.startsWith('compress:')) { menuCompress(item, key.slice('compress:'.length)); return }
  switch (key) {
    case 'open': openItem(item); break
    case 'download': menuDownload(item); break
    case 'rename': menuRename(item); break
    case 'delete': deletePaths(selectedPaths.value.size > 0 ? Array.from(selectedPaths.value) : [item.path]); break
    case 'copyDir': copyText(parentDir(item.path)).then(() => ElMessage.success('已复制所在路径')); break
    case 'copyPath': copyText(item.path).then(() => ElMessage.success('已复制文件路径')); break
    case 'chmod': permTarget.value = item; permDialogVisible.value = true; break
    case 'extractHere': menuExtract(item, false); break
    case 'extractTo': menuExtract(item, true); break
  }
}

/** 强制刷新当前目录（文件操作后调用） */
async function refreshCurrentDir() {
  if (pathStack.value.length === 0) {
    if (props.kind === 'local') return
    rootLoaded.value = false
  } else {
    const node = currentNodeInTree.value
    if (node) { node.loaded = false; node.children = undefined }
  }
  await ensurePathLoaded(pathStack.value)
  fingerprintCurrentDir()
}

// ===== 自动刷新：定时拉取当前目录，对比指纹有变化才更新 UI =====
const AUTO_REFRESH_MS = 5000
const autoRefreshEnabled = ref(true)
let autoRefreshTimer: ReturnType<typeof setInterval> | null = null
/** 当前目录指纹（路径 + 子项数 + 最大 mtime + 总大小 + 名字集合），用于检测外部变更 */
let lastFingerprint = ''

/** 计算当前目录指纹；本地逻辑根（驱动器列表）跳过 */
function computeFingerprint(): string {
  if (pathStack.value.length === 0) {
    if (props.kind === 'local') return 'local-roots:' + localRoots.value.join('|')
    return 'ssh-root:' + rootChildren.value.length + ':' + rootChildren.value.reduce((s, n) => s + n.size, 0)
  }
  const children = currentChildren.value
  if (!children || children.length === 0) return pathStack.value.join('/') + ':0:0:0:'
  const names = children.map(c => c.name + ':' + c.type).sort().join('|')
  const totalSize = children.reduce((s, c) => s + (c.size || 0), 0)
  const maxMtime = children.reduce((m, c) => c.modified && c.modified > m ? c.modified : m, '')
  return pathStack.value.join('/') + ':' + children.length + ':' + totalSize + ':' + maxMtime + ':' + names
}

/** 把当前快照写入指纹（用于开启自动刷新后第一帧不误判） */
function fingerprintCurrentDir() {
  lastFingerprint = computeFingerprint()
}

function startAutoRefresh() {
  stopAutoRefresh()
  if (!props.connected || !props.sessionId) return
  fingerprintCurrentDir()
  autoRefreshTimer = setInterval(async () => {
    // 窗口拖拽中跳过：整表重渲染会阻塞窗口移动，造成掉帧
    if (isUiFrozen()) return
    if (!props.connected || !props.sessionId || loading.value) return
    await doAutoRefresh()
  }, AUTO_REFRESH_MS)
}

function stopAutoRefresh() {
  if (autoRefreshTimer) { clearInterval(autoRefreshTimer); autoRefreshTimer = null }
}

/** 拉取一次并对比指纹；变化时刷新 UI（保留选中状态） */
async function doAutoRefresh() {
  // 行内重命名中不刷新：避免重建 DOM 导致 input 失焦/提交/占位被误删
  if (renamingPath.value) return
  if (pathStack.value.length === 0) {
    if (props.kind === 'local') {
      if (!isWindows.value) {
        // Unix（macOS/Linux）：此电脑 = /，直接刷新根目录内容
        await ensurePathLoaded(pathStack.value, { silent: true })
        return
      }
      // Windows：重新探测驱动器列表（U 盘插拔）
      const oldRoots = localRoots.value.join('|')
      localRootsLoaded.value = false
      await ensureLocalRootsAndHome()
      const newRoots = localRoots.value.join('|')
      if (oldRoots !== newRoots) {
        rootChildren.value = mappedRootChildren.value.map(n => ({ ...n, children: undefined, loaded: false }))
        rootLoaded.value = true
      }
    } else {
      // 根目录列表也走 silent：自动刷新失败保留旧 rootChildren，不再清空 + 报错
      await ensurePathLoaded(pathStack.value, { silent: true })
    }
    return
  }
  // 拉取当前目录（不破坏选中状态）
  let fresh: FileNode[] | null
  if (props.kind === 'local') {
    const abs = joinSegmentsLocal(pathStack.value)
    fresh = await fetchChildrenLocal(abs)
  } else {
    // silent=true：自动刷新瞬时失败（exec 通道被终端/sftp 占用）时保留旧列表 + 不弹错误，
    // 避免 deploy.sh / 大文件上传期间每 5 秒闪一次"加载失败"红字 + 清空文件列表。
    fresh = await fetchChildren('/' + pathStack.value.join('/'), { silent: true })
  }
  if (!fresh) return  // 自动刷新失败：保留旧 children 与 loadError，不打扰用户
  const sig = pathStack.value.join('/') + ':' + fresh.length + ':' + fresh.reduce((s, c) => s + (c.size || 0), 0) + ':' + fresh.reduce((m, c) => c.modified && c.modified > m ? c.modified : m, '') + ':' + fresh.map(c => c.name + ':' + c.type).sort().join('|')
  if (sig === lastFingerprint) return
  lastFingerprint = sig
  // 写回树节点；列表/分栏/图标 视图通过 currentChildren 派生自动更新
  const node = currentNodeInTree.value
  if (node) {
    // 保留 sub-nodes 的 loaded/expanded 状态
    const oldMap = new Map<string, FileNode>()
    for (const c of node.children ?? []) oldMap.set(c.name + ':' + c.type, c)
    for (const f of fresh) {
      const o = oldMap.get(f.name + ':' + f.type)
      if (o) { f.loaded = o.loaded; f.expanded = o.expanded; f.children = o.children }
    }
    node.children = fresh
    node.loaded = true
  }
  if (props.kind === 'local' && pathStack.value.length === 0) {
    rootChildren.value = fresh
  }
}

/** 手动刷新：当前目录强制重载 + 重置指纹 */
async function manualRefresh() {
  if (renamingPath.value) {
    ElMessage.warning('请先完成重命名后再刷新')
    return
  }
  await refreshCurrentDir()
}

/** 打开：文件进入文本编辑器（本地编辑远程文件） */
async function openInEditor(item: FileNode) {
  if (!props.sessionId) return
  const err = await editorStore.open(props.sessionId, props.kind, item.path, item.name)
  if (err) handleApiError(err, 'components/FileSystemBrowser.openInEditor', { prefix: '打开失败' })
}

/** 下载：SSH → 下载到所选文件夹（带进度/可取消）；本地会话 → 复制到所选文件夹 */
async function menuDownload(item: FileNode) {
  const dir = await invoke<string | null>('pick_folder_path', { title: `下载 "${item.name}" 到…` }).catch(() => null)
  if (!dir) return
  const sep = dir.includes('\\') ? '\\' : '/'
  const localPath = dir.replace(/[\\/]+$/, '') + sep + item.name
  const transferId = 'tf-' + Date.now() + '-' + Math.random().toString(36).slice(2, 6)
  const store = useTransferStore()
  store.begin({ id: transferId, name: item.name, direction: 'download', ownerSessionId: props.sessionId, dstPath: localPath })
  try {
    if (props.kind === 'local') {
      await invoke<string>('copy_local_file', { src: item.path, dstDir: dir, transferId })
    } else {
      await invoke('download_file', {
        dto: { session_id: props.sessionId, transfer_id: transferId, local_path: localPath, remote_path: item.path }
      })
    }
    ElMessage.success(`已下载到 ${localPath}`)
  } catch (e: any) {
    const msg = e?.message || String(e)
    store.markError(transferId, msg)
    if (!msg.includes('传输已取消')) {
      handleApiError(e, 'components/FileSystemBrowser.menuDownload', { prefix: '下载失败' })
    }
  }
}

/** 名称校验：拒绝空、路径分隔符、. 和 ..（防止跳出当前目录） */
function validateItemName(name: string): string | true {
  if (!name) return '名称不能为空'
  if (/[\\/]/.test(name)) return '名称不能包含路径分隔符'
  if (name === '.' || name === '..') return '名称不能为 "." 或 ".."'
  return true
}

/** 在切换目录、选中其它项等操作前，先安全结束当前重命名。
 *  返回 true 表示重命名已结束（提交成功/取消/无重命名），可以继续后续操作；
 *  返回 false 表示提交失败（如重名/非法字符），应留在当前目录让用户继续编辑。 */
async function finishRenamingBeforeAction(): Promise<boolean> {
  const rp = renamingPath.value
  if (!rp) return true
  const item = currentChildren.value.find(c => c.path === rp)
  if (!item) {
    renamingPath.value = ''
    renamingOriginalName.value = ''
    return true
  }
  await commitRename(item, renamingOriginalName.value)
  return !renamingPath.value
}

/** 强制结束当前重命名：先尝试提交；若提交失败则取消（删除占位/回退原名），保证后续操作可以继续 */
async function ensureRenamingFinishedBeforeAction(): Promise<boolean> {
  if (!renamingPath.value) return true
  const ok = await finishRenamingBeforeAction()
  if (ok) return true
  const rpItem = currentChildren.value.find(c => c.path === renamingPath.value)
  if (rpItem) await cancelRename(rpItem)
  return true
}

/** 在当前目录创建新项（内联版：不弹窗，创建后选中并进入重命名） */
async function createInline(kind: 'file' | 'folder') {
  if (creatingInline.value) return
  if (!props.connected || !props.sessionId) {
    ElMessage.warning('请先连接到主机')
    return
  }
  if (pathStack.value.length === 0 && props.kind === 'local') {
    ElMessage.warning('请先进入具体的文件夹')
    return
  }
  creatingInline.value = true
  try {
    const base = kind === 'file' ? '新建文本文件.txt' : '新建文件夹'
    // 找唯一名：base → base (1) → base (2) …；遍历当前已加载项（避免硬冲突）
    const existing = new Set(currentChildren.value.map(c => c.name))
    const ext = kind === 'file' ? '.txt' : ''
    const stem = kind === 'file' ? '新建文本文件' : '新建文件夹'
    let name = base
    for (let i = 1; existing.has(name) && i < 1000; i++) {
      name = `${stem} (${i})${ext}`
    }
    const path = joinFs(currentPath.value, name)
    if (kind === 'file') {
      await invoke('create_remote_file', { sessionId: props.sessionId, path })
    } else {
      await invoke('make_remote_dir', { sessionId: props.sessionId, path })
    }
    // 把新节点 unshift 到当前目录的 children（响应式数组），列表只多一项而不全量重载
    //   - pathStack 为空 → 改根 children
    //   - pathStack 非空 → 改 currentNodeInTree 的 children
    // 注：sortedChildren 是按名称/大小/修改时间重排的，所以 unshift 到源也只影响一项的位置
    const now = new Date().toISOString().replace('T', ' ').slice(0, 19)
    const newNode: FileNode = {
      name,
      type: kind === 'file' ? 'file' : 'dir',
      size: 0,
      modified: now,
      permissions: kind === 'file' ? '-rw-r--r--' : 'drwxr-xr-x',
      owner: '',
      group: '',
      path,
      ...(kind === 'folder' ? { children: [], loaded: false } : {})
    }
    if (pathStack.value.length === 0) {
      rootChildren.value.unshift(newNode)
    } else {
      const parent = currentNodeInTree.value
      if (parent) {
        if (!parent.children) parent.children = []
        parent.children.unshift(newNode)
      }
    }
    // 选中新项并进入重命名；ref 回调 setupRenameInput 会自动 focus + 选区
    selectedPaths.value = new Set([path])
    renamingOriginalName.value = name
    renamingPath.value = path
    // 如果新项不在视口内，滚动到视口（已在视口内不滚动，避免动画）
    // 与 setupRenameInput 的 focus 兜底分开，避免互相干扰
    //   WebView2 中 scrollIntoView({behavior:'smooth'}) 不可靠，手动算 scrollTop
    //   row 选择器兼容 list 的 <tr> 和 column 的 [data-path]
    //   container 选择器兼容 overflow-auto / overflow-y-auto / overflow-x-auto
    await nextTick()
    for (let i = 0; i < 15; i++) {
      await new Promise<void>(r => requestAnimationFrame(() => r()))
      const el = fsRootRef.value?.querySelector('input[data-rename-input]') as HTMLInputElement | null
      if (!el) continue
      const row = el.closest('tr, [data-path]') as HTMLElement | null
      const container = el.closest('.overflow-auto, .overflow-y-auto, .overflow-x-auto') as HTMLElement | null
      if (row && container) {
        const rowRect = row.getBoundingClientRect()
        const containerRect = container.getBoundingClientRect()
        if (rowRect.top < containerRect.top) {
          container.scrollTop -= (containerRect.top - rowRect.top)
        } else if (rowRect.bottom > containerRect.bottom) {
          container.scrollTop += (rowRect.bottom - containerRect.bottom)
        }
      }
      break
    }
  } catch (e: any) {
    handleApiError(e, 'components/FileSystemBrowser.commitCreate', { prefix: '创建失败' })
  } finally {
    creatingInline.value = false
  }
}

/** 从当前列表和树数据同步移除指定 path 的节点（用于取消新建占位） */
function removeChildNode(path: string) {
  if (pathStack.value.length === 0) {
    const idx = rootChildren.value.findIndex(c => c.path === path)
    if (idx >= 0) rootChildren.value.splice(idx, 1)
  } else {
    const currentNode = currentNodeInTree.value
    if (currentNode?.children) {
      const tIdx = currentNode.children.findIndex(c => c.path === path)
      if (tIdx >= 0) currentNode.children.splice(tIdx, 1)
    }
  }
}

/** 提交重命名：原名=新名视为取消（不调后端）；不同则按本地/SSH 调 mv */
async function commitRename(item: FileNode, newName: string) {
  // 防重复：Enter 提交 + blur 提交 + 导航 finish 可能并发；仅保留第一个调用
  if (committingRename.value) return
  // 捕获局部 rp：异步 await 期间 renamingPath 可能已被其他流程清空，guard 用局部值判断
  const rp = renamingPath.value
  if (rp !== item.path) return
  committingRename.value = true
  // 提前清空 renamingPath：避免 blur + Enter / blur + 兜底 onContentBgClick 并发提交
  renamingPath.value = ''
  renamingOriginalName.value = ''
  try {
    const trimmed = newName.trim()
    if (trimmed === item.name) {
      // 未变化：仅退出重命名态，保留文件/文件夹
      return
    }
    const v = validateItemName(trimmed)
    if (v !== true) {
      // 失败：恢复重命名态（保留用户已输入的内容）
      renamingPath.value = rp
      renamingOriginalName.value = trimmed
      ElMessage.warning(v)
      return
    }
    // 路径相同（仅大小写差异）按未变化处理
    if (trimmed.toLowerCase() === item.name.toLowerCase()) {
      return
    }
    // 重名检查
    if (currentChildren.value.some(c => c.path !== item.path && c.name === trimmed)) {
      // 失败：恢复重命名态
      renamingPath.value = rp
      renamingOriginalName.value = trimmed
      ElMessage.warning(`已存在同名项：${trimmed}`)
      return
    }
    const dst = joinFs(parentDir(item.path), trimmed)
    if (props.kind === 'local') {
      await invoke<string>('move_local_file', { src: item.path, dst })
    } else {
      const q = (s: string) => `'${s.replace(/'/g, "'\\''")}'`
      await invoke<string>('exec_remote_command', {
        dto: { session_id: props.sessionId, command: `mv -f -- ${q(item.path)} ${q(dst)}` }
      })
    }
    // 重命名后保持选中（指向新 path）
    selectedPaths.value = new Set([dst])
    await refreshCurrentDir()
  } catch (e: any) {
    // 失败：恢复重命名态（让用户继续编辑）
    renamingPath.value = rp
    renamingOriginalName.value = newName
    handleApiError(e, 'components/FileSystemBrowser.commitRename', { prefix: '重命名失败' })
  } finally {
    committingRename.value = false
  }
}

/** 取消重命名：若是刚创建的空项且未改名，删掉后端文件/文件夹；返回是否真清理了 */
async function maybeCleanupUnnamedNew(item: FileNode): Promise<boolean> {
  // 仅当是默认名（新建文本文件 / 新建文件夹）时才认为是"刚创建未命名"
  const isUnnamed = /^新建文本文件(\s*\(\d+\))?\.txt$/.test(item.name) || /^新建文件夹(\s*\(\d+\))?$/.test(item.name)
  if (!isUnnamed) return false
  try {
    if (props.kind === 'local') {
      await invoke('delete_local_path', { path: item.path })
    } else {
      const q = `'${item.path.replace(/'/g, "'\\''")}'`
      await invoke<string>('exec_remote_command', {
        dto: { session_id: props.sessionId, command: `rm -rf -- ${q}` }
      })
    }
    return true
  } catch { return false }
}

/** 取消重命名：Esc 触发；仅清理"刚创建未改名"项的占位 + 后端，保留现有文件 */
async function cancelRename(item: FileNode) {
  const cleaned = await maybeCleanupUnnamedNew(item)
  if (cleaned) removeChildNode(item.path)
  renamingPath.value = ''
  renamingOriginalName.value = ''
}

/** 重命名入口（右键 / 双击 F2 等）：直接进入行内重命名 */
function menuRename(item: FileNode) {
  selectedPaths.value = new Set([item.path])
  renamingPath.value = item.path
  renamingOriginalName.value = item.name
}

/** 选中当前重命名 input 的文件名部分（不含后缀） */
function focusAndSelectName() {
  const el = fsRootRef.value?.querySelector('input[data-rename-input]') as HTMLInputElement | null
  if (!el) return
  el.focus({ preventScroll: true })
  const v = el.value
  const dot = v.lastIndexOf('.')
  if (dot > 0) el.setSelectionRange(0, dot)
  else el.setSelectionRange(0, v.length)
}

// 重命名结束后清空自动全选标记，保证下一次重命名（含同一文件再次重命名）仍能自动全选
watch(renamingPath, (v) => { if (!v) renameSelectAppliedFor.value = '' })

/** 重命名 input 挂载后立刻 focus + 选中文件名主体（不含后缀）。
 *  注意：模板中必须用稳定函数引用绑定（:ref="setupRenameInput"）。若用内联箭头函数，
 *  每次输入触发重渲染都会重新执行本函数，导致用户每输入一个字符内容就被重新全选。
 *  WebView2 中 focus 后默认光标可能在末尾，下一帧再 setSelectionRange 一次确保覆盖。 */
function setupRenameInput(el: unknown) {
  const input = el as HTMLInputElement | null
  if (!input) return
  // 同一次重命名内已全选过（如视图切换导致 input 重挂载）→ 直接返回，避免打断正在进行的输入
  const target = renamingPath.value
  if (target && renameSelectAppliedFor.value === target) return
  // 等 v-model :value 应用（Vue 3 v-for+v-if 内部 ref 回调时机偶发比 :value 早），
  // 最多重试 20 帧（约 330ms），找到 el.value 非空时再 focus + 设选区
  const tryApply = (n: number) => {
    if (input.value !== '') {
      if (target) renameSelectAppliedFor.value = target
      input.focus({ preventScroll: true })
      const apply = () => {
        const v = input.value
        if (v === '') return
        const dot = v.lastIndexOf('.')
        if (dot > 0) input.setSelectionRange(0, dot)
        else input.setSelectionRange(0, v.length)
      }
      apply()
      // 下一帧再设一次：WebView2 中第一次 setSelectionRange 偶发被 focus 默认行为覆盖
      requestAnimationFrame(apply)
      return
    }
    if (n > 0) requestAnimationFrame(() => tryApply(n - 1))
  }
  tryApply(20)
}

/** 工具栏删除按钮：批量删除 selectedPaths；右键删除单走此函数兼容 */
async function deletePaths(paths: string[]) {
  if (paths.length === 0) {
    ElMessage.warning('请先选择要删除的文件或文件夹')
    return
  }
  const items = currentChildren.value.filter(c => paths.includes(c.path))
  if (items.length === 0) return
  const hasDir = items.some(i => i.type === 'dir')
  const tip = items.length === 1
    ? `确定删除${items[0].type === 'dir' ? '文件夹' : '文件'} "${items[0].name}" 吗？${hasDir ? '文件夹内所有内容将被递归删除，' : ''}此操作不可恢复。`
    : `确定删除选中的 ${items.length} 项吗？${hasDir ? '包含文件夹，其内容将被递归删除；' : ''}此操作不可恢复。`
  try {
    await ElMessageBox.confirm(tip, '删除确认', {
      type: 'warning', confirmButtonText: '删除', cancelButtonText: '取消', confirmButtonClass: 'el-button--danger'
    })
  } catch { return }
  const removedPaths: string[] = []
  for (const it of items) {
    try {
      if (props.kind === 'local') {
        await invoke('delete_local_path', { path: it.path })
      } else {
        const q = `'${it.path.replace(/'/g, "'\\''")}'`
        await invoke<string>('exec_remote_command', {
          dto: { session_id: props.sessionId, command: `rm -rf -- ${q}` }
        })
      }
      removedPaths.push(it.path)
    } catch (e: any) {
      handleApiError(e, 'components/FileSystemBrowser.menuDelete', {
        prefix: `删除 ${it.name} 失败`,
      })
    }
  }
  selectedPaths.value = new Set()
  for (const p of removedPaths) removeChildNode(p)
}

/** 右键菜单"删除"单文件入口：委托给批量删除 */
function menuDelete(item: FileNode) {
  deletePaths([item.path])
}

/** 压缩：输出到同目录，文件名 = 去扩展名 + 格式后缀 */
async function menuCompress(item: FileNode, format: string) {
  const base = item.type === 'dir' ? item.name : baseNameNoExt(item.name)
  const output = joinFs(parentDir(item.path), `${base}.${format}`)
  const pending = ElMessage({ message: `正在压缩为 ${base}.${format}…`, type: 'info', duration: 0, grouping: false })
  try {
    await invoke('archive_compress', {
      sessionId: props.sessionId, sources: [item.path], output, format
    })
    ElMessage.success(`压缩完成：${base}.${format}`)
    await refreshCurrentDir()
  } catch (e: any) {
    handleApiError(e, 'components/FileSystemBrowser.menuCompress', { prefix: '压缩失败' })
  } finally {
    pending.close()
  }
}

/** 解压：直接解压到当前目录 / 解压到以压缩包命名的文件夹；加密包失败时提示输入密码重试 */
async function menuExtract(item: FileNode, toFolder: boolean) {
  const parent = parentDir(item.path)
  const dest = toFolder ? joinFs(parent, stripArchiveExt(item.name)) : parent
  const pending = ElMessage({ message: `正在解压 ${item.name}…`, type: 'info', duration: 0, grouping: false })
  try {
    if (toFolder) {
      await invoke('make_remote_dir', { sessionId: props.sessionId, path: dest })
    }
    await invoke('archive_extract', { sessionId: props.sessionId, archive: item.path, destDir: dest, password: null })
    ElMessage.success(`解压完成：${dest}`)
    await refreshCurrentDir()
  } catch (e: any) {
    const msg = e?.message || String(e)
    if (/password|encrypted|wrong|密码/i.test(msg)) {
      // 加密压缩包：提示输入密码后重试一次
      try {
        const res = await ElMessageBox.prompt('该压缩包已加密，请输入密码', '解压密码', {
          inputType: 'password', inputPlaceholder: '压缩包密码'
        })
        await invoke('archive_extract', { sessionId: props.sessionId, archive: item.path, destDir: dest, password: res.value })
        ElMessage.success(`解压完成：${dest}`)
        await refreshCurrentDir()
      } catch (e2: any) {
        if (e2 !== 'cancel') {
          handleApiError(e2, 'components/FileSystemBrowser.menuExtract.retryWithPassword', { prefix: '解压失败' })
        }
      }
    } else {
      handleApiError(e, 'components/FileSystemBrowser.menuExtract', { prefix: '解压失败' })
    }
  } finally {
    pending.close()
  }
}

function sortBy(field: 'name' | 'size' | 'modified') {
  if (sortField.value === field) sortAsc.value = !sortAsc.value
  else { sortField.value = field; sortAsc.value = true }
}

// ===== 分栏视图（直接派生自统一目录树，天然与联动同步） =====
const columnData = computed(() => {
  const initialItems: FileNode[] = (props.kind === 'local' && pathStack.value.length === 0)
    ? currentChildren.value
    : rootChildren.value
  const chain: { items: FileNode[] }[] = [{ items: initialItems }]
  let children = initialItems
  for (const seg of pathStack.value) {
    const node = children.find(c => c.name === seg && c.type === 'dir')
    if (!node) break
    chain.push({ items: node.children ?? [] })
    children = node.children ?? []
  }
  return chain
})

/** 分栏行样式：当前路径链上被打开的目录逐级标记——最里层高亮、父级低亮；普通选中项次之 */
function columnItemClass(colIdx: number, item: FileNode): string {
  const seg = pathStack.value[colIdx]
  if (seg !== undefined && item.type === 'dir' &&
      (item.name === seg || item.name === seg + '\\' || item.name === seg + '/')) {
    return colIdx === pathStack.value.length - 1
      ? 'bg-blue-600/30 text-blue-200'
      : 'bg-blue-600/10 text-blue-200/70'
  }
  if (selectedPaths.value.has(item.path)) return 'bg-blue-600/20'
  return ''
}

async function onColumnClick(colIdx: number, item: FileNode, e: MouseEvent) {
  await ensureRenamingFinishedBeforeAction()
  handleSelectionClick(item, e)
  // 普通单击目录才进入；带 Ctrl/Shift 时只改变选中，不导航
  if ((e.ctrlKey || e.shiftKey || e.metaKey) || item.type !== 'dir') return
  historyBack.value.push([...pathStack.value])
  historyForward.value.length = 0
  if (props.kind === 'local' && pathStack.value.length === 0) {
    pathStack.value = [item.name.endsWith(':') ? item.name : item.name.replace(/[\\/]$/, '')]
  } else {
    pathStack.value = [...pathStack.value.slice(0, colIdx), item.name]
  }
  doNavigate()
}

// ===== 树形视图 =====
async function toggleTreeNode(node: FileNode) {
  if (node.type !== 'dir') return
  if (!node.expanded && !node.loaded) {
    node.children = (await fetchChildren(node.path)) ?? []
    node.loaded = true
  }
  node.expanded = !node.expanded
}

async function openFromTree(node: FileNode) {
  await ensureRenamingFinishedBeforeAction()
  if (node.type !== 'dir') {
    openInEditor(node)
    return
  }
  const targetSegs = props.kind === 'local' ? splitPathLocal(node.path) : splitPathSsh(node.path)
  const cur = pathStack.value
  const same = targetSegs.length === cur.length && targetSegs.every((s, i) => s === cur[i])
  if (same) return
  historyBack.value.push([...pathStack.value])
  historyForward.value.length = 0
  pathStack.value = targetSegs
  doNavigate()
}

/** 手机端：调起系统文件选择器，把选中的文件上传到当前目录（可多选，逐个传输） */
function pickAndUploadFiles() {
  const input = document.createElement('input')
  input.type = 'file'
  input.multiple = true
  input.style.display = 'none'
  input.addEventListener('change', async () => {
    const files = Array.from(input.files ?? [])
    input.remove()
    if (files.length === 0) return
    for (const f of files) {
      try {
        await uploadFileBytes(f, props.sessionId, currentPath.value)
      } catch { /* 失败已在传输队列标记错误，继续下一个 */ }
    }
    manualRefresh()
  })
  document.body.appendChild(input)
  input.click()
}

// ===== Toolbar Buttons =====
// 顺序：联动 | 创建文件/创建文件夹/删除选中/刷新 | 缩放 | 后退/前进/上级/自动刷新 | 视图
const allToolbarButtons = computed<ToolbarButton[]>(() => [
  // 1. 联动（FTP 模式下隐藏）
  ...(props.hideSync ? [] : [{
    key: 'sync',
    icon: Connection,
    tooltip: syncEnabled.value ? '联动终端：开（点击关闭）' : '联动终端：关（点击开启）',
    active: syncEnabled.value,
    onClick: toggleSync
  } as ToolbarButton, { key: 'd-1', divider: true, onClick: () => {} } as ToolbarButton]),
  // 2. 创建 / 删除 / 刷新
  {
    key: 'create-file',
    icon: DocumentAdd,
    tooltip: '创建文件',
    onClick: () => createInline('file')
  },
  {
    key: 'create-folder',
    icon: FolderAdd,
    tooltip: '创建文件夹',
    onClick: () => createInline('folder')
  },
  {
    key: 'delete-selected',
    icon: Delete,
    tooltip: selectedPaths.value.size > 0 ? `删除选中的 ${selectedPaths.value.size} 项` : '删除（请先选中文件或文件夹）',
    onClick: () => deletePaths(Array.from(selectedPaths.value))
  },
  {
    key: 'refresh-list',
    icon: Refresh,
    tooltip: loading.value ? '正在刷新…' : '刷新文件列表',
    onClick: () => manualRefresh()
  },
  { key: 'd-2', divider: true, onClick: () => {} },
  // 3. 缩放
  {
    key: 'zoom-out',
    icon: Minus,
    tooltip: '缩小 (' + itemZoomLabel.value + ')',
    onClick: zoomOut
  },
  {
    key: 'zoom-in',
    icon: Plus,
    tooltip: '放大 (' + itemZoomLabel.value + ')',
    onClick: zoomIn
  },
  { key: 'd-3', divider: true, onClick: () => {} },
  // 4. 导航
  {
    key: 'back',
    icon: ArrowLeft,
    tooltip: historyBack.value.length === 0 ? '后退（无历史记录）' : '后退',
    disabled: historyBack.value.length === 0,
    onClick: goBack
  },
  {
    key: 'forward',
    icon: ArrowRight,
    tooltip: historyForward.value.length === 0 ? '前进（尚未后退过）' : '前进',
    disabled: historyForward.value.length === 0,
    onClick: goForward
  },
  {
    key: 'up',
    icon: Top,
    tooltip: pathStack.value.length === 0 ? '已是最顶层目录' : '上级目录',
    disabled: pathStack.value.length === 0,
    onClick: goUp
  },
  { key: 'd-5', divider: true, onClick: () => {} },
  {
    key: 'upload-file',
    icon: Upload,
    tooltip: '从手机上传文件到当前目录',
    onClick: () => pickAndUploadFiles()
  },
  {
    key: 'auto-refresh',
    icon: Refresh,
    tooltip: '自动刷新',
    active: autoRefreshEnabled.value,
    onClick: () => {
      if (autoRefreshEnabled.value) {
        stopAutoRefresh()
        autoRefreshEnabled.value = false
      } else {
        autoRefreshEnabled.value = true
        startAutoRefresh()
      }
    }
  },
  { key: 'd-4', divider: true, onClick: () => {} },
  // 5. 视图
  {
    key: 'view-list',
    icon: ListViewIcon,
    tooltip: '列表视图',
    active: viewMode.value === 'list',
    onClick: () => viewMode.value = 'list'
  },
  {
    key: 'view-column',
    icon: ColumnViewIcon,
    tooltip: '分栏视图',
    active: viewMode.value === 'column',
    onClick: () => viewMode.value = 'column'
  },
  {
    key: 'view-tree',
    icon: TreeViewIcon,
    tooltip: '树状视图',
    active: viewMode.value === 'tree',
    onClick: () => viewMode.value = 'tree'
  },
  {
    key: 'view-icon',
    icon: GridViewIcon,
    tooltip: '图标视图',
    active: viewMode.value === 'icon',
    onClick: () => viewMode.value = 'icon'
  }
].map(wrapToolbarBtn))

/**
 * 移动端工具栏保留集合，按功能分组（组间用分隔符 d-x 隔开）：
 *  - 文件操作：新建文件、新建文件夹、刷新（d-2）
 *  - 缩放：-、+（d-3）
 *  - 导航：后退、前进、上级目录（d-5）
 *  - 上传：从手机上传文件到当前目录
 * 去掉「联动」（已要求长期开启）、「自动刷新」（列表本身有轮询）与四个视图切换按钮，
 * 窄屏下头部只留最常用的一排，其余交给长按右键菜单。
 */
const MOBILE_TOOLBAR_KEYS = [
  'create-file', 'create-folder', 'refresh-list',
  'd-2', 'zoom-out', 'zoom-in',
  'd-3', 'back', 'forward', 'up',
  'd-5', 'upload-file',
]
const toolbarButtons = computed<ToolbarButton[]>(() =>
  isMobile ? allToolbarButtons.value.filter(b => MOBILE_TOOLBAR_KEYS.includes(b.key)) : allToolbarButtons.value
)

// ===== 多窗口同步：工具栏按钮 + 滚动位置（仅 Ctrl 批量模式经 SplitPane 广播） =====
/** 工具栏按钮点击：先执行原动作，再上报按钮 key 与当前相关状态快照 */
function emitToolbarSync(key: string) {
  emit('fs-toolbar', {
    key,
    viewMode: viewMode.value,
    syncEnabled: syncEnabled.value,
    autoRefreshEnabled: autoRefreshEnabled.value,
  })
}
/** 包裹工具栏按钮的 onClick，使其在触发原动作后同步上报 */
function wrapToolbarBtn(b: ToolbarButton): ToolbarButton {
  const original = b.onClick
  return { ...b, onClick: () => { original(); emitToolbarSync(b.key) } }
}

/** 内容区滚动容器（用于监听/套用滚动同步） */
const fsContentRef = ref<HTMLElement | null>(null)
/** 远端滚动同步套用期间，抑制本地面板因 DOM 滚动事件回播，避免环路 */
let remoteScrollLockUntil = 0

/** 本地滚动：换算成 0..1 比例后上报（由 SplitPane 节流并广播） */
function onFsScrollLocal(e: Event) {
  if (remoteScrollLockUntil > Date.now()) return
  const el = e.target as HTMLElement
  const sx = el.scrollWidth > el.clientWidth ? el.scrollLeft / (el.scrollWidth - el.clientWidth) : 0
  const sy = el.scrollHeight > el.clientHeight ? el.scrollTop / (el.scrollHeight - el.clientHeight) : 0
  emit('fs-scroll', sx, sy)
}

/** 套用其它分屏同步过来的滚动比例：按比例设置所有可滚动后代元素 */
function applyScroll(sx: number, sy: number) {
  const root = fsContentRef.value
  if (!root) return
  remoteScrollLockUntil = Date.now() + 150
  root.querySelectorAll<HTMLElement>('*').forEach(el => {
    if (el.scrollHeight > el.clientHeight + 1) {
      el.scrollTop = sy * (el.scrollHeight - el.clientHeight)
    }
    if (el.scrollWidth > el.clientWidth + 1) {
      el.scrollLeft = sx * (el.scrollWidth - el.clientWidth)
    }
  })
}

/** 接收其它分屏在批量模式下同步过来的 fs 工具栏/视图/滚动状态 */
function applyRemoteFs(data: FsSyncPayload) {
  if (data.viewMode) viewMode.value = data.viewMode as 'list' | 'column' | 'tree' | 'icon'
  if (data.syncEnabled !== undefined) {
    syncEnabled.value = data.syncEnabled
    if (data.syncEnabled) startSyncTimer()
    else stopSyncTimer()
  }
  if (data.autoRefreshEnabled !== undefined) {
    autoRefreshEnabled.value = data.autoRefreshEnabled
    if (data.autoRefreshEnabled) startAutoRefresh()
    else stopAutoRefresh()
  }
  switch (data.toolbarKey) {
    case 'refresh-list': manualRefresh(); break
    case 'zoom-in': zoomIn(); break
    case 'zoom-out': zoomOut(); break
    case 'back': goBack(); break
    case 'forward': goForward(); break
    case 'up': goUp(); break
    // 视图/联动/自动刷新 通过上面的 viewMode/syncEnabled/autoRefreshEnabled 字段同步
  }
  if (data.scrollX !== undefined && data.scrollY !== undefined) applyScroll(data.scrollX, data.scrollY)
}
defineExpose({ applyRemoteFs })

// ===== 视图图标：统一线性风格（14x14，stroke=currentColor，1.2px，圆角端点） =====
const svgAttrs = {
  viewBox: '0 0 14 14', width: 14, height: 14,
  fill: 'none', stroke: 'currentColor',
  'stroke-width': '1.2', 'stroke-linecap': 'round', 'stroke-linejoin': 'round'
}
/** 列表视图：项目符号 + 三行 */
const ListViewIcon = defineComponent({
  render: () => h('svg', svgAttrs, [
    h('circle', { cx: 2, cy: 3, r: 1, fill: 'currentColor', stroke: 'none' }),
    h('line', { x1: 5, y1: 3, x2: 13, y2: 3 }),
    h('circle', { cx: 2, cy: 7, r: 1, fill: 'currentColor', stroke: 'none' }),
    h('line', { x1: 5, y1: 7, x2: 13, y2: 7 }),
    h('circle', { cx: 2, cy: 11, r: 1, fill: 'currentColor', stroke: 'none' }),
    h('line', { x1: 5, y1: 11, x2: 13, y2: 11 })
  ])
})
/** 分栏视图：三个竖向面板 */
const ColumnViewIcon = defineComponent({
  render: () => h('svg', svgAttrs, [
    h('rect', { x: 1, y: 1.5, width: 3.4, height: 11, rx: 0.5 }),
    h('rect', { x: 5.3, y: 1.5, width: 3.4, height: 11, rx: 0.5 }),
    h('rect', { x: 9.6, y: 1.5, width: 3.4, height: 11, rx: 0.5 })
  ])
})
/** 树状视图：竖向主干 + 三个分支条目 */
const TreeViewIcon = defineComponent({
  render: () => h('svg', svgAttrs, [
    h('line', { x1: 2.5, y1: 2, x2: 2.5, y2: 12 }),
    h('line', { x1: 2.5, y1: 4, x2: 5, y2: 4 }),
    h('line', { x1: 2.5, y1: 8, x2: 5, y2: 8 }),
    h('line', { x1: 2.5, y1: 12, x2: 5, y2: 12 }),
    h('rect', { x: 5, y: 2.9, width: 8, height: 2.2, rx: 0.6 }),
    h('rect', { x: 5, y: 6.9, width: 8, height: 2.2, rx: 0.6 }),
    h('rect', { x: 5, y: 10.9, width: 8, height: 2.2, rx: 0.6 })
  ])
})
/** 图标视图：2x2 网格 */
const GridViewIcon = defineComponent({
  render: () => h('svg', svgAttrs, [
    h('rect', { x: 1, y: 1, width: 5.2, height: 5.2, rx: 0.8 }),
    h('rect', { x: 7.8, y: 1, width: 5.2, height: 5.2, rx: 0.8 }),
    h('rect', { x: 1, y: 7.8, width: 5.2, height: 5.2, rx: 0.8 }),
    h('rect', { x: 7.8, y: 7.8, width: 5.2, height: 5.2, rx: 0.8 })
  ])
})

// ===== 传输队列（状态栏总进度 + 详情弹窗） =====
const transferStore = useTransferStore()
const showQueue = ref(false)
const sessionTransfers = computed(() =>
  transferStore.items.filter(i => i.ownerSessionId === props.sessionId).slice().reverse()
)
/** 进行中传输聚合：总进度 = Σtransferred / Σtotal（total 未知 → -1 不定态） */
const transferSummary = computed(() => {
  const act = transferStore.items.filter(i => i.ownerSessionId === props.sessionId && !i.done)
  const transferred = act.reduce((s, i) => s + i.transferred, 0)
  const total = act.reduce((s, i) => s + i.total, 0)
  const percent = total > 0 ? Math.min(100, Math.round((transferred / total) * 100)) : (act.length ? -1 : 0)
  return { count: act.length, percent }
})
/**
 * 全局传输队列请求（顶部菜单「工具-传输队列」触发）：
 * - targetSessionId 与本组件 sessionId 匹配（或为空表示任意）→ 打开本地队列弹窗
 * - 关闭时若当前是本组件响应打开的，也同步关闭
 */
let queueOpenedByGlobal = false
watch(
  () => transferStore.globalRequestSeq,
  () => {
    if (!transferStore.globalVisible) {
      if (queueOpenedByGlobal) {
        showQueue.value = false
        queueOpenedByGlobal = false
      }
      return
    }
    const target = transferStore.targetSessionId
    if (target && target !== props.sessionId) return
    showQueue.value = true
    queueOpenedByGlobal = true
  }
)
/** 关闭本地队列时通知全局（避免下次菜单触发 seq 自增但 visible 仍为 true 不响应） */
function onLocalQueueClose() {
  showQueue.value = false
  if (queueOpenedByGlobal) {
    transferStore.closeGlobal()
    queueOpenedByGlobal = false
  }
}
/** 弹窗从状态栏上方弹出（底部锚定），头部以下区域内自适应高度 */
const queuePanelStyle = computed(() => ({
  left: '8px',
  right: '8px',
  bottom: '30px',
  minHeight: '120px',
  maxHeight: props.hideHeader ? 'calc(100% - 40px)' : 'calc(100% - 72px)'
}))

/** 路径的父目录（兼容 / 与 \） */
function parentDir(p: string): string {
  const norm = p.replace(/[\\/]+$/, '')
  const idx = Math.max(norm.lastIndexOf('/'), norm.lastIndexOf('\\'))
  if (idx < 0) return norm
  if (idx === 0) return norm.startsWith('/') ? '/' : norm
  if (idx === 2 && norm[1] === ':') return norm.slice(0, 3)
  return norm.slice(0, idx)
}

/** 队列弹窗"打开所在目录"：本地 fs 跳转到该文件父目录；SSH fs 在 OS 文件管理器中打开本地下载路径 */
async function openTransferDir(item: TransferItem) {
  showQueue.value = false
  const dir = parentDir(item.dstPath)
  if (!dir) return
  if (props.kind === 'local') {
    await navigateToPath(dir)
  } else {
    // SSH 终端会话下，下载到本地的文件不在远端目录树中，用 OS 文件管理器打开
    try {
      await invoke('open_in_file_manager', { path: item.dstPath })
    } catch (e: any) {
      handleApiError(e, 'components/FileSystemBrowser.openInFileManager', {
        prefix: '打开所在目录失败',
      })
    }
  }
}

// ===== OS 文件拖入放置区（本 fs 面板：悬停文件夹 → 该文件夹，否则当前目录） =====
const fsRootRef = ref<HTMLElement | null>(null)
let fsZoneId = ''

function registerFsZone() {
  if (fsZoneId) { unregisterOsDropZone(fsZoneId); fsZoneId = '' }
  if (!props.sessionId || !fsRootRef.value) return
  fsZoneId = `fs-${props.sessionId}-${Math.random().toString(36).slice(2, 6)}`
  registerOsDropZone({
    id: fsZoneId,
    el: fsRootRef.value,
    sessionId: props.sessionId,
    local: props.kind === 'local',
    resolveDir: (x, y) => {
      const el = document.elementFromPoint(x, y)?.closest?.('[data-fs-dir]')
      return el?.getAttribute('data-fs-dir') || null
    }
  })
}

// 传输完成后：目标目录正是当前目录则自动刷新
watch(() => transferStore.items.filter(i => i.ownerSessionId === props.sessionId && i.done).map(i => i.id).join(','), () => {
  const hit = transferStore.items.some(i =>
    i.ownerSessionId === props.sessionId && i.done && !i.error && parentDir(i.dstPath) === currentPath.value
  )
  if (hit) doNavigate()
})

// 连接状态或 sessionId 变化时重置并加载根目录
watch(() => [props.connected, props.sessionId] as const, async ([connected, sessionId]) => {
  stopSyncTimer()
  stopAutoRefresh()
  rootChildren.value = []
  rootLoaded.value = false
  localRoots.value = []
  localRootsLoaded.value = false
  localHome.value = ''
  isWindows.value = false
  pathStack.value = []
  historyBack.value.length = 0
  historyForward.value.length = 0
  selectedPaths.value = new Set()
  loadError.value = ''
  pendingTermPath = null
  lastTermCwd = ''
  ctxMenu.value = null
  archiveTools.value = null
  lastFingerprint = ''
  if (connected && sessionId) {
    // 初始化（含「等终端 cwd 就绪」的重试窗口）整体纳入 loading 覆盖：
    // 否则这段等待里页面是空列表且无任何反馈，用户会以为卡死或没加载出来。
    beginOverlayLoading()
    try {
      if (props.kind === 'local') {
        // 先拉驱动器+家目录，为 splitPathLocal 提供 isWindows/localSep 依赖
        await ensureLocalRootsAndHome()
        // 主动同步 shell cwd：与 SSH 一致，避免 3 秒后定时器把 fs 从家目录拉到 shell 启动目录
        await syncInitialCwdFromTerminal()
        // shell cwd 读取失败兜底定位到家目录
        if (pathStack.value.length === 0 && localHome.value) {
          pathStack.value = splitPathLocal(localHome.value)
        }
      } else {
        // SSH：主动拉一次终端 cwd 作为文件系统初始目录，避免 3 秒后联动定时器把 fs 从 / 跳到 home
        await syncInitialCwdFromTerminal()
      }
      await ensurePathLoaded(pathStack.value)
    } finally {
      endOverlayLoading()
    }
    if (syncEnabled.value) startSyncTimer()
    if (autoRefreshEnabled.value) startAutoRefresh()
  }
}, { immediate: true })

/**
 * 会话初次连接时，主动同步一次终端 cwd 到 fs（本地与 SSH 通用）。
 * - 最多重试 5 次（每次 500ms），覆盖 shell_pid 还没就绪的窗口
 * - 成功后把 lastTermCwd 一起设上，避免 3 秒后联动定时器再次触发 navigateToPath
 * - 全部失败：保持 pathStack=[]（根目录），后续联动定时器兜底
 */
async function syncInitialCwdFromTerminal() {
  if (!props.sessionId) return
  for (let i = 0; i < 5; i++) {
    try {
      const out = await invoke<string>('get_shell_cwd', { sessionId: props.sessionId })
      const cwd = out.trim()
      if (cwd) {
        lastTermCwd = cwd
        pathStack.value = props.kind === 'local' ? splitPathLocal(cwd) : splitPathSsh(cwd)
        return
      }
    } catch { /* shell 还没就绪，重试 */ }
    // 最后一次失败后不再等待：这段等待只为「下一次重试」服务，白等 500ms 只会拖慢首屏
    if (i < 4) await new Promise(r => setTimeout(r, 500))
  }
}

watch(() => props.sessionId, () => registerFsZone(), { flush: 'post' })
/** 取消"解冻后补做"回调注册 */
let unsubscribeThaw: (() => void) | null = null
onMounted(() => {
  registerFsZone()
  // 拖拽期间被跳过的轮询，解冻后立即补做一次（定时器未运行说明组件已停用，不补）
  unsubscribeThaw = onUiThaw(() => {
    if (syncTimer && syncEnabled.value && props.connected && props.sessionId) void syncCurrentDir()
    if (autoRefreshTimer && !loading.value && props.connected && props.sessionId) void doAutoRefresh()
  })
  // 面包屑溢出检测
  nextTick(() => {
    if (breadcrumbRef.value) {
      breadcrumbObserver = new ResizeObserver(checkBreadcrumbOverflow)
      breadcrumbObserver.observe(breadcrumbRef.value)
      // 同时观察父容器宽度变化（Toolbar 可能折叠）
      const parent = breadcrumbRef.value.parentElement
      if (parent) breadcrumbObserver.observe(parent)
      checkBreadcrumbOverflow()
    }
  })
})

// 路径切换时重置指纹（防止旧目录与新目录指纹撞车）
watch(() => pathStack.value.slice().join('/'), () => {
  lastFingerprint = ''
  checkBreadcrumbOverflow()
  if (autoRefreshEnabled.value && props.connected && props.sessionId) {
    startAutoRefresh()
  }
})

onBeforeUnmount(() => {
  unsubscribeThaw?.()
  unsubscribeThaw = null
  stopSyncTimer()
  stopAutoRefresh()
  if (fsZoneId) { unregisterOsDropZone(fsZoneId); fsZoneId = '' }
  breadcrumbObserver?.disconnect()
  breadcrumbObserver = null
})

// keep-alive 缓存：切走暂停后台定时器省资源，切回恢复
onActivated(() => {
  pageVisible = true
  if (syncEnabled.value && props.connected && props.sessionId) startSyncTimer()
  if (autoRefreshEnabled.value && props.connected && props.sessionId) startAutoRefresh()
  // 新会话可能在页面不可见（keep-alive 缓存）期间就完成了加载：loading 在后台走完，
  // 用户切回时什么都看不到。这里补播一次，保证「每次进入新会话都能看到加载过渡」。
  if (overlayReplayPending) {
    overlayReplayPending = false
    beginOverlayLoading()
    endOverlayLoading()
  }
})
onDeactivated(() => {
  pageVisible = false
  stopSyncTimer()
  stopAutoRefresh()
})

// 行内重命名：input 挂载时由 @vue:mounted 自动 focus + 选中，无需额外 watch

// 点击 Content Area 空白处：清空选中 + 退出重命名
function onContentBgClick() {
  if (renamingPath.value) {
    const item = currentChildren.value.find(c => c.path === renamingPath.value)
    if (!item) {
      renamingPath.value = ''
      renamingOriginalName.value = ''
      return
    }
    const el = fsRootRef.value?.querySelector('input[data-rename-input]') as HTMLInputElement | null
    if (el && document.activeElement === el) {
      // input 已 focus：blur 会触发 onRenameBlur → commitRename，避免并发提交
      el.blur()
    } else {
      // input 未 focus（focus 失败兜底）：直接提交
      commitRename(item, renamingOriginalName.value)
    }
    return
  }
  selectedPaths.value = new Set()
  shiftAnchorPath.value = ''
}

/** 重命名 input 双击：全选名字主体（不含扩展名），方便直接改名 */
function selectRenameText(e: Event) {
  const el = e.target as HTMLInputElement
  if (!el || typeof el.select !== 'function') return
  // 仅当当前没有选区（用户没主动拖选）时才接管双击 → 全选文件名主体
  const s = el.selectionStart ?? 0
  const en = el.selectionEnd ?? 0
  if (s !== en) return
  const dot = el.value.lastIndexOf('.')
  if (dot > 0) el.setSelectionRange(0, dot)
  else el.select()
}

/** 重命名 input 失焦：自动提交（与 Enter 提交共用 commitRename，guard 防重入） */
async function onRenameBlur(item: FileNode) {
  if (renamingPath.value !== item.path) return
  // 兜底：如果 input 从未 focus（如 watch 的 rAF focus 失败），
  // onContentBgClick 会兜底提交；这里直接信任 commitRename 的 guard
  await commitRename(item, renamingOriginalName.value)
  // 提交失败（重名/非法字符/后端报错）→ 仍在重命名态，重新 focus 让用户继续编辑
  if (renamingPath.value === item.path) {
    await new Promise<void>(r => requestAnimationFrame(() => r()))
    const el = fsRootRef.value?.querySelector('input[data-rename-input]') as HTMLInputElement | null
    el?.focus()
  }
}

// 键盘快捷键：Ctrl+A 全选当前目录，F2 触发单选重命名
function onFsKeydown(e: KeyboardEvent) {
  const ae = document.activeElement as HTMLElement | null
  const inEditor = !!ae && (ae.tagName === 'INPUT' || ae.tagName === 'TEXTAREA' || (ae as any).isContentEditable)
  if (e.key === 'a' && (e.ctrlKey || e.metaKey) && !inEditor) {
    e.preventDefault()
    selectedPaths.value = new Set(currentChildren.value.map(c => c.path))
  } else if (e.key === 'F2' && !inEditor) {
    if (selectedPaths.value.size === 1) {
      const p = Array.from(selectedPaths.value)[0]
      const it = currentChildren.value.find(c => c.path === p)
      if (it) {
        e.preventDefault()
        renamingPath.value = it.path
        renamingOriginalName.value = it.name
      }
    }
  } else if (e.key === 'Delete' && !inEditor) {
    if (selectedPaths.value.size > 0) {
      e.preventDefault()
      deletePaths(Array.from(selectedPaths.value))
    }
  }
}
onMounted(() => document.addEventListener('keydown', onFsKeydown))
onBeforeUnmount(() => document.removeEventListener('keydown', onFsKeydown))
</script>

<style scoped>
/* 工具条包装层：PC 端 display:contents 让包装层不产生盒子，
   工具条仍是原布局里的那个 flex 子项（宽度 / 溢出收纳判定完全不受影响）；
   移动端由 mobile.css 第 19.2 节改用 display:flex + 绝对定位贴面板底部 */
.fs-mobile-toolbar { display: contents; }
.scrollbar-hide::-webkit-scrollbar { display: none; }
.scrollbar-hide { -ms-overflow-style: none; scrollbar-width: none; }
.line-clamp-2 {
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
/* 列表表头列宽拖拽手柄 */
.col-resize-handle {
  position: absolute;
  top: 0;
  right: -4px;
  width: 8px;
  height: 100%;
  cursor: col-resize;
  user-select: none;
  z-index: 2;
}
.col-resize-handle:hover,
.col-resize-handle:active { background: rgba(128, 128, 128, 0.35); }

/* 文件列表加载过渡覆盖层：仅 loading 为 true 时出现，铺在内容区之上 */
.fs-loading {
  position: absolute;
  inset: 0;
  z-index: 20;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  background: var(--jc-bg);
  color: var(--jc-text-muted);
  pointer-events: none;
}
.fs-loading__spin { animation: jc-fs-spin 0.9s linear infinite; }
@keyframes jc-fs-spin { to { transform: rotate(360deg); } }
.fs-loading__text { font-size: 13px; letter-spacing: 0.05em; }
/* 加载过渡：淡入淡出 */
.jc-fs-fade-enter-active,
.jc-fs-fade-leave-active { transition: opacity 0.2s ease; }
.jc-fs-fade-enter-from,
.jc-fs-fade-leave-to { opacity: 0; }
</style>