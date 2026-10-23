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
    width="100%"
    append-to-body
    class="jc-dialog fwd-dialog fwd-config-dialog"
    @update:model-value="(v: boolean) => emit('update:modelValue', v)"
  >
    <!-- 标题行：标题 + 模式 + 规则名称（并入弹窗头部，避免多占一行纵向空间） -->
    <template #header>
      <div class="cfg-head">
        <span class="cfg-title">{{ isNew ? t('portForward.create.title') : t('portForward.edit.title') }}</span>
        <span class="cfg-mode">{{ t(`portForward.mode.${form.mode}.title`) }}</span>
        <el-input
          v-model="form.name"
          size="small"
          class="cfg-name"
          :placeholder="t('portForward.field.name')"
        />
      </div>
    </template>

    <!-- 页面式：所有配置项一次填完（配置项内嵌在架构图对应节点上） -->
    <div v-if="!isWizard" class="cfg-body">
      <ForwardDiagram ref="pageDiagRef" :mode="form.mode" :form="form" :apply="apply" :remote-focus="remoteConfiguring ? rcFocus : null" :dyn-target-open="dynTargetOpen" @pick-session="openPicker" @edit-remote-client="onEditRemoteClient" @dyn-close="dynConfiguring = false" />
      <!-- 远程模式「测试连通性」未配置客户端时，进入两台远程客户端的 SSH 配置面板。
           Teleport 到 body + fixed 遮罩：上下占满整个窗口，遮住底部「取消/保存/确定」，
           避免误以为那三个按钮是操作本面板的 -->
      <Teleport to="body">
        <Transition name="sp-slide">
          <div v-if="remoteConfiguring && form.mode === 'remote'" class="rc-mask" @click.self="remoteConfiguring = false">
            <div class="rcfg-panel" @click.stop>
              <div class="rcfg-head">
                <span>{{ t('portForward.remoteClient.title') }}</span>
                <el-button text size="small" @click="remoteConfiguring = false">{{ t('portForward.btn.cancel') }}</el-button>
              </div>
              <div v-if="remoteClientRightStale" class="rcfg-stale">{{ t('portForward.remoteClient.rightStale') }}</div>
              <div class="rcfg-grid">
                <section
                  class="rcfg-card"
                  :class="{ 'is-active': rcFocus === 'left', 'is-ok': clientTest.left === 'ok', 'is-fail': clientTest.left === 'fail' }"
                >
                  <h4>{{ t('portForward.remoteClient.left') }}</h4>
                  <RemoteClientForm
                    :model="form.remoteClientLeft || defaultClient()"
                    :bind-addr="'127.0.0.1'"
                    :remote-port="form.remotePort || 0"
                    @update="(p) => applyClient('left', p)"
                    @test="clientTest.left = $event"
                    @pick="openPicker('left')"
                    @focus="rcFocus = 'left'"
                    @blur="rcFocus = null"
                  />
                </section>
                <section
                  class="rcfg-card"
                  :class="{ 'is-active': rcFocus === 'right', 'is-ok': clientTest.right === 'ok', 'is-fail': clientTest.right === 'fail' }"
                >
                  <h4>{{ t('portForward.remoteClient.right') }}</h4>
                  <RemoteClientForm
                    :model="rightClientModel"
                    :bind-addr="(form.bindAddr || '').trim()"
                    :remote-port="form.remotePort || 0"
                    @update="(p) => applyClient('right', p)"
                    @test="clientTest.right = $event"
                    @pick="openPicker('right')"
                    @focus="rcFocus = 'right'"
                    @blur="rcFocus = null"
                  />
                </section>
              </div>
              <div class="rcfg-foot">
                <span>{{ t('portForward.remoteClient.configured') }}</span>
              </div>
            </div>
          </div>
        </Transition>

        <!-- 选择 SSH 会话：从右侧滑出的会话列表面板（样式照搬终端页「会话」窗口）。
             与 rc-mask 同处一个 Teleport：始终在 rc 面板之后挂载、且 z-index(10010) 高于
             rc-mask(10003)，保证从「配置远程客户端」面板里点「选择会话」时列表浮在最上层 -->
        <Transition name="sp-slide">
          <div v-if="showPicker" class="sp-mask" @click="showPicker = false">
            <div class="sp-panel" @click.stop>
              <SessionPicker @pick="onPickSession" @close="showPicker = false" />
            </div>
          </div>
        </Transition>
      </Teleport>
    </div>

    <!-- 交互式：一步一步配置，每步有说明与动画 -->
    <div v-else class="cfg-body">
      <div class="wiz-progress">
        <span
          v-for="(s, i) in steps"
          :key="i"
          class="wiz-dot"
          :class="{ 'is-active': i === step, 'is-done': i < step }"
        />
        <span class="wiz-count">{{ step + 1 }} / {{ steps.length }}</span>
      </div>

      <Transition :name="wizDir" mode="out-in">
        <div :key="step" class="wiz-step">
          <div class="wiz-step__title">{{ t(currentStep.titleKey) }}</div>
          <p class="wiz-step__desc">{{ t(currentStep.descKey) }}</p>
          <ForwardDiagram
            ref="wizDiagRef"
            :mode="form.mode"
            compact
            :form="form"
            :highlight="currentStep.node"
            :remote-focus="remoteConfiguring ? rcFocus : null"
          />
          <div class="wiz-fields">
            <template v-for="f in currentStep.fields" :key="f.key">
              <!-- 认证方式 -->
              <div v-if="f.type === 'auth'" class="wiz-field">
                <label>{{ t(f.labelKey) }}</label>
                <el-radio-group
                  :model-value="String(form[f.key] ?? 'password')"
                  size="small"
                  @update:model-value="(v: string | number | boolean) => apply({ [f.key]: String(v) })"
                >
                  <el-radio-button value="password">{{ t('portForward.auth.password') }}</el-radio-button>
                  <el-radio-button value="key">{{ t('portForward.auth.key') }}</el-radio-button>
                </el-radio-group>
              </div>
              <!-- 密码 -->
              <div v-else-if="f.type === 'password' && form.sshAuthType !== 'key'" class="wiz-field">
                <label :for="`wiz-${String(f.key)}`">{{ t(f.labelKey) }}</label>
                <el-input
                  :id="`wiz-${String(f.key)}`"
                  :model-value="String(form[f.key] ?? '')"
                  type="password"
                  size="small"
                  show-password
                  @update:model-value="(v: string) => apply({ [f.key]: v })"
                />
              </div>
              <!-- 私钥 -->
              <div v-else-if="f.type === 'key' && form.sshAuthType === 'key'" class="wiz-field">
                <label :for="`wiz-${String(f.key)}`">{{ t(f.labelKey) }}</label>
                <div class="wiz-key-row">
                  <el-input
                    :model-value="form.sshKeyName || form.sshKeyPath || ''"
                    size="small"
                    :placeholder="form.sshKeyContent ? t('portForward.auth.key') : t('portForward.btn.browse')"
                    readonly
                  />
                  <el-button size="small" @click="wizPickKey">{{ t('portForward.btn.browse') }}</el-button>
                </div>
              </div>
              <!-- 端口 -->
              <div v-else-if="f.type === 'port'" class="wiz-field">
                <label :for="`wiz-${String(f.key)}`">{{ t(f.labelKey) }}</label>
                <!-- 服务器监听端口：修改数值后在其正下方显示内联 Tip（约 10 秒后自动消失），
                     提示需在服务器放行该端口并开启 GatewayPorts；内联占位不浮层，与其它提示互不重叠。
                     端口输入：强制阻止（超界/超长立即丢弃，输入 70000 实时变 65535） -->
                <el-input
                  :id="`wiz-${String(f.key)}`"
                  :model-value="portText(f)"
                  inputmode="numeric"
                  :maxlength="5"
                  size="small"
                  @update:model-value="(v: string) => onPortInput(f, v)"
                />
              </div>
              <!-- 文本 -->
              <div v-else class="wiz-field">
                <label :for="`wiz-${String(f.key)}`">{{ t(f.labelKey) }}</label>
                <el-input
                  :id="`wiz-${String(f.key)}`"
                  :model-value="String(form[f.key] ?? '')"
                  size="small"
                  @update:model-value="(v: string) => apply({ [f.key]: v })"
                />
              </div>
            </template>
            <!-- 服务器监听端口 Tip：横跨表单整行（与表单同宽），文档流占位不盖其它提示 -->
            <Transition v-if="wizHasRemotePort" name="wiz-tip-fade">
              <div v-if="portTipVisible" class="wiz-port-tip">
                <span class="wiz-port-tip__icon">⚠</span>
                <span>{{
                  t('portForward.remoteClient.gatewayTip', {
                    host: String(form.sshHost ?? '').trim() || '…',
                    port: Number(portTipValue) || 0
                  })
                }}</span>
              </div>
            </Transition>
          </div>
        </div>
      </Transition>

      <!-- 远程模式「测试连通性」未配置客户端时，进入两台远程客户端的 SSH 配置面板。
           Teleport 到 body + fixed 遮罩：上下占满整个窗口，遮住底部「取消/保存/确定」，
           避免误以为那三个按钮是操作本面板的 -->
      <Teleport to="body">
        <Transition name="sp-slide">
          <div v-if="remoteConfiguring && form.mode === 'remote'" class="rc-mask" @click.self="remoteConfiguring = false">
            <div class="rcfg-panel" @click.stop>
              <div class="rcfg-head">
                <span>{{ t('portForward.remoteClient.title') }}</span>
                <el-button text size="small" @click="remoteConfiguring = false">{{ t('portForward.btn.cancel') }}</el-button>
              </div>
              <div v-if="remoteClientRightStale" class="rcfg-stale">{{ t('portForward.remoteClient.rightStale') }}</div>
              <div class="rcfg-grid">
                <section
                  class="rcfg-card"
                  :class="{ 'is-active': rcFocus === 'left', 'is-ok': clientTest.left === 'ok', 'is-fail': clientTest.left === 'fail' }"
                >
                  <h4>{{ t('portForward.remoteClient.left') }}</h4>
                  <RemoteClientForm
                    :model="form.remoteClientLeft || defaultClient()"
                    :bind-addr="'127.0.0.1'"
                    :remote-port="form.remotePort || 0"
                    @update="(p) => applyClient('left', p)"
                    @test="clientTest.left = $event"
                    @pick="openPicker('left')"
                    @focus="rcFocus = 'left'"
                    @blur="rcFocus = null"
                  />
                </section>
                <section
                  class="rcfg-card"
                  :class="{ 'is-active': rcFocus === 'right', 'is-ok': clientTest.right === 'ok', 'is-fail': clientTest.right === 'fail' }"
                >
                  <h4>{{ t('portForward.remoteClient.right') }}</h4>
                  <RemoteClientForm
                    :model="rightClientModel"
                    :bind-addr="(form.bindAddr || '').trim()"
                    :remote-port="form.remotePort || 0"
                    @update="(p) => applyClient('right', p)"
                    @test="clientTest.right = $event"
                    @pick="openPicker('right')"
                    @focus="rcFocus = 'right'"
                    @blur="rcFocus = null"
                  />
                </section>
              </div>
              <div class="rcfg-foot">
                <span>{{ t('portForward.remoteClient.configured') }}</span>
              </div>
            </div>
          </div>
        </Transition>
      </Teleport>
    </div>

    <!-- 连通性测试结果 -->
    <div v-if="testResult" class="cfg-result" :class="testResult.ok ? 'is-ok' : 'is-fail'">
      <el-icon>
        <CircleCheck v-if="testResult.ok" />
        <CircleClose v-else />
      </el-icon>
      <span>{{ testResult.msg }}</span>
    </div>

    <template #footer>
      <div class="cfg-footer">
        <el-button class="cfg-btn" :loading="testing" @click="runTest">
          {{ t('portForward.btn.test') }}
        </el-button>
        <span class="cfg-footer__right">
          <el-button v-if="isWizard && step > 0" class="cfg-btn" @click="goStep(step - 1)">
            {{ t('portForward.btn.prev') }}
          </el-button>
          <el-button v-if="isWizard && !isLastStep" type="primary" @click="goStep(step + 1)">
            {{ t('portForward.btn.next') }}
          </el-button>
          <!-- 从左往右：取消（不保存）/ 保存（保存但不关闭）/ 确定（保存并关闭） -->
          <el-button class="cfg-btn" @click="close">{{ t('portForward.btn.cancel') }}</el-button>
          <el-button class="cfg-btn" @click="save(false)">{{ t('portForward.btn.save') }}</el-button>
          <el-button type="primary" @click="save(true)">{{ t('portForward.btn.ok') }}</el-button>
        </span>
      </div>
    </template>
  </el-dialog>
</template>

<script setup lang="ts">
/**
 * 端口转发：配置界面
 * 作者：JC
 *
 * 两种交互（在「设置-偏好设置-端口转发」里切换）：
 * - page   页面式：所有配置项一次性填完，配置项内嵌在架构图对应节点上（MobaXterm 风格）；
 * - wizard 交互式：一步一步配置，每步有说明、进度与切换动画（Termius 风格）。
 * 两者都支持「测试连通性」：消息沿隧道逐段传输，通过变绿、失败变红并给出原因。
 */
import { computed, nextTick, reactive, ref, watch } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { CircleCheck, CircleClose } from '@element-plus/icons-vue'
import { useI18nStore } from '@/stores/i18n'
import { useSettingsStore } from '@/stores/settings'
import { useHostStore, type HostItem } from '@/stores/host'
import { invoke, isTauriEnv } from '@/api'
import {
  buildTestSteps, defaultRule, ruleSummary, validateRule, probeSshReachable, toBackendRule,
  probeRemoteClient, remoteClientConfigured, remoteClientRightValid, sshErrorKey, proxyInvokeArgs,
  usePortForwardStore,
  type ForwardRule, type ForwardTestStep, type RemoteClientSsh,
} from '@/stores/portForward'
import { WIZARD_STEPS, type FieldDef } from './forwardMeta'
import ForwardDiagram from './ForwardDiagram.vue'
import SessionPicker from './SessionPicker.vue'
import RemoteClientForm from './RemoteClientForm.vue'

const props = defineProps<{
  modelValue: boolean
  /** 规则草稿（编辑时传入；为 null 表示新建） */
  draft: ForwardRule | null
  isNew?: boolean
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', v: boolean): void
  (e: 'saved', rule: ForwardRule): void
}>()

const settings = useSettingsStore()
const t = useI18nStore().t
const hostStore = useHostStore()
const pfStore = usePortForwardStore()

const form = ref<ForwardRule>(defaultRule('local'))
/** 「选择 SSH 会话」右侧面板是否展开 */
const showPicker = ref(false)
/** 本次选择会话的回填目标：SSH 服务器本身，或左/右远程客户端 */
const pickerTarget = ref<'ssh' | 'left' | 'right'>('ssh')
const step = ref(0)
const wizDir = ref<'wiz-next' | 'wiz-prev'>('wiz-next')
const testing = ref(false)
const testResult = ref<{ ok: boolean; msg: string } | null>(null)
/** 结果通知保持时长（ms）：到时与架构图上的发光/徽标一起消失 */
const RESULT_HOLD_MS = 6000
let resultTimer: number | null = null
function clearResultTimer(): void {
  if (resultTimer !== null) {
    window.clearTimeout(resultTimer)
    resultTimer = null
  }
}
const pageDiagRef = ref<InstanceType<typeof ForwardDiagram> | null>(null)
const wizDiagRef = ref<InstanceType<typeof ForwardDiagram> | null>(null)

/** 服务器监听端口 Tip：修改数值（>0）时立即弹出展示，约 10 秒后自动消失（非常驻） */
const PORT_TIP_HOLD_MS = 10000
const portTipVisible = ref(false)
const portTipValue = ref(0)
let portTipTimer: number | null = null
function flashPortTip(port: number | undefined): void {
  if (portTipTimer !== null) {
    window.clearTimeout(portTipTimer)
    portTipTimer = null
  }
  portTipValue.value = Number(port) || 0
  if (!(portTipValue.value > 0)) {
    portTipVisible.value = false
    return
  }
  portTipVisible.value = true
  portTipTimer = window.setTimeout(() => {
    portTipTimer = null
    portTipVisible.value = false
  }, PORT_TIP_HOLD_MS)
}

/** 端口显示文本：0 / 空 一律按「未填」显示为空；值恒被钳制到 [0, 65535] */
function portText(f: FieldDef): string {
  const n = Number(form.value[f.key])
  if (!Number.isFinite(n) || n <= 0) return ''
  return String(Math.min(n, 65535))
}

/**
 * 端口「强制阻止」输入：逐键即时处理，超界 / 超长字符直接丢弃不上屏。
 * - 只保留数字，且按住 5 位上限截断（65535 为最大 5 位）→ 超出的位根本输不进去；
 * - 解析后值 > 65535 立即变为 65535 → 输入 70000 实时显示成 65535；
 * 因 el-input 为受控组件，过滤后的值写回即同步刷新界面，无需等失焦。
 */
function onPortInput(f: FieldDef, raw: string): void {
  const digits = String(raw ?? '').replace(/\D/g, '').slice(0, 5)
  let n = digits ? Number(digits) : 0
  if (n > 65535) {
    n = 65535
    ElMessage.warning(t('portForward.tip.portRange'))
  }
  apply({ [f.key]: n })
  if (f.key === 'remotePort') flashPortTip(n)
}

/** 远程模式：是否正在展示「两台远程客户端 SSH 配置」面板（点击测试且未配置时进入） */
const remoteConfiguring = ref(false)
/** 动态模式：是否展示「测试目标（IP+端口）」输入框 —— 非必填，仅当用户明确选择「配置」时出现，
 *  也可点关闭按钮收起；收起后再次点击测试连通性会重新询问 */
const dynConfiguring = ref(false)
/** 动态模式是否显示「测试目标」输入框：仅由 dynConfiguring 控制 */
const dynTargetOpen = computed(() => form.value.mode === 'dynamic' && dynConfiguring.value)
// 编辑已有规则时，测试目标输入即持久化（输入即落盘），关闭弹窗/窗口后再次打开仍保留
watch(
  () => [form.value.remoteTargetHost, form.value.remoteTargetPort],
  (v) => {
    if (form.value.mode !== 'dynamic' || props.isNew) return
    const host = String(v[0] ?? '').trim()
    const port = Number(v[1] ?? 0) || 0
    if (!host && port === 0) return // 全清空：不当作「已配置」写盘
    pfStore.updateTestTarget({ id: form.value.id, remoteTargetHost: host, remoteTargetPort: port })
  },
)
/** 光标当前停在 left/right 哪台远程客户端的表单里：联动高亮架构图上对应的客户端节点 */
const rcFocus = ref<'left' | 'right' | null>(null)
/** left/right 两台客户端的单机连接测试结果：外层卡片据此显示悬浮绿/红态 */
type RcTestState = 'idle' | 'ok' | 'fail'
const clientTest = reactive<{ left: RcTestState; right: RcTestState }>({ left: 'idle', right: 'idle' })
watch(remoteConfiguring, (v) => {
  if (!v) {
    rcFocus.value = null
    clientTest.left = 'idle'
    clientTest.right = 'idle'
  }
})

/** 一台远程客户端的默认 SSH 配置 */
function defaultClient(): RemoteClientSsh {
  return {
    sshHost: '',
    sshPort: 22,
    sshUser: 'root',
    sshAuthType: 'password',
    sshPassword: '',
    sshKeyPath: '',
    sshKeyContent: '',
    sshKeyName: '',
    sshPassphrase: '',
  }
}

/** 右侧远程客户端未配置时的预填值：IP 地址 = 当前服务器监听地址（它就是连这个地址的那台） */
function prefilledRight(): RemoteClientSsh {
  return { ...defaultClient(), sshHost: (form.value.bindAddr || '').trim() }
}
/** 右侧远程客户端表单的展示取值：已配置用已存配置，否则用预填 */
const rightClientModel = computed<RemoteClientSsh>(
  () => form.value.remoteClientRight?.cfg ?? prefilledRight(),
)

/** 写回某台远程客户端的 SSH 配置（right 同时绑定当前监听地址） */
function applyClient(side: 'left' | 'right', patch: Partial<RemoteClientSsh>): void {
  if (side === 'left') {
    const cur = form.value.remoteClientLeft ?? defaultClient()
    form.value.remoteClientLeft = { ...cur, ...patch }
  } else {
    const cur = form.value.remoteClientRight?.cfg ?? prefilledRight()
    form.value.remoteClientRight = { addr: (form.value.bindAddr || '').trim(), cfg: { ...cur, ...patch } }
  }
}

/** 右侧远程客户端配置是否已因监听地址变更而失效 */
const remoteClientRightStale = computed(
  () => !!form.value.remoteClientRight && !remoteClientRightValid(form.value),
)

/** 配置交互方式由设置决定 */
const isWizard = computed(() => settings.settings.forwardConfigStyle === 'wizard')
const steps = computed(() => WIZARD_STEPS[form.value.mode])
const currentStep = computed(() => steps.value[Math.min(step.value, steps.value.length - 1)])
const isLastStep = computed(() => step.value >= steps.value.length - 1)
/** 当前向导步骤是否包含「服务器监听端口」字段（端口 Tip 只在该步骤显示） */
const wizHasRemotePort = computed(() => currentStep.value.fields.some(f => f.key === 'remotePort'))

/** 远程模式：监听地址改变后，右侧远程客户端（绑定在该地址上）的配置即作废，需重新配置 */
watch(() => form.value.bindAddr, (cur) => {
  if (form.value.mode !== 'remote' || !form.value.remoteClientRight) return
  if ((form.value.remoteClientRight.addr || '').trim().toLowerCase() !== (cur || '').trim().toLowerCase()) {
    form.value.remoteClientRight = null
  }
})

/** 打开时用草稿初始化；关闭时不动（保存由父组件负责） */
watch(() => props.modelValue, (open) => {
  if (!open) return
  form.value = props.draft ? JSON.parse(JSON.stringify(props.draft)) : defaultRule('local')
  step.value = 0
  wizDir.value = 'wiz-next'
  clearResultTimer()
  testResult.value = null
  showPicker.value = false
  // 会话列表面板依赖会话库数据：打开时确保已加载
  if (!hostStore.loaded) void hostStore.load()
  void nextTick(() => activeDiagram()?.reset())
})

/** 点击架构图上的远程客户端「编辑」标签：重新打开该客户端的 SSH 配置面板 */
function onEditRemoteClient(side: 'left' | 'right'): void {
  rcFocus.value = side
  remoteConfiguring.value = true
}

/** 打开「选择 SSH 会话」面板（target 决定回填到 SSH 服务器还是某台远程客户端） */
function openPicker(target: 'ssh' | 'left' | 'right' = 'ssh'): void {
  if (!hostStore.loaded) void hostStore.load()
  pickerTarget.value = target
  showPicker.value = true
}

/** 从会话回填连接信息（地址/端口/登录名/认证方式与凭据） */
function onPickSession(host: HostItem): void {
  const addr = host.addr || ''
  const user = addr.split('@')[0] || ''
  const hostPart = addr.includes('@') ? addr.split('@')[1] : addr
  const port = hostPart.includes(':') ? parseInt(hostPart.split(':')[1]) || 22 : 22
  const realHost = hostPart.includes(':') ? hostPart.split(':')[0] : hostPart
  const cfg = host.config || {}
  const sshAuthType = cfg.authType === 'key' ? 'key' : 'password'
  if (pickerTarget.value === 'ssh') {
    apply({
      sshHost: realHost,
      sshPort: port,
      sshUser: cfg.username || user,
      sshAuthType,
      sshPassword: cfg.password || '',
      sshKeyPath: cfg.keyPath || '',
      sshKeyContent: cfg.keyContent || '',
      sshKeyName: cfg.keyName || '',
      sshPassphrase: cfg.passphrase || '',
    })
  } else {
    applyClient(pickerTarget.value, {
      sshHost: realHost,
      sshPort: port,
      sshUser: cfg.username || user,
      sshAuthType,
      sshPassword: cfg.password || '',
      sshKeyPath: cfg.keyPath || '',
      sshKeyContent: cfg.keyContent || '',
      sshKeyName: cfg.keyName || '',
      sshPassphrase: cfg.passphrase || '',
    })
  }
  showPicker.value = false
}

/** 写回草稿字段（避免直接修改 props） */
function apply(patch: Partial<ForwardRule>): void {
  Object.assign(form.value, patch)
  clearResultTimer()
  testResult.value = null
}

/** 向导模式：选择私钥文件（与页面式一致，托管私钥内容） */
async function wizPickKey(): Promise<void> {
  if (!isTauriEnv()) return
  try {
    const path = await invoke<string | null>('pick_key_file')
    if (!path) return
    const data = await invoke<{ content: string; name: string }>('read_key_file', { path })
    apply({ sshKeyPath: path, sshKeyContent: data.content, sshKeyName: data.name || path.split(/[\\/]/).pop() || '' })
  } catch (e) {
    console.warn('[portForward] 读取私钥文件失败:', e)
  }
}

function activeDiagram() {
  return isWizard.value ? wizDiagRef.value : pageDiagRef.value
}

/** 未配置客户端（发模拟请求）时，SSH 服务器是否接受它的请求：
 *  0.0.0.0 接受所有来源；自定义监听地址（特定 IP）下，与之同地址的那台客户端被接受；
 *  回环 / SSH 服务器自身 IP = 仅接受本机来源 → 远程客户端被拒（红光） */
function clientAccepted(bind: string): boolean {
  const b = (bind || '').trim().toLowerCase()
  if (!b || b === '127.0.0.1' || b === 'localhost') return false
  return b !== (form.value.sshHost || '').trim().toLowerCase()
}

function goStep(next: number): void {
  wizDir.value = next >= step.value ? 'wiz-next' : 'wiz-prev'
  step.value = Math.max(0, Math.min(next, steps.value.length - 1))
  testResult.value = null
}

/** 测试连通性：按模式选择「真实」或「模拟」 */
async function runTest(): Promise<void> {
  if (testing.value) return
  // 测试开始前先把拓扑图恢复初始状态：清掉上一次测试残留的发光、光球与结果徽标
  activeDiagram()?.reset()

  // 远程模式：未配置两台客户端时，先询问是否配置（配置 → 进入表单；不配置 → 模拟）
  if (form.value.mode === 'remote' && !remoteClientConfigured(form.value) && !remoteConfiguring.value) {
    try {
      await ElMessageBox.confirm(
        t('portForward.test.confirmMsg'),
        t('portForward.test.confirmTitle'),
        {
          confirmButtonText: t('portForward.test.configure'),
          cancelButtonText: t('portForward.test.simulate'),
          type: 'info',
          distinguishCancelAndClose: true,
        },
      )
      remoteConfiguring.value = true
      return
    } catch {
      // 选择「仅模拟测试」：不发真实请求，仅按拓扑推演（黄色信号）
      await runSimulated()
      return
    }
  }

  // 动态模式：只要「测试目标」表单处于收起状态（含关闭后、或从未配置），再次点击测试都先询问：
  // 配置（→ 在目标区显示 IP+端口输入框）或仅模拟测试（→ 不发真实请求）
  if (form.value.mode === 'dynamic' && !dynConfiguring.value) {
    try {
      await ElMessageBox.confirm(
        t('portForward.test.dynTargetMsg'),
        t('portForward.test.confirmTitle'),
        {
          confirmButtonText: t('portForward.test.dynTargetConfig'),
          cancelButtonText: t('portForward.test.simulate'),
          type: 'info',
          distinguishCancelAndClose: true,
        },
      )
      dynConfiguring.value = true
      return
    } catch {
      // 选择「仅模拟测试」：不发真实请求，仅按拓扑推演（黄色信号）
      await runDynamicSimulated()
      return
    }
  }

  // 远程模式 + 已配置两台客户端：发真实请求（绿色信号）
  if (form.value.mode === 'remote' && remoteClientConfigured(form.value)) {
    if (remoteConfiguring.value) remoteConfiguring.value = false
    await runRemoteReal()
    return
  }

  // 进入配置面板但配置未完成：提示先填完两台客户端
  if (form.value.mode === 'remote' && remoteConfiguring.value) {
    ElMessage.warning(t('portForward.test.confirmMsg'))
    return
  }

  // 本地 / 动态模式：现有真实探测（绿色信号）
  await runLocalDynamic()
}

/** 本地 / 动态模式：配置校验 + 真实探测 SSH 服务器 + 端到端建通道，最后播放动画（绿色） */
async function runLocalDynamic(): Promise<void> {
  // 动态模式：已选择配置测试目标、但 IP/端口还没填全时，停在目标区让用户补全
  if (form.value.mode === 'dynamic' && dynConfiguring.value) {
    const targetOk = !!(form.value.remoteTargetHost || '').trim() && form.value.remoteTargetPort > 0
    if (!targetOk) {
      ElMessage.warning(t('portForward.test.dynNeedTarget'))
      return
    }
  }
  testing.value = true
  testResult.value = null
  const steps = buildTestSteps(form.value)
  const sshStep = steps.find(s => s.leg === 1)
  if (sshStep?.ok) {
    const err = await probeSshReachable(form.value)
    if (err) {
      sshStep.ok = false
      sshStep.reason = err
    }
  }
  if (steps.every(s => s.ok)) {
    try {
      await invoke('port_forward_probe', { rule: await toBackendRule(form.value), ...(await proxyInvokeArgs()) })
    } catch (e) {
      console.warn('[portForward] 端到端探测失败:', e)
      const last = steps[steps.length - 1]
      last.ok = false
      last.reason = 'portForward.err.forwardProbe'
    }
  }
  const diag = activeDiagram()
  const failed = diag ? await diag.play(steps) : null
  finishTest(failed)
}

/**
 * 动态模式「未配置测试目标」时选择「仅模拟测试」：不真实连远端目标，
 * 只按拓扑推演（SSH 服务器自身可达性决定其节点光色，隧道黄光模拟）。
 * 与远程模式模拟一致 —— 都是「模拟请求」而非真实发包。
 */
async function runDynamicSimulated(): Promise<void> {
  testing.value = true
  testResult.value = null
  const steps = buildTestSteps(form.value)
  const sshStep = steps.find(s => s.leg === 1)
  if (sshStep?.ok) {
    const err = await probeSshReachable(form.value)
    if (err) {
      sshStep.ok = false
      sshStep.reason = err
    }
  }
  const diag = activeDiagram()
  const failed = diag ? await diag.play(steps, { simLegs: [0, 1, 2], simNodes: ['target'] }) : null
  finishTest(failed, t('portForward.test.simulation'))
}

/** 远程模式「未配置客户端」：纯拓扑推演，不发任何真实请求——
 *  两台（台）客户端都发模拟请求：被接受 → 黄光；被拒 → 红光。
 *  SSH 服务器自身请求的可达性只决定它自己的光色（绿/红），不牵连隧道 */
async function runSimulated(): Promise<void> {
  testing.value = true
  testResult.value = null
  const bind = (form.value.bindAddr || '').trim()
  const steps = buildTestSteps(form.value)
  // 模拟请求能否被接受并穿隧道继续：0.0.0.0（接受所有）/ 特定 IP（同地址的那台被接受）；
  // 回环 / 服务器自身 IP = 仅接受本机来源 → 客户端模拟请求被拒（红光），测试止于第一段
  steps[0].ok = clientAccepted(bind)
  // 被接受的模拟请求可穿过隧道继续；SSH 自身被拒（监听特定 IP）只让服务器节点变红，
  // 不影响隧道——同地址客户端的请求依然能穿隧道抵达 JC Shell（sshSelfOk 仅控制节点颜色）
  steps[1].ok = true
  const diag = activeDiagram()
  const failed = diag ? await diag.play(steps, {
    simLegs: [0, 1, 2],
    simClient: true,
    simExtra: true,
  }) : null
  finishTest(failed, t('portForward.test.simulation'))
}

/** 远程模式「已配置两台客户端」：临时启动隧道让服务器真正监听，SSH 连两台客户端，
 *  由它们向监听地址:端口发真实请求（绿色信号），测完立刻拆掉临时隧道。
 *  客户端要么不配置、要么配置就必须认证通过，否则测试无法开始 */
async function runRemoteReal(): Promise<void> {
  testing.value = true
  testResult.value = null
  const bind = (form.value.bindAddr || '').trim()
  const port = form.value.remotePort
  const leftCfg = form.value.remoteClientLeft!
  const rightCfg = form.value.remoteClientRight!.cfg
  // 临时 id：避免误停用户正在运行的、与当前规则同 id 的真实隧道
  const tmpId = `__probe__${form.value.id}_${Date.now()}`
  let tunnelUp = false
  try {
    // 先临时启动隧道，让 SSH 服务器真正 bind「监听地址:监听端口」；
    // 若启动失败（端口已被用户自己的隧道占用、或服务器 OpenSSH 不允许绑定该地址）：
    // 不阻断测试，改为按端口现有监听状态继续探测——已在监听则可达，否则探测自会判为不可达
    try {
      const rule = await toBackendRule(form.value)
      await invoke('port_forward_start', { rule: { ...rule, id: tmpId }, ...(await proxyInvokeArgs()) })
      tunnelUp = true
    } catch (e) {
      console.warn('[portForward] 临时启动隧道(测试连通性用)失败，按现有监听状态继续探测:', e)
    }
    // 左侧客户端连「非监听地址」（自身回环，预期被拒）；右侧客户端连 SSH 服务器的监听端口。
    // 监听端口的实际归属在 SSH 服务器（表单里的“监听地址”仅是配置文本，可能指向非服务器地址，
    // 不能直接当连接目标）；客户端能路由到服务器绑定的监听端口，故统一连 SSH 服务器可达地址
    const rightAddr = (form.value.sshHost || '').trim() || bind
    const [l, r] = await Promise.all([
      probeRemoteClient(leftCfg, '127.0.0.1', port),
      probeRemoteClient(rightCfg, rightAddr, port),
    ])
    // 配置了的客户端必须认证通过，否则测试无法开始（配置但未通过认证 = 无法发起真实请求）；
    // 失败时只给用户友好提示（认证失败/超时…），不暴露后端原始报错
    if (!l.sshOk) {
      testing.value = false
      ElMessage.error(t('portForward.remoteClient.sshFail', { reason: t(sshErrorKey(l.sshErr)) }))
      return
    }
    if (!r.sshOk) {
      testing.value = false
      ElMessage.error(t('portForward.remoteClient.sshFail', { reason: t(sshErrorKey(r.sshErr)) }))
      return
    }
    const leftState = l.targetOk ? 'ok' : 'blocked'
    const rightState = r.targetOk ? 'ok' : 'blocked'
    const ok = (await activeDiagram()?.playRemoteReal(leftState, rightState)) ?? false
    testing.value = false
    testResult.value = ok
      ? { ok: true, msg: t('portForward.test.real') }
      : { ok: false, msg: t('portForward.test.failed') }
    clearResultTimer()
    resultTimer = window.setTimeout(() => {
      resultTimer = null
      testResult.value = null
    }, RESULT_HOLD_MS)
  } finally {
    // 无论成功失败都拆掉临时隧道，避免占住服务器上的监听端口
    if (tunnelUp) {
      try {
        await invoke('port_forward_stop', { id: tmpId })
      } catch {
        console.warn('[portForward] 清理临时隧道失败:', tmpId)
      }
    }
  }
}

/** 统一收尾：写结果通知、按 RESULT_HOLD_MS 自动消失 */
function finishTest(failed: ForwardTestStep | null, simNote?: string): void {
  testing.value = false
  testResult.value = failed
    ? { ok: false, msg: `${t('portForward.test.failed')}：${t(failed.reason)}` }
    : { ok: true, msg: simNote ?? t('portForward.test.passed') }
  clearResultTimer()
  resultTimer = window.setTimeout(() => {
    resultTimer = null
    testResult.value = null
  }, RESULT_HOLD_MS)
}

function close(): void {
  clearResultTimer()
  testResult.value = null
  emit('update:modelValue', false)
}

/**
 * 保存配置。
 * @param closeAfter 「确定」传 true：保存并关闭弹窗；「保存」传 false：保存但留在页面继续编辑。
 * 两者都写回父组件并提示「修改已保存」；若该规则已开启，父组件/store 会立即按新配置重建隧道。
 */
function save(closeAfter: boolean): void {
  const err = validateRule(form.value)
  if (err) {
    testResult.value = { ok: false, msg: `${t('portForward.test.failed')}：${t(err)}` }
    ElMessage.warning(t(err))
    return
  }
  if (!form.value.name.trim()) {
    form.value.name = `${t(`portForward.mode.${form.value.mode}.title`)} · ${ruleSummary(form.value)}`
  }
  emit('saved', JSON.parse(JSON.stringify(form.value)) as ForwardRule)
  ElMessage.success(t('portForward.saved'))
  if (closeAfter) close()
}
</script>

<style scoped>
/* 标题行（位于弹窗头部）：标题 + 模式标签 + 规则名称，同一行 */
.cfg-head {
  display: flex;
  align-items: center;
  gap: 10px;
  padding-right: 26px;   /* 给右上角关闭按钮留位 */
}
.cfg-title {
  flex-shrink: 0;
  font-size: 14px;
  font-weight: 600;
  color: var(--jc-text);
}
.cfg-mode {
  flex-shrink: 0;
  padding: 2px 8px;
  border-radius: 4px;
  font-size: 12px;
  color: var(--jc-accent);
  background: color-mix(in srgb, var(--jc-accent) 14%, transparent);
}
/* 规则名称：加高输入框，与弹窗标题行更协调 */
.cfg-name { width: 460px; max-width: 55%; }
.cfg-name :deep(.el-input__wrapper) {
  padding: 8px 12px;
}
.cfg-name :deep(.el-input__inner) {
  height: 22px;
  line-height: 22px;
  font-size: 14px;
  caret-color: var(--jc-accent);
}
.cfg-body {
  padding: 2px 2px 2px;
  min-height: 240px;
  position: relative;
  overflow: hidden;
}

/* ===== 选择 SSH 会话：右侧滑出面板 ===== */
.sp-mask {
  /* 与 rc-mask 同处一个 Teleport：fixed 全屏 + 高于 rc-mask(10003)/el-message(10005)/
     el-overlay(10000)，保证会话列表永远在最上层（含从「配置远程客户端」面板内打开的场景） */
  position: fixed;
  inset: 0;
  z-index: 10010;
  display: flex;
  justify-content: flex-end;
  background: rgba(0, 0, 0, 0.35);
}
.sp-panel {
  width: 42%;
  min-width: 300px;
  max-width: 460px;
  height: 100%;
  box-shadow: -12px 0 30px -16px rgba(0, 0, 0, 0.8);
}
.sp-slide-enter-active,
.sp-slide-leave-active { transition: opacity 0.22s ease; }
.sp-slide-enter-active .sp-panel,
.sp-slide-leave-active .sp-panel { transition: transform 0.24s ease; }
.sp-slide-enter-from,
.sp-slide-leave-to { opacity: 0; }
.sp-slide-enter-from .sp-panel,
.sp-slide-leave-to .sp-panel { transform: translateX(100%); }

/* ===== 远程客户端 SSH 配置面板（Teleport 到 body 的全屏遮罩：上下占满窗口，
   底部「取消/保存/确定」被遮住，不会误认为是本面板的按钮） ===== */
.rc-mask {
  position: fixed;
  inset: 0;
  /* 本应用全局把 .el-overlay 抬到 10000（MainWindow）、MessageBox 到 10002，
     此处必须更高才能盖住「编辑端口转发」弹窗；仍低于 ElMessage（10005），
     保证测试失败提示能浮在本面板之上 */
  z-index: 10003;
  display: flex;
  justify-content: flex-end;
  background: rgba(0, 0, 0, 0.5);
}
.rcfg-panel {
  width: 64%;
  min-width: 440px;
  max-width: 760px;
  height: 100%;
  background: var(--jc-bg-panel);
  box-shadow: -12px 0 30px -16px rgba(0, 0, 0, 0.8);
  padding: 16px 18px;
  overflow: auto;
}
.rcfg-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 14px;
  font-weight: 600;
  color: var(--jc-text);
}
.rcfg-stale {
  margin: 8px 0;
  padding: 6px 10px;
  border-radius: 6px;
  font-size: 12px;
  color: var(--jc-danger);
  background: color-mix(in srgb, var(--jc-danger) 12%, transparent);
}
.rcfg-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 16px;
  margin-top: 12px;
}
.rcfg-card {
  border: 1px solid var(--jc-border-strong);
  border-radius: 10px;
  padding: 12px;
  transition: border-color 0.18s ease, box-shadow 0.18s ease, transform 0.18s ease, background-color 0.18s ease;
}
/* 连接测试通过：绿色悬浮卡片 */
.rcfg-card.is-ok {
  border-color: var(--el-color-success, #67c23a);
  background: color-mix(in srgb, var(--el-color-success, #67c23a) 7%, transparent);
  transform: translateY(-3px);
  box-shadow: 0 10px 28px rgba(0, 0, 0, 0.35), 0 0 0 1px var(--el-color-success, #67c23a);
}
/* 连接测试失败：红色悬浮卡片 */
.rcfg-card.is-fail {
  border-color: var(--jc-danger);
  background: color-mix(in srgb, var(--jc-danger) 7%, transparent);
  transform: translateY(-3px);
  box-shadow: 0 10px 28px rgba(0, 0, 0, 0.35), 0 0 0 1px var(--jc-danger);
}
/* 编辑中：悬浮卡片选中态（抬升 + 主题色描边 + 柔和投影），编辑时覆盖测试结果色 */
.rcfg-card.is-active {
  border-color: var(--jc-accent);
  background: color-mix(in srgb, var(--jc-accent) 6%, transparent);
  transform: translateY(-3px);
  box-shadow: 0 10px 28px rgba(0, 0, 0, 0.35), 0 0 0 1px var(--jc-accent);
}
.rcfg-card h4 {
  margin: 0 0 10px;
  font-size: 12px;
  font-weight: 600;
  color: var(--jc-text-muted);
}
.rcfg-foot {
  margin-top: 14px;
  font-size: 12px;
  color: var(--jc-success);
}

/* ===== 交互式向导 ===== */
.wiz-progress {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 12px;
}
.wiz-dot {
  width: 22px;
  height: 3px;
  border-radius: 2px;
  background: var(--jc-border-strong);
  transition: background 0.25s ease;
}
.wiz-dot.is-done { background: var(--jc-success); }
.wiz-dot.is-active { background: var(--jc-accent); }
.wiz-count {
  margin-left: auto;
  font-size: 11px;
  color: var(--jc-text-muted);
}
.wiz-step__title {
  font-size: 14px;
  font-weight: 600;
  color: var(--jc-text);
}
.wiz-step__desc {
  margin: 4px 0 12px;
  font-size: 12px;
  line-height: 1.6;
  color: var(--jc-text-muted);
}
.wiz-fields {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(190px, 1fr));
  gap: 10px 14px;
  margin-top: 14px;
}
.wiz-field {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.wiz-field label {
  font-size: 11px;
  color: var(--jc-text-muted);
}
/* 服务器监听端口 Tip：内联在输入框正下方（文档流占位，不与其它提示重叠），
   深色底上用暖色警示配色保证可读 */
.wiz-port-tip {
  /* 横跨字段网格整行：宽度与表单一致 */
  grid-column: 1 / -1;
  display: flex;
  align-items: flex-start;
  gap: 6px;
  margin-top: 2px;
  padding: 5px 8px;
  border-radius: 8px;
  border: 1px solid rgba(245, 158, 11, 0.5);
  background: color-mix(in srgb, #f59e0b 14%, var(--jc-bg-panel, #1d2334));
  color: #fcd34d;
  font-size: 11px;
  line-height: 1.45;
}
.wiz-port-tip__icon {
  flex: 0 0 auto;
}
/* 端口 Tip 出现/消失的淡入淡出 */
.wiz-tip-fade-enter-active,
.wiz-tip-fade-leave-active {
  transition: opacity 0.2s ease, transform 0.2s ease;
}
.wiz-tip-fade-enter-from,
.wiz-tip-fade-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}
/* 私钥：输入框 + 浏览按钮一行 */
.wiz-key-row {
  display: flex;
  gap: 6px;
}
.wiz-key-row .el-input {
  flex: 1;
  min-width: 0;
}
/* 步骤切换动画 */
.wiz-next-enter-active,
.wiz-next-leave-active,
.wiz-prev-enter-active,
.wiz-prev-leave-active {
  transition: opacity 0.18s ease, transform 0.18s ease;
}
.wiz-next-enter-from { opacity: 0; transform: translateX(24px); }
.wiz-next-leave-to { opacity: 0; transform: translateX(-24px); }
.wiz-prev-enter-from { opacity: 0; transform: translateX(-24px); }
.wiz-prev-leave-to { opacity: 0; transform: translateX(24px); }

/* ===== 测试结果 ===== */
.cfg-result {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 12px;
  padding: 7px 10px;
  border-radius: 6px;
  font-size: 12px;
}
.cfg-result.is-ok {
  color: var(--jc-success);
  background: color-mix(in srgb, var(--jc-success) 12%, transparent);
}
.cfg-result.is-fail {
  color: var(--jc-danger);
  background: color-mix(in srgb, var(--jc-danger) 12%, transparent);
}

.cfg-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.cfg-footer__right {
  display: inline-flex;
  gap: 10px;
}
/* 底部按钮：加大尺寸 */
.cfg-footer :deep(.el-button) {
  height: 36px;
  padding: 0 18px;
  font-size: 14px;
}
/* 次要按钮（测试连通性 / 取消 / 保存）：给底色与描边，避免和面板背景融成一片 */
.cfg-btn {
  background: var(--jc-bg-hover);
  border-color: var(--jc-border-strong);
  color: var(--jc-text);
}
.cfg-btn:hover {
  background: color-mix(in srgb, var(--jc-accent) 14%, var(--jc-bg-panel));
  border-color: var(--jc-accent);
  color: var(--jc-accent);
}
</style>
