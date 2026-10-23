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
  <div class="device-info-panel h-full flex flex-col bg-[var(--jc-bg-soft)] overflow-hidden">
    <!-- Header：按钮位置随 dock 自动排布（right dock 时折叠按钮最左），折叠态保留头部 -->
    <PanelHeader panel-id="deviceInfo" :dock="dock" :collapsed="collapsed" @collapse="$emit('collapse')">
      <!-- 移动端只有两个按钮（折叠展开 / 设置），头部右侧留白多，改成靠右显示 -->
      <AdaptiveToolbar v-if="!collapsed" :buttons="toolbarButtons" :align="isMobile ? 'right' : 'left'" />
    </PanelHeader>

    <!-- 折叠态：占位图标（DeviceInfo 固定 Monitor） -->
    <Transition name="jc-swap">
      <div
        v-if="collapsed"
        class="flex-1 min-h-0 min-w-0 p-1 flex gap-1 items-center justify-center"
        :class="dock === 'bottom' ? 'flex-row overflow-x-auto' : 'flex-col overflow-y-auto overflow-x-hidden'"
      >
        <div class="w-7 h-7 shrink-0 flex items-center justify-center rounded" :title="t('device.collapsed')">
          <el-icon :size="16" color="var(--jc-text-muted)"><Monitor /></el-icon>
        </div>
      </div>
    </Transition>

    <Transition name="jc-swap">
      <div v-show="!collapsed" class="flex-1 overflow-y-auto px-2.5 py-2 space-y-2">
      <div v-if="!connected" class="text-center text-[var(--jc-text-muted)] text-sm mt-8">
        <el-icon size="40" class="mb-2"><Connection /></el-icon>
        <div>{{ t('device.noConnection') }}</div>
        <div class="text-xs mt-1 opacity-60">{{ t('device.noConnectionHint') }}</div>
      </div>

      <div
        v-else
        data-drag-container
        :class="['cards-container', layout.deviceViewMode]"
      >
        <div
          v-for="(element, index) in visibleCards"
          :key="element.id"
          class="card rounded-lg bg-[var(--jc-bg-hover)] border border-[var(--jc-border)]"
          :class="{ 'card--dragging': dragIndex === index }"
          :data-drag-index="index"
        >
            <div class="card-header flex items-center gap-2 px-3 py-2">
              <span class="card-handle" :title="t('device.dragSort')" @mousedown="startDrag($event, index)">
                <el-icon size="12"><Rank /></el-icon>
              </span>
              <span class="text-xs font-semibold text-[var(--jc-text-soft)] uppercase">{{ titles[element.id as DeviceCardId] }}</span>
              <span class="ml-auto text-xs font-bold" :class="summaryClass(element.id)">{{ summary(element.id) }}</span>
              <!-- CPU 进程视图切换 -->
              <el-button
                v-if="element.id === 'cpu'"
                size="small" text class="!p-0 !h-5"
                :title="layout.cpuProcessView ? t('device.view.backSystem') : t('device.view.process')"
                @click="layout.toggleCpuProcessView()"
              >
                <el-icon size="13">
                  <List v-if="!layout.cpuProcessView" />
                  <DataLine v-else />
                </el-icon>
              </el-button>
              <!-- 内存进程视图切换 -->
              <el-button
                v-if="element.id === 'mem'"
                size="small" text class="!p-0 !h-5"
                :title="layout.memProcessView ? t('device.view.backSystem') : t('device.view.process')"
                @click="layout.toggleMemProcessView()"
              >
                <el-icon size="13">
                  <List v-if="!layout.memProcessView" />
                  <DataLine v-else />
                </el-icon>
              </el-button>
              <el-button size="small" text class="!p-0 !h-5" :title="element.collapsed ? t('device.expand') : t('device.collapseBtn')" @click="layout.toggleCardCollapsed(element.id)">
                <el-icon size="13">
                  <ArrowDown v-if="!element.collapsed" />
                  <ArrowRight v-else />
                </el-icon>
              </el-button>
              <!-- 放大展示按钮 -->
              <el-button size="small" text class="!p-0 !h-5 ml-1" :title="t('device.enlarge')" @click="openDetail(element.id)">
                <el-icon size="13"><FullScreen /></el-icon>
              </el-button>
            </div>
            <!-- 折叠态：最小化 -->
            <div v-if="element.collapsed" class="px-3 pb-2 text-[11px] text-[var(--jc-text-muted)]">
              {{ mini(element.id) }}
            </div>
            <!-- 展开态：详细内容 -->
            <div v-else class="px-3 pb-3">
              <template v-if="element.id === 'system'">
                <div class="space-y-1.5 text-xs">
                  <div class="flex justify-between"><span class="text-[var(--jc-text-muted)]">{{ t('device.meta.hostname') }}</span><span class="text-[var(--jc-text-soft)]">{{ meta.hostname }}</span></div>
                  <div class="flex justify-between"><span class="text-[var(--jc-text-muted)]">{{ t('device.meta.os') }}</span><span class="text-[var(--jc-text-soft)]">{{ meta.os }}</span></div>
                  <div class="flex justify-between"><span class="text-[var(--jc-text-muted)]">{{ t('device.meta.kernel') }}</span><span class="text-[var(--jc-text-soft)]">{{ meta.kernel }}</span></div>
                  <div class="flex justify-between"><span class="text-[var(--jc-text-muted)]">{{ t('device.meta.uptime') }}</span><span class="text-[var(--jc-text-soft)]">{{ meta.uptime }}</span></div>
                  <div class="flex justify-between"><span class="text-[var(--jc-text-muted)]">{{ t('device.meta.cpuModel') }}</span><span class="text-[var(--jc-text-soft)] truncate ml-2" :title="meta.cpuModel">{{ meta.cpuModel }}</span></div>
                  <div class="flex justify-between"><span class="text-[var(--jc-text-muted)]">{{ t('device.meta.cores') }}</span><span class="text-[var(--jc-text-soft)]">{{ meta.cpuCores }} {{ t('device.meta.coreUnit') }}</span></div>
                  <div class="flex justify-between"><span class="text-[var(--jc-text-muted)]">{{ t('device.meta.ip') }}</span><span class="text-[var(--jc-text-soft)]">{{ meta.ip }}</span></div>
                </div>
              </template>
              <template v-else-if="element.id === 'cpu'">
                <ProcessView v-if="layout.cpuProcessView" :rows="processes" sort-key="cpu" :hide-mem="true" @kill="killProcess" @refresh="regenProcesses" />
                <div v-else class="mt-1">
                  <div class="h-2 rounded-full bg-[var(--jc-bg)] overflow-hidden mb-2">
                    <div class="h-full rounded-full transition-all duration-500" :style="{ width: cpu.usage + '%', background: cpuGradient }"></div>
                  </div>
                  <div class="grid grid-cols-4 gap-1.5">
                    <div v-for="(u, i) in cpu.perCore" :key="i" class="text-center">
                      <div class="h-1 rounded-full bg-[var(--jc-bg)] overflow-hidden mb-0.5">
                        <div class="h-full rounded-full bg-blue-500 transition-all duration-500" :style="{ width: u + '%' }"></div>
                      </div>
                      <span class="text-[10px] text-[var(--jc-text-muted)]">{{ t('device.meta.coreUnit') }}{{ i }}</span>
                    </div>
                  </div>
                  <div class="flex items-center gap-3 mt-2 text-xs">
                    <span class="text-[var(--jc-text-muted)]">{{ t('device.cpu.load') }}:</span>
                    <span class="flex items-center gap-1" title="1 分钟 / 5 分钟 / 15 分钟 平均负载">
                      <span class="text-[var(--jc-text-soft)] font-mono">{{ cpu.loadAvg[0].toFixed(2) }}</span>
                      <span class="text-[var(--jc-text-muted)] text-[10px]">1m</span>
                      <span class="text-[var(--jc-text-soft)] font-mono ml-1">{{ cpu.loadAvg[1].toFixed(2) }}</span>
                      <span class="text-[var(--jc-text-muted)] text-[10px]">5m</span>
                      <span class="text-[var(--jc-text-soft)] font-mono ml-1">{{ cpu.loadAvg[2].toFixed(2) }}</span>
                      <span class="text-[var(--jc-text-muted)] text-[10px]">15m</span>
                    </span>
                  </div>
                </div>
              </template>
              <template v-else-if="element.id === 'mem'">
                <ProcessView v-if="layout.memProcessView" :rows="processes" sort-key="mem" :hide-cpu="true" @kill="killProcess" @refresh="regenProcesses" />
                <div v-else class="mt-1">
                  <div class="h-2 rounded-full bg-[var(--jc-bg)] overflow-hidden mb-1">
                    <div class="h-full rounded-full transition-all duration-500" :style="{ width: memPercent + '%', background: 'linear-gradient(90deg, #10b981, #34d399)' }"></div>
                  </div>
                  <div class="flex justify-between text-xs text-[var(--jc-text-muted)]">
                    <span>{{ t('device.mem.used') }} {{ formatSize(mem.used) }}</span>
                    <span>{{ t('device.mem.available') }} {{ formatSize(mem.available) }}</span>
                  </div>
                  <div class="flex gap-3 mt-1.5 text-xs">
                    <span class="text-[var(--jc-text-muted)]">{{ t('device.mem.cached') }}: {{ formatSize(mem.cached) }}</span>
                    <span class="text-[var(--jc-text-muted)]">Swap: {{ formatSize(mem.swapUsed) }}/{{ formatSize(mem.swapTotal) }}</span>
                  </div>

                </div>
              </template>
              <template v-else-if="element.id === 'disk'">
                <div class="mt-1">
                  <div class="h-2 rounded-full bg-[var(--jc-bg)] overflow-hidden mb-2">
                    <div class="h-full rounded-full transition-all duration-500" :style="{ width: diskPercent + '%', background: diskGradient }"></div>
                  </div>
                  <div class="space-y-1">
                    <div v-for="p in disk.partitions" :key="p.mount" class="flex items-center gap-2 text-xs">
                      <span class="text-[var(--jc-text-soft)] max-w-[35%] min-w-0 flex-shrink-0 truncate" :title="p.mount">{{ p.mount }}</span>
                      <div class="flex-1 min-w-[20px] max-w-[200px] h-1 rounded-full bg-[var(--jc-bg)] overflow-hidden">
                        <div class="h-full rounded-full bg-purple-500 transition-all duration-500" :style="{ width: (p.used / p.total * 100) + '%' }"></div>
                      </div>
                      <span class="text-[var(--jc-text-muted)] flex-shrink-0 whitespace-nowrap text-right">{{ formatSize(p.used) }}/{{ formatSize(p.total) }}</span>
                    </div>
                  </div>
                </div>
              </template>
              <template v-else-if="element.id === 'net'">
                <div class="mt-1 flex items-center justify-around">
                  <div class="text-center">
                    <el-icon size="18" color="var(--jc-success)"><Bottom /></el-icon>
                    <div class="text-sm font-bold text-green-400 mt-1">{{ formatSize(net.rxRate) }}/s</div>
                    <div class="text-[10px] text-[var(--jc-text-muted)]">{{ t('device.net.down') }}</div>
                  </div>
                  <div class="text-center">
                    <el-icon size="18" color="var(--jc-warn)"><Top /></el-icon>
                    <div class="text-sm font-bold text-amber-400 mt-1">{{ formatSize(net.txRate) }}/s</div>
                    <div class="text-[10px] text-[var(--jc-text-muted)]">{{ t('device.net.up') }}</div>
                  </div>
                </div>
              </template>
              <template v-else-if="element.id === 'proc'">
                <div class="mt-1">
                  <ProcessView :rows="processes" sort-key="cpu" show-ports @kill="killProcess" @refresh="regenProcesses" />
                </div>
              </template>
              <template v-else-if="element.id === 'aiAssistant'">
                <AiAssistantCard />
              </template>
            </div>
        </div>
      </div>
      </div>
    </Transition>

    <DeviceCardSettings v-model="settingsOpen" />

    <!-- 放大展示 Modal -->
    <el-dialog
      v-model="detailModalOpen"
      :title="currentDetailCard ? titles[currentDetailCard] : ''"
      width="600"
      append-to-body
      :close-on-click-modal="true"
      align-center
      class="device-detail-dialog"
      @click.self="closeDetail"
    >
      <template v-if="currentDetailCard === 'system'">
        <div class="grid grid-cols-2 gap-x-6 gap-y-2 text-sm">
          <div class="flex justify-between"><span class="text-[var(--jc-text-muted)]">{{ t('device.meta.hostname') }}</span><span class="text-[var(--jc-text-soft)]">{{ meta.hostname }}</span></div>
          <div class="flex justify-between"><span class="text-[var(--jc-text-muted)]">{{ t('device.meta.os') }}</span><span class="text-[var(--jc-text-soft)]">{{ meta.os }}</span></div>
          <div class="flex justify-between"><span class="text-[var(--jc-text-muted)]">{{ t('device.meta.kernel') }}</span><span class="text-[var(--jc-text-soft)]">{{ meta.kernel }}</span></div>
          <div class="flex justify-between"><span class="text-[var(--jc-text-muted)]">{{ t('device.meta.uptime') }}</span><span class="text-[var(--jc-text-soft)]">{{ meta.uptime }}</span></div>
          <div class="flex justify-between col-span-2"><span class="text-[var(--jc-text-muted)]">{{ t('device.meta.cpuModel') }}</span><span class="text-[var(--jc-text-soft)]">{{ meta.cpuModel }}</span></div>
          <div class="flex justify-between"><span class="text-[var(--jc-text-muted)]">{{ t('device.meta.cores') }}</span><span class="text-[var(--jc-text-soft)]">{{ meta.cpuCores }} {{ t('device.meta.coreUnit') }}</span></div>
          <div class="flex justify-between"><span class="text-[var(--jc-text-muted)]">{{ t('device.meta.ip') }}</span><span class="text-[var(--jc-text-soft)]">{{ meta.ip }}</span></div>
          <div class="flex justify-between"><span class="text-[var(--jc-text-muted)]">{{ t('device.meta.arch') }}</span><span class="text-[var(--jc-text-soft)]">x86_64</span></div>
          <div class="flex justify-between"><span class="text-[var(--jc-text-muted)]">{{ t('device.meta.timezone') }}</span><span class="text-[var(--jc-text-soft)]">Asia/Shanghai</span></div>
        </div>
      </template>
      <template v-else-if="currentDetailCard === 'cpu'">
        <div class="space-y-4">
          <div class="flex items-center justify-center gap-4">
            <div class="text-center">
              <div class="text-5xl font-bold" :class="cpuUsageClass">{{ cpu.usage.toFixed(1) }}%</div>
              <div class="text-xs text-[var(--jc-text-muted)] mt-1">{{ t('device.cpu.usageTotal') }}</div>
            </div>
          </div>
          <div class="h-3 rounded-full bg-[var(--jc-bg)] overflow-hidden">
            <div class="h-full rounded-full transition-all duration-500" :style="{ width: cpu.usage + '%', background: cpuGradient }"></div>
          </div>
          <div>
            <div class="text-xs text-[var(--jc-text-muted)] mb-2">{{ t('device.cpu.coreLoad') }}</div>
            <div class="grid gap-2" :style="{ gridTemplateColumns: 'repeat(auto-fill, minmax(80px, 1fr))' }">
              <div v-for="(u, i) in cpu.perCore" :key="i" class="text-center">
                <div class="h-2 rounded-full bg-[var(--jc-bg)] overflow-hidden mb-1">
                  <div class="h-full rounded-full bg-blue-500 transition-all duration-500" :style="{ width: u + '%' }"></div>
                </div>
                <div class="text-xs text-[var(--jc-text-soft)]">{{ u.toFixed(0) }}%</div>
                <div class="text-[10px] text-[var(--jc-text-muted)]">{{ t('device.meta.coreUnit') }}{{ i }}</div>
              </div>
            </div>
          </div>
          <div class="grid grid-cols-3 gap-4 text-center">
            <div class="p-2 bg-[var(--jc-bg-hover)] rounded">
              <div class="text-lg font-semibold text-[var(--jc-text-soft)]">{{ cpu.loadAvg[0].toFixed(2) }}</div>
              <div class="text-[10px] text-[var(--jc-text-muted)]">{{ t('device.cpu.load1') }}</div>
            </div>
            <div class="p-2 bg-[var(--jc-bg-hover)] rounded">
              <div class="text-lg font-semibold text-[var(--jc-text-soft)]">{{ cpu.loadAvg[1].toFixed(2) }}</div>
              <div class="text-[10px] text-[var(--jc-text-muted)]">{{ t('device.cpu.load5') }}</div>
            </div>
            <div class="p-2 bg-[var(--jc-bg-hover)] rounded">
              <div class="text-lg font-semibold text-[var(--jc-text-soft)]">{{ cpu.loadAvg[2].toFixed(2) }}</div>
              <div class="text-[10px] text-[var(--jc-text-muted)]">{{ t('device.cpu.load15') }}</div>
            </div>
          </div>
          <div class="flex items-center justify-center gap-2 text-xs">
            <span class="text-[var(--jc-text-soft)]">{{ t('device.processView') }}:</span>
            <el-button size="small" text @click="layout.toggleCpuProcessView()">
              <el-icon class="mr-1"><component :is="layout.cpuProcessView ? DataLine : List" /></el-icon>
              {{ layout.cpuProcessView ? t('device.view.backSystem') : t('device.view.process') }}
            </el-button>
          </div>
          <ProcessView v-if="layout.cpuProcessView" :rows="processes" sort-key="cpu" :hide-mem="true" @kill="killProcess" @refresh="regenProcesses" />
        </div>
      </template>
      <template v-else-if="currentDetailCard === 'mem'">
        <div class="space-y-4">
          <div class="flex items-center justify-center gap-4">
            <div class="text-center">
              <div class="text-5xl font-bold text-emerald-400">{{ formatSize(mem.used).split(' ')[0] }} <span class="text-2xl text-[var(--jc-text-soft)]">{{ formatSize(mem.used).split(' ')[1] || '' }}</span></div>
              <div class="text-xs text-[var(--jc-text-muted)] mt-1">{{ t('device.mem.used') }} / {{ formatSize(mem.total) }}</div>
            </div>
          </div>
          <div class="h-3 rounded-full bg-[var(--jc-bg)] overflow-hidden">
            <div class="h-full rounded-full transition-all duration-500" :style="{ width: memPercent + '%', background: 'linear-gradient(90deg, #10b981, #34d399)' }"></div>
          </div>
          <div class="grid grid-cols-2 gap-3 text-sm">
            <div class="flex justify-between p-2 bg-[var(--jc-bg-hover)] rounded">
              <span class="text-[var(--jc-text-muted)]">{{ t('device.mem.used') }}</span>
              <span class="text-emerald-400 font-semibold">{{ formatSize(mem.used) }}</span>
            </div>
            <div class="flex justify-between p-2 bg-[var(--jc-bg-hover)] rounded">
              <span class="text-[var(--jc-text-muted)]">{{ t('device.mem.available') }}</span>
              <span class="text-green-400 font-semibold">{{ formatSize(mem.available) }}</span>
            </div>
            <div class="flex justify-between p-2 bg-[var(--jc-bg-hover)] rounded">
              <span class="text-[var(--jc-text-muted)]">{{ t('device.mem.cached') }}</span>
              <span class="text-blue-400 font-semibold">{{ formatSize(mem.cached) }}</span>
            </div>
            <div class="flex justify-between p-2 bg-[var(--jc-bg-hover)] rounded">
              <span class="text-[var(--jc-text-muted)]">{{ t('device.mem.swapUsed') }}</span>
              <span class="text-amber-400 font-semibold">{{ formatSize(mem.swapUsed) }}</span>
            </div>
            <div class="flex justify-between p-2 bg-[var(--jc-bg-hover)] rounded">
              <span class="text-[var(--jc-text-muted)]">{{ t('device.mem.swapTotal') }}</span>
              <span class="text-[var(--jc-text-soft)] font-semibold">{{ formatSize(mem.swapTotal) }}</span>
            </div>
          </div>
          <div class="flex items-center justify-center gap-2 text-xs">
            <span class="text-[var(--jc-text-soft)]">{{ t('device.processView') }}:</span>
            <el-button size="small" text @click="layout.toggleMemProcessView()">
              <el-icon class="mr-1"><component :is="layout.memProcessView ? DataLine : List" /></el-icon>
              {{ layout.memProcessView ? t('device.view.backSystem') : t('device.view.process') }}
            </el-button>
          </div>
          <ProcessView v-if="layout.memProcessView" :rows="processes" sort-key="mem" :hide-cpu="true" @kill="killProcess" @refresh="regenProcesses" />
        </div>
      </template>
      <template v-else-if="currentDetailCard === 'disk'">
        <div class="space-y-4">
          <div class="flex items-center justify-center gap-4">
            <div class="text-center">
              <div class="text-5xl font-bold" :class="diskUsageClass">{{ formatSize(disk.used).split(' ')[0] }} <span class="text-2xl text-[var(--jc-text-soft)]">{{ formatSize(disk.used).split(' ')[1] || '' }}</span></div>
              <div class="text-xs text-[var(--jc-text-muted)] mt-1">{{ t('device.mem.used') }} / {{ formatSize(disk.total) }}</div>
            </div>
          </div>
          <div class="h-3 rounded-full bg-[var(--jc-bg)] overflow-hidden">
            <div class="h-full rounded-full transition-all duration-500" :style="{ width: diskPercent + '%', background: diskGradient }"></div>
          </div>
          <div>
            <div class="text-xs text-[var(--jc-text-muted)] mb-2">{{ t('device.disk.partitions') }}</div>
            <div class="space-y-2">
              <div v-for="p in disk.partitions" :key="p.mount" class="p-2 bg-[var(--jc-bg-hover)] rounded">
                <div class="flex justify-between text-sm mb-1">
                  <span class="text-[var(--jc-text-soft)] font-mono">{{ p.mount }}</span>
                  <span class="text-[var(--jc-text-muted)]">{{ formatSize(p.used) }} / {{ formatSize(p.total) }} ({{ p.total > 0 ? ((p.used / p.total) * 100).toFixed(0) : 0 }}%)</span>
                </div>
                <div class="h-1.5 rounded-full bg-[var(--jc-bg)] overflow-hidden">
                  <div class="h-full rounded-full bg-purple-500" :style="{ width: (p.used / p.total * 100) + '%' }"></div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </template>
      <template v-else-if="currentDetailCard === 'net'">
        <div class="space-y-4">
          <div class="flex justify-around">
            <div class="text-center p-4 bg-[var(--jc-bg-hover)] rounded-lg min-w-[120px]">
              <el-icon size="28" color="var(--jc-success)"><Bottom /></el-icon>
              <div class="text-3xl font-bold text-green-400 mt-2">{{ formatSize(net.rxRate) }}/s</div>
              <div class="text-xs text-[var(--jc-text-muted)]">{{ t('device.net.down') }}</div>
            </div>
            <div class="text-center p-4 bg-[var(--jc-bg-hover)] rounded-lg min-w-[120px]">
              <el-icon size="28" color="var(--jc-warn)"><Top /></el-icon>
              <div class="text-3xl font-bold text-amber-400 mt-2">{{ formatSize(net.txRate) }}/s</div>
              <div class="text-xs text-[var(--jc-text-muted)]">{{ t('device.net.up') }}</div>
            </div>
          </div>
          <div class="grid grid-cols-2 gap-3 text-sm">
            <div class="flex justify-between p-2 bg-[var(--jc-bg-hover)] rounded">
              <span class="text-[var(--jc-text-muted)]">{{ t('device.net.downTotal') }}</span>
              <span class="text-green-400 font-semibold">{{ formatSize(net.rxRate * 3600) }}/h</span>
            </div>
            <div class="flex justify-between p-2 bg-[var(--jc-bg-hover)] rounded">
              <span class="text-[var(--jc-text-muted)]">{{ t('device.net.upTotal') }}</span>
              <span class="text-amber-400 font-semibold">{{ formatSize(net.txRate * 3600) }}/h</span>
            </div>
            <div class="flex justify-between p-2 bg-[var(--jc-bg-hover)] rounded">
              <span class="text-[var(--jc-text-muted)]">{{ t('device.net.activeConn') }}</span>
              <span class="text-[var(--jc-text-soft)] font-semibold">24</span>
            </div>
            <div class="flex justify-between p-2 bg-[var(--jc-bg-hover)] rounded">
              <span class="text-[var(--jc-text-muted)]">{{ t('device.net.interface') }}</span>
              <span class="text-[var(--jc-text-soft)] font-semibold">eth0</span>
            </div>
          </div>
        </div>
      </template>
      <template v-else-if="currentDetailCard === 'aiAssistant'">
        <AiAssistantCard detailed />
      </template>
      <template v-else-if="currentDetailCard === 'proc'">
        <div class="space-y-4">
          <div class="grid grid-cols-2 gap-3 text-sm">
            <div class="flex justify-between p-2 bg-[var(--jc-bg-hover)] rounded">
              <span class="text-[var(--jc-text-muted)]">进程总数</span>
              <span class="text-[var(--jc-text-soft)] font-semibold">{{ processes.length }}</span>
            </div>
            <div class="flex justify-between p-2 bg-[var(--jc-bg-hover)] rounded">
              <span class="text-[var(--jc-text-muted)]">总 CPU 使用率</span>
              <span class="text-red-400 font-semibold">{{ cpu.usage.toFixed(1) }}%</span>
            </div>
          </div>
          <ProcessView :rows="processes" sort-key="cpu" show-ports @kill="killProcess" @refresh="regenProcesses" />
        </div>
      </template>
    </el-dialog>
  </div>
</template>

<script setup lang="ts">
/**
 * 设备信息面板
 * 作者：JC
 * 卡片支持：拖拽排序（原生 HTML5 拖拽）、显隐设置、单卡折叠/展开、一键全展开折叠。
 */
import { computed, ref, onMounted, onBeforeUnmount, onActivated, onDeactivated, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import {
  Monitor, Connection, Setting, Rank, ArrowDown, ArrowRight, Expand, Fold, Bottom, Top,
  Grid, Operation, List, DataLine, FullScreen
} from '@element-plus/icons-vue'
import PanelHeader from '@/components/common/PanelHeader.vue'
import { isMobileWebview } from '@/utils/platform'
import AdaptiveToolbar from '@/components/common/AdaptiveToolbar.vue'
import type { ToolbarButton } from '@/components/common/AdaptiveToolbar.vue'
import DeviceCardSettings from './DeviceCardSettings.vue'
import ProcessView, { type ProcessInfo } from './ProcessView.vue'
import AiAssistantCard from './AiAssistantCard.vue'
import { useLayoutStore, type DeviceCardId } from '@/stores/layout'
import { useAgentStore } from '@/stores/agent'
import { useI18nStore } from '@/stores/i18n'
import type { Dock } from '@/stores/layout'
import { useDragSort } from '@/composables/useDragSort'
import { formatSize } from '@/utils/format'
import { isUiFrozen } from '@/utils/uiFreeze'

interface Props {
  connected?: boolean
  hostname?: string
  sessionId?: string
  /** 所在 dock 方向（决定头部按钮排布） */
  dock?: Dock
  /** 是否折叠 */
  collapsed?: boolean
  /** 会话类型：local 走本机信息命令，ssh 走 exec_remote_command */
  kind?: 'local' | 'ssh'
}
const props = withDefaults(defineProps<Props>(), { connected: false, hostname: '', sessionId: '', dock: 'right', collapsed: false, kind: 'ssh' })
defineEmits<{ (e: 'collapse'): void }>()

const layout = useLayoutStore()
const agentStore = useAgentStore()
const i18n = useI18nStore()
const t = (k: string) => i18n.t(k)
const settingsOpen = ref(false)
/** 是否移动端（平台启动即固定）：决定工具栏按钮集合等移动端差异 */
const isMobile = isMobileWebview()

// ===== Toolbar Buttons (Adaptive) =====
/** 移动端工具栏只保留这两个：一键展开/折叠 + 设置。
 *  视图切换（列表/网格）在手机上一屏只有一列，切换没有意义，去掉以免挤占头部 */
const MOBILE_TOOLBAR_KEYS = ['toggle-collapse', 'settings']
const toolbarButtons = computed<ToolbarButton[]>(() => {
  const buttons: ToolbarButton[] = [
    {
      key: 'view-list',
      icon: Operation,
      tooltip: t('device.toolbar.list'),
      active: layout.deviceViewMode === 'list',
      onClick: () => layout.setDeviceViewMode('list')
    },
    {
      key: 'view-grid',
      icon: Grid,
      tooltip: t('device.toolbar.grid'),
      active: layout.deviceViewMode === 'grid',
      onClick: () => layout.setDeviceViewMode('grid')
    },
    {
      key: 'toggle-collapse',
      icon: allCollapsed.value ? Expand : Fold,
      tooltip: allCollapsed.value ? t('device.toolbar.expandAll') : t('device.toolbar.collapseAll'),
      onClick: () => layout.setAllCollapsed(!allCollapsed.value)
    },
    {
      key: 'settings',
      icon: Setting,
      tooltip: t('device.toolbar.settings'),
      onClick: () => settingsOpen.value = true
    }
  ]
  return isMobile ? buttons.filter(b => MOBILE_TOOLBAR_KEYS.includes(b.key)) : buttons
})

const titles: Record<DeviceCardId, string> = {
  system: t('device.card.system'), cpu: 'CPU', mem: t('device.card.mem'), disk: t('device.card.disk'), net: t('device.card.net'), proc: '进程', aiAssistant: 'AI 助手'
}

/** 仅可见子集（渲染用）；拖拽排序通过 useDragSort 提交新顺序 */
const visibleCards = computed(() => layout.deviceCards.filter(c => c.visible))

/** 把可见卡片新顺序与隐藏卡片合并成完整顺序写回 store */
function commitVisibleOrder(visibleIds: DeviceCardId[]): void {
  const hidden = layout.deviceCards.filter(c => !c.visible).map(c => c.id)
  layout.reorderCards([...visibleIds, ...hidden])
}

const { dragIndex, startDrag } = useDragSort((from, to) => {
  const ids = visibleCards.value.map(c => c.id)
  const [moved] = ids.splice(from, 1)
  ids.splice(to, 0, moved)
  commitVisibleOrder(ids)
})

const allCollapsed = computed(() => layout.deviceCards.every(c => c.collapsed))

/** 所有容量类数据统一以字节存储，展示统一使用 utils/format 的 formatSize（B/KB/MB/GB/TB/PB 自适应） */

// ===== 放大展示 Modal =====
const detailModalOpen = ref(false)
const currentDetailCard = ref<DeviceCardId | null>(null)

function openDetail(id: DeviceCardId) {
  currentDetailCard.value = id
  detailModalOpen.value = true
}

function closeDetail() {
  detailModalOpen.value = false
  currentDetailCard.value = null
}

const cpuUsageClass = computed(() => {
  const u = cpu.value.usage
  if (u > 80) return 'text-red-400'
  if (u > 60) return 'text-amber-400'
  return 'text-blue-400'
})

const diskUsageClass = computed(() => {
  const p = diskPercent.value
  if (p > 85) return 'text-red-400'
  if (p > 70) return 'text-amber-400'
  return 'text-purple-400'
})

// ===== 数据（初始为空，连接后通过 SSH 获取真实数据）=====
const meta = ref({
  hostname: '', os: '', kernel: '',
  uptime: '', cpuModel: '', cpuCores: 0, ip: ''
})
const cpu = ref({ usage: 0, perCore: [] as number[], loadAvg: [0, 0, 0] as number[] })
const mem = ref({ total: 0, used: 0, cached: 0, available: 0, swapTotal: 0, swapUsed: 0 })
const disk = ref({
  total: 0, used: 0,
  partitions: [] as { mount: string; total: number; used: number }[]
})
const net = ref({ rxRate: 0, txRate: 0 })

async function loadRemoteInfo() {
  if (!props.sessionId) return
  try {
    // 系统信息
    const sysOut = await invoke<string>('exec_remote_command', {
      dto: { session_id: props.sessionId, command: 'hostname 2>/dev/null; echo "---UNAME---"; uname -sr 2>/dev/null; echo "---CPUINFO---"; grep "model name" /proc/cpuinfo 2>/dev/null | head -1; echo "---NPROC---"; nproc 2>/dev/null; echo "---UPTIME---"; uptime -p 2>/dev/null || uptime 2>/dev/null; echo "---IP---"; hostname -I 2>/dev/null || ip -br addr 2>/dev/null | head -3' }
    })
    parseSystemInfo(sysOut)

    // CPU 使用率：/proc/stat 双采样（间隔 1s），得到准确的瞬时总使用率与每核使用率
    const cpuOut = await invoke<string>('exec_remote_command', {
      dto: { session_id: props.sessionId, command: "grep '^cpu' /proc/stat; echo '---SAMPLE---'; sleep 1; grep '^cpu' /proc/stat; echo '---LOAD---'; cat /proc/loadavg" }
    })
    parseCpuInfo(cpuOut)

    // 内存（字节）
    const memOut = await invoke<string>('exec_remote_command', {
      dto: { session_id: props.sessionId, command: 'free -b' }
    })
    parseMemInfo(memOut)

    // 磁盘（字节）
    const diskOut = await invoke<string>('exec_remote_command', {
      dto: { session_id: props.sessionId, command: 'df -B1 -x tmpfs -x devtmpfs -x squashfs 2>/dev/null || df -B1' }
    })
    parseDiskInfo(diskOut)

    // 网络速率
    const netOut = await invoke<string>('exec_remote_command', {
      dto: { session_id: props.sessionId, command: "cat /proc/net/dev | grep -E 'eth|ens|enp' | head -5" }
    })
    parseNetInfo(netOut)
  } catch (e) {
    console.error('[device] load failed:', e)
  }
}

// ===== 本地终端：直读本机系统信息（后端 sysinfo 命令，不依赖 SSH） =====
interface LocalSystemInfo { hostname: string; os: string; kernel: string; uptime: string; cpuModel: string; cpuCores: number; ip: string }
interface LocalCpuInfo { usage: number; perCore: number[]; loadAvg: [number, number, number] }
interface LocalMemInfo { total: number; used: number; available: number; cached: number; swapTotal: number; swapUsed: number }
interface LocalDiskInfo { total: number; used: number; partitions: { mount: string; total: number; used: number }[] }
interface LocalNetInfo { rxRate: number; txRate: number }

async function loadLocalInfo() {
  try {
    const [sys, cpuInfo, memInfo, diskInfo, netInfo] = await Promise.all([
      invoke<LocalSystemInfo>('get_local_system_info'),
      invoke<LocalCpuInfo>('get_local_cpu_info'),
      invoke<LocalMemInfo>('get_local_mem_info'),
      invoke<LocalDiskInfo>('get_local_disk_info'),
      invoke<LocalNetInfo>('get_local_net_rate')
    ])
    Object.assign(meta.value, {
      hostname: sys.hostname, os: sys.os, kernel: sys.kernel, uptime: sys.uptime,
      cpuModel: sys.cpuModel, cpuCores: sys.cpuCores, ip: sys.ip
    })
    cpu.value.usage = cpuInfo.usage
    cpu.value.perCore = cpuInfo.perCore
    cpu.value.loadAvg = cpuInfo.loadAvg
    Object.assign(mem.value, memInfo)
    disk.value.total = diskInfo.total
    disk.value.used = diskInfo.used
    disk.value.partitions = diskInfo.partitions
    net.value.rxRate = netInfo.rxRate
    net.value.txRate = netInfo.txRate
  } catch (e) {
    console.error('[device] local load failed:', e)
  }
}

function parseSystemInfo(out: string) {
  const lines = out.split('\n')
  let section = ''
  let hostname = ''
  for (const l of lines) {
    const line = l.trim()
    if (!line) continue
    if (line === '---UNAME---') { section = 'uname'; continue }
    if (line === '---CPUINFO---') { section = 'cpu'; continue }
    if (line === '---NPROC---') { section = 'nproc'; continue }
    if (line === '---UPTIME---') { section = 'uptime'; continue }
    if (line === '---IP---') { section = 'ip'; continue }
    if (!section) { hostname = line; section = ''; continue }
    switch (section) {
      case 'uname':
        meta.value.os = line
        section = ''
        break
      case 'cpu':
        meta.value.cpuModel = line.split(':').slice(1).join(':').trim() || line
        section = ''
        break
      case 'nproc':
        meta.value.cpuCores = parseInt(line) || 0
        section = ''
        break
      case 'uptime':
        meta.value.uptime = line
        section = ''
        break
      case 'ip':
        meta.value.ip = line.split(/\s+/).find(w => w.match(/^\d+\./)) || line
        section = ''
        break
    }
  }
  meta.value.hostname = hostname || meta.value.hostname
}

/** /proc/stat 单行采样：user nice system idle iowait irq softirq steal */
interface StatSample { idle: number; total: number }

function parseStatCpuLines(text: string): Map<string, StatSample> {
  const map = new Map<string, StatSample>()
  for (const raw of text.split('\n')) {
    const m = raw.trim().match(/^(cpu\d*)\s+(.*)$/)
    if (!m) continue
    const nums = m[2].split(/\s+/).map(Number).filter(n => !isNaN(n))
    if (nums.length < 5) continue
    const idle = (nums[3] ?? 0) + (nums[4] ?? 0)
    const total = nums.slice(0, 8).reduce((s, n) => s + n, 0)
    map.set(m[1], { idle, total })
  }
  return map
}

/** 由两次采样差值计算使用率（%），无效返回 null */
function usageBetween(a?: StatSample, b?: StatSample): number | null {
  if (!a || !b) return null
  const dt = b.total - a.total
  const di = b.idle - a.idle
  if (dt <= 0 || di < 0) return null
  return Math.max(0, Math.min(100, Math.round((1 - di / dt) * 1000) / 10))
}

function parseCpuInfo(out: string) {
  const cleaned = out.replace(/\r/g, '')
  const loadSplit = cleaned.split('---LOAD---')
  const samples = (loadSplit[0] ?? '').split('---SAMPLE---')
  const before = parseStatCpuLines(samples[0] ?? '')
  const after = parseStatCpuLines(samples[1] ?? '')

  // 负载：/proc/loadavg 前三个数
  const lp = (loadSplit[1] ?? '').trim().split(/\s+/)
  if (lp.length >= 3) {
    cpu.value.loadAvg = [parseFloat(lp[0]) || 0, parseFloat(lp[1]) || 0, parseFloat(lp[2]) || 0]
  }

  // 总使用率
  const agg = usageBetween(before.get('cpu'), after.get('cpu'))
  if (agg !== null) cpu.value.usage = agg

  // 每核使用率（同时以 cpuN 行数修正核心数）
  const coreKeys = [...after.keys()]
    .filter(k => k !== 'cpu')
    .sort((a, b) => parseInt(a.slice(3)) - parseInt(b.slice(3)))
  if (coreKeys.length > 0) {
    meta.value.cpuCores = coreKeys.length
    cpu.value.perCore = coreKeys.map(k => usageBetween(before.get(k), after.get(k)) ?? 0)
  }
}

function parseMemInfo(out: string) {
  const lines = out.split('\n')
  for (const l of lines) {
    if (l.startsWith('Mem:')) {
      const parts = l.trim().split(/\s+/)
      if (parts.length >= 4) {
        // free -b 返回字节：total used free shared buff/cache available
        mem.value.total = Number(parts[1]) || 0
        mem.value.used = Number(parts[2]) || 0
        mem.value.cached = Number(parts[5]) || 0
        mem.value.available = Number(parts[6]) || Math.max(0, mem.value.total - mem.value.used)
      }
    }
    if (l.startsWith('Swap:')) {
      const parts = l.trim().split(/\s+/)
      if (parts.length >= 3) {
        mem.value.swapTotal = Number(parts[1]) || 0
        mem.value.swapUsed = Number(parts[2]) || 0
      }
    }
  }
}

function parseDiskInfo(out: string) {
  const lines = out.split('\n').filter(l => l.trim() && !l.startsWith('Filesystem'))
  const partitions: { mount: string; total: number; used: number }[] = []
  let total = 0, used = 0
  for (const l of lines) {
    const parts = l.split(/\s+/)
    // df -B1：1B-blocks Used Available Use% Mounted on（均为纯数字字节）
    if (parts.length >= 6 && /^\d+$/.test(parts[1]) && /^\d+$/.test(parts[2])) {
      const size = Number(parts[1])
      if (size <= 0) continue
      const usedBytes = Number(parts[2])
      const mount = parts[5]
      partitions.push({ mount, total: size, used: usedBytes })
      if (mount === '/') {
        total = size
        used = usedBytes
      }
    }
  }
  if (total === 0 && partitions.length > 0) {
    total = partitions.reduce((s, p) => s + p.total, 0)
    used = partitions.reduce((s, p) => s + p.used, 0)
  }
  disk.value.total = total
  disk.value.used = used
  disk.value.partitions = partitions
}

// 网络速率采样历史
let netLastRx = 0, netLastTx = 0, netLastTime = 0

function parseNetInfo(out: string) {
  const lines = out.split('\n').filter(l => l.trim())
  let rx = 0, tx = 0
  for (const l of lines) {
    const parts = l.split(/\s+/)
    if (parts.length >= 10) {
      rx += parseInt(parts[1]) || 0
      tx += parseInt(parts[9]) || 0
    }
  }
  const now = Date.now()
  if (netLastTime > 0) {
    const dt = (now - netLastTime) / 1000 // seconds
    if (dt > 0) {
      // 字节/秒，展示时由 formatSize 自适应单位
      net.value.rxRate = Math.max(0, Math.round((rx - netLastRx) / dt))
      net.value.txRate = Math.max(0, Math.round((tx - netLastTx) / dt))
    }
  }
  netLastRx = rx
  netLastTx = tx
  netLastTime = now
}

// ===== 进程数据（暂用 mock，后续改为真实 ps 命令）=====
const PROC_NAMES = [
  'systemd', 'kthreadd', 'sshd', 'bash', 'vim', 'node', 'python3', 'nginx', 'redis-server',
  'mysqld', 'dockerd', 'containerd', 'java', 'go-build', 'rust-analyzer', 'chrome', 'code-server',
  'cron', 'rsyslogd', 'dbus-daemon', 'snapd', 'unattended-upgrade', 'polkitd', 'agetty'
]
const PROC_PATHS: Record<string, string> = {
  systemd: '/usr/lib/systemd/systemd', sshd: '/usr/sbin/sshd', bash: '/usr/bin/bash',
  node: '/usr/local/bin/node', python3: '/usr/bin/python3', nginx: '/usr/sbin/nginx',
  mysqld: '/usr/sbin/mysqld', dockerd: '/usr/bin/dockerd', java: '/opt/java/bin/java',
  'rust-analyzer': '/root/.cargo/bin/rust-analyzer', 'code-server': '/usr/bin/code-server'
}
const processes = ref<ProcessInfo[]>([])

/** 进程名 → 可能监听的端口（mock，真实 SSH 数据由后端提供） */
const PROC_PORTS: Record<string, number[]> = {
  sshd: [22], nginx: [80, 443], mysqld: [3306], dockerd: [2375],
  python3: [8000], java: [8080, 8443], 'code-server': [8080], redis: [6379],
  node: [3000, 5173], httpd: [80, 443]
}
function regenProcesses() {
  const basePid = 100
  processes.value = PROC_NAMES.slice(0, 16).map((name, i) => {
    // 内存百分比：普通进程 0.1%-5%，nginx/mysqld/java 稍大 5%-15%
    const heavy = ['mysqld', 'java', 'dockerd', 'redis', 'node'].includes(name)
    const memPct = heavy ? Math.random() * 15 + 1 : Math.random() * 5 + 0.1
    // 内存 MB：假设总内存 32GB
    const memBytes = Math.round(memPct * 320)
    return {
      pid: basePid + i * 7,
      name,
      path: PROC_PATHS[name] ?? `/usr/local/sbin/${name}`,
      cpu: Math.random() * 40,
      mem: memPct,
      memBytes,
      ports: PROC_PORTS[name]
    }
  })
}

function killProcess(pid: number) {
  processes.value = processes.value.filter(p => p.pid !== pid)
  setTimeout(() => {
    if (processes.value.length < 16) {
      const name = PROC_NAMES[Math.floor(Math.random() * PROC_NAMES.length)]
      processes.value.push({
        pid: 5000 + Math.floor(Math.random() * 9999),
        name,
        path: PROC_PATHS[name] ?? `/usr/local/sbin/${name}`,
        cpu: Math.random() * 30,
        mem: Math.random() * 20
      })
    }
  }, 800)
}

const memPercent = computed(() => mem.value.total > 0 ? (mem.value.used / mem.value.total) * 100 : 0)
const diskPercent = computed(() => disk.value.total > 0 ? (disk.value.used / disk.value.total) * 100 : 0)

const cpuGradient = computed(() => {
  const u = cpu.value.usage
  if (u > 80) return 'linear-gradient(90deg, #ef4444, #f87171)'
  if (u > 60) return 'linear-gradient(90deg, #f59e0b, #fbbf24)'
  return 'linear-gradient(90deg, #3b82f6, #60a5fa)'
})
const diskGradient = computed(() => {
  const p = diskPercent.value
  if (p > 85) return 'linear-gradient(90deg, #ef4444, #f87171)'
  if (p > 70) return 'linear-gradient(90deg, #f59e0b, #fbbf24)'
  return 'linear-gradient(90deg, #8b5cf6, #a78bfa)'
})

/** 头部数值预览（展开/折叠均显示） */
function summary(id: DeviceCardId): string {
  switch (id) {
    case 'system': return meta.value.hostname
    case 'cpu': return cpu.value.usage.toFixed(1) + '%'
    case 'mem': return formatSize(mem.value.used) + ' / ' + formatSize(mem.value.total)
    case 'disk': return formatSize(disk.value.used) + ' / ' + formatSize(disk.value.total)
    case 'net': return '↓' + formatSize(net.value.rxRate) + '/s ↑' + formatSize(net.value.txRate) + '/s'
    case 'proc': return processes.value.length + ' 个'
    case 'aiAssistant': return agentStore.balance !== null ? `积分 ${agentStore.balance}` : ''
  }
}
function summaryClass(id: DeviceCardId): string {
  if (id === 'cpu') {
    const u = cpu.value.usage
    if (u > 80) return 'text-red-400'
    if (u > 60) return 'text-amber-400'
    return 'text-green-400'
  }
  return 'text-[var(--jc-text-soft)]'
}
/** 折叠态第二行摘要 */
function mini(id: DeviceCardId): string {
  switch (id) {
    case 'system': return `${meta.value.os} · ${meta.value.ip}`
    case 'cpu': return `${meta.value.cpuCores} ${t('device.meta.coreUnit')} · ${t('device.cpu.load')} ${cpu.value.loadAvg[0].toFixed(2)}`
    case 'mem': return `${t('device.mem.used')} ${formatSize(mem.value.used)} · ${t('device.mem.available')} ${formatSize(mem.value.available)}`
    case 'disk': return `${t('device.mem.used')} ${diskPercent.value.toFixed(0)}% · ${disk.value.partitions.length} ${t('device.disk.partitions')}`
    case 'net': return `${t('device.net.down')} ${formatSize(net.value.rxRate)}/s · ${t('device.net.up')} ${formatSize(net.value.txRate)}/s`
    case 'proc': return `CPU ${cpu.value.usage.toFixed(0)}% · ${processes.value.length} 进程`
    // 折叠态沿用「可用积分」口径(永久 + 限时), 与卡片内「积分余额」(仅永久)区分开
    case 'aiAssistant': return agentStore.balance !== null ? `可用积分 ${agentStore.balance}` : '登录后查看积分与用量'
  }
}

// ===== 实时数据模拟（仅未连接时的演示模式；容量一律按字节） =====
const GB = 1024 * 1024 * 1024
let timer: ReturnType<typeof setInterval> | null = null
function startMockUpdates() {
  stopMockUpdates()
  if (!processes.value.length) regenProcesses()
  timer = setInterval(() => {
    cpu.value.usage = Math.max(5, Math.min(95, cpu.value.usage + (Math.random() - 0.5) * 15))
    cpu.value.perCore = cpu.value.perCore.map(() => Math.max(2, Math.min(99, Math.random() * 100)))
    cpu.value.loadAvg = cpu.value.loadAvg.map(v => Math.max(0.01, Math.min(4, v + (Math.random() - 0.5) * 0.3)))
    if (mem.value.total > 0) {
      mem.value.used = Math.max(GB, Math.min(mem.value.total - GB, mem.value.used + (Math.random() - 0.5) * 0.5 * GB))
      mem.value.cached = Math.max(0.5 * GB, Math.min(6 * GB, mem.value.cached + (Math.random() - 0.5) * 0.3 * GB))
      mem.value.available = Math.max(0, mem.value.total - mem.value.used)
      mem.value.swapUsed = Math.max(0, Math.min(mem.value.swapTotal, mem.value.swapUsed + (Math.random() - 0.5) * 0.1 * GB))
    }
    net.value.rxRate = Math.max(0, Math.min(500 * 1024, net.value.rxRate + (Math.random() - 0.5) * 50 * 1024))
    net.value.txRate = Math.max(0, Math.min(300 * 1024, net.value.txRate + (Math.random() - 0.5) * 30 * 1024))
    // 进程资源抖动
    processes.value.forEach(p => {
      p.cpu = Math.max(0, Math.min(99, p.cpu + (Math.random() - 0.5) * 8))
      p.mem = Math.max(0, Math.min(99, p.mem + (Math.random() - 0.5) * 5))
    })
  }, 2000)
}
function stopMockUpdates() { if (timer) { clearInterval(timer); timer = null } }

function regenByHost(name: string) {
  if (!name) return
  meta.value.hostname = name
  let h = 0
  for (const c of name) h = (h * 31 + c.charCodeAt(0)) | 0
  const seed = Math.abs(h)
  meta.value.ip = `192.168.${(seed % 200) + 1}.${(seed % 254) + 1}`
  meta.value.cpuCores = [4, 8, 16][seed % 3]
  cpu.value.perCore = Array(meta.value.cpuCores).fill(0).map(() => Math.random() * 80)
  mem.value.total = [8, 16, 32, 64][seed % 4] * GB
  mem.value.used = mem.value.total * 0.4
  mem.value.cached = 2 * GB
  mem.value.available = mem.value.total - mem.value.used
  mem.value.swapTotal = 4 * GB
  mem.value.swapUsed = 0.5 * GB
  disk.value.total = [100, 200, 500, 1000][seed % 4] * GB
  disk.value.partitions.forEach(p => { p.total = disk.value.total / 4; p.used = p.total * (0.3 + Math.random() * 0.5) })
  disk.value.used = disk.value.partitions.reduce((s, p) => s + p.used, 0)
}

// 演示数据仅在未连接时生成，避免覆盖真实 SSH 数据
watch(() => props.hostname, v => { if (v && !props.connected) regenByHost(v) })
let refreshTimer: ReturnType<typeof setInterval> | null = null
/** 启动真实数据刷新：连接时加载数据 + 5s 定时刷新；未连接走演示数据 */
async function startRealtimeRefresh() {
  const isLocal = props.kind === 'local'
  // 本地会话不依赖 sessionId 之外的 SSH 上下文；SSH 会话需已连接且有会话
  if (props.connected && (isLocal || props.sessionId)) {
    stopMockUpdates()
    const load = isLocal ? loadLocalInfo : loadRemoteInfo
    await load()
    if (refreshTimer) clearInterval(refreshTimer)
    refreshTimer = setInterval(() => {
      // 窗口拖拽中跳过：SSH 采样 + 图表重渲染会阻塞窗口移动，造成掉帧
      if (!isUiFrozen()) void load()
    }, 5000)
  } else {
    stopRealtimeRefresh()
    stopMockUpdates()
    startMockUpdates()
  }
}
/** 停止真实数据定时刷新 */
function stopRealtimeRefresh() {
  if (refreshTimer) { clearInterval(refreshTimer); refreshTimer = null }
}
watch(() => [props.connected, props.sessionId, props.kind] as const, startRealtimeRefresh, { immediate: true })
onMounted(() => {})
// keep-alive 缓存：切走暂停定时刷新省资源，切回恢复（顺带拿最新数据）
onActivated(startRealtimeRefresh)
onDeactivated(() => { stopRealtimeRefresh(); stopMockUpdates() })
onBeforeUnmount(() => { stopRealtimeRefresh(); stopMockUpdates() })
</script>

<style scoped>
.cards-container.list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.cards-container.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
  gap: 8px;
}
.card-handle {
  cursor: grab;
  color: var(--jc-text-muted);
  display: inline-flex;
  padding: 2px;
  border-radius: 3px;
  transition: color 0.15s, background-color 0.15s;
}
.card-handle:hover {
  color: var(--jc-accent);
  background-color: color-mix(in srgb, var(--jc-accent) 12%, transparent);
}
.card-handle:active { cursor: grabbing; }
.card { transition: border-color 0.15s, opacity 0.15s; }
.card:hover { border-color: var(--jc-border); }
.card--dragging { opacity: 0.5; border-style: dashed; }
</style>

<style>
.device-detail-dialog.el-dialog {
  background-color: var(--jc-bg-panel) !important;
  border: 1px solid var(--jc-border);
}
.device-detail-dialog .el-dialog__header {
  border-bottom: 1px solid var(--jc-border);
  margin-right: 0;
  padding: 12px 16px;
}
.device-detail-dialog .el-dialog__title {
  color: var(--jc-text);
  font-weight: 600;
}
.device-detail-dialog .el-dialog__body {
  padding: 16px;
  color: var(--jc-text-soft);
}
.device-detail-dialog .el-dialog__headerbtn .el-dialog__close {
  color: var(--jc-text-muted);
}
.device-detail-dialog .el-dialog__headerbtn:hover .el-dialog__close {
  color: var(--jc-text);
}
</style>
