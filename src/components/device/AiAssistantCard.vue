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
  <!-- AI 助手卡片：积分余额 / 限时批次 / 最近流水 / 近7日用量 -->
  <div class="ai-card text-xs">
    <!-- 未登录 -->
    <div v-if="!loggedIn" class="py-3 text-center text-[var(--jc-text-muted)]">
      <div>登录后可使用 AI 助手并查看积分</div>
      <el-button size="small" type="primary" plain class="mt-2" :loading="authStore.loading" @click="authStore.startLogin()">
        立即登录
      </el-button>
    </div>

    <template v-else>
      <!-- 概览：积分余额 = 所有不限时(永久有效)积分之和 -->
      <div class="flex items-center justify-between mb-2">
        <div>
          <span class="text-[var(--jc-text-muted)]">积分余额</span>
          <span class="ml-2 text-base font-bold text-[var(--jc-accent)]">{{ permanentBalance ?? '—' }}</span>
        </div>
        <el-button size="small" text class="!p-0 !h-5" title="刷新" :loading="loading" @click="loadAll">
          <el-icon size="13"><Refresh /></el-icon>
        </el-button>
      </div>

      <!-- 限时批次: 只列有到期时间的积分条目, 不限时的已汇总进上方余额, 不逐条展示 -->
      <template v-if="limitedGrants.length > 0">
        <div class="section-title flex items-center justify-between">
          <span>限时批次</span>
          <span class="font-normal">合计 {{ expiringBalance }}<template v-if="nearestExpireAt"> · {{ fmtDate(nearestExpireAt) }} 起到期</template></span>
        </div>
        <div v-for="(g, i) in limitedGrants" :key="i" class="flex justify-between py-0.5">
          <span class="text-[var(--jc-text-muted)] truncate mr-2" :title="g.sourceLabel || sourceText(g.source)">
            {{ g.sourceLabel || sourceText(g.source) }}
          </span>
          <span class="text-[var(--jc-text-soft)] whitespace-nowrap">
            {{ g.remaining }}/{{ g.points }} · {{ fmtDate(g.expireAt || '') }} 到期
          </span>
        </div>
        <div class="mt-0.5 text-[10px] leading-4 text-[var(--jc-text-muted)]">
          消耗时优先扣减更早到期的批次, 到期后未用完部分作废
        </div>
      </template>

      <!-- 最近流水 -->
      <template v-if="detailed && txns.length > 0">
        <div class="section-title">最近流水</div>
        <div v-for="txn in txns" :key="txn.id" class="flex justify-between py-0.5">
          <span class="text-[var(--jc-text-muted)] truncate mr-2" :title="txn.remark || ''">
            {{ txnTypeText(txn.type) }}
          </span>
          <span :class="txn.amount >= 0 ? 'text-green-400' : 'text-[var(--jc-text-soft)]'">
            {{ txn.amount >= 0 ? '+' : '' }}{{ txn.amount }}
          </span>
        </div>
      </template>

      <!-- 近7日用量（仅请求数与 tokens，计费细节不下发展示） -->
      <template v-if="detailed && usageDays.length > 0">
        <div class="section-title">近 7 日用量</div>
        <div v-for="d in usageDays" :key="d.statDate" class="flex justify-between py-0.5">
          <span class="text-[var(--jc-text-muted)]">{{ d.statDate }}</span>
          <span class="text-[var(--jc-text-soft)]">{{ d.requests }} 次</span>
        </div>
      </template>

      <div v-if="error" class="mt-2">
        <div class="text-red-400">{{ error }}</div>
        <!-- 授权类错误：一键重新登录 -->
        <el-button
          v-if="isAuthError"
          size="small" type="primary" plain class="mt-1"
          :loading="authStore.loading"
          @click="authStore.startLogin()"
        >
          重新登录
        </el-button>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
/**
 * AI 助手设备卡片
 * 作者：JC
 * 展示积分余额、限时批次、最近流水、近7日用量。
 * 口径: 「积分余额」= 所有不限时(永久有效)积分之和, 此类积分不逐条展示;
 *      有到期时间的积分逐条列在「限时批次」, 中文来源 + 剩余/总额 + 到期日;
 *      订阅状态不在此卡片展示; 全局摘要(agentStore)用可消耗总积分, 与 balanceRemaining 同口径。
 * 注意: 计费细节(成本/服务费)为服务端内部数据, 本组件不展示。
 */
import { computed, onMounted, ref } from 'vue'
import { Refresh } from '@element-plus/icons-vue'
import { invoke } from '@/api'
import { useAuthStore } from '@/stores/auth'
import { useAgentStore } from '@/stores/agent'
import { friendlyError } from '@/utils/errorHandler'

const props = withDefaults(defineProps<{ detailed?: boolean }>(), { detailed: false })

interface GrantItem { points?: number; remaining?: number; source?: string; sourceLabel?: string; expireAt?: string }
interface BalanceVO {
  balance?: number
  permanentBalance?: number
  grants?: GrantItem[]
}
interface TxnItem { id: number; type: string; amount: number; remark?: string }
interface UsageDay { statDate: string; requests: number }

const authStore = useAuthStore()
const agentStore = useAgentStore()
const loggedIn = computed(() => authStore.isAuthenticated)

const loading = ref(false)
/** 展示给用户的错误文案（已脱敏，绝不含技术细节） */
const error = ref('')
/** 原始技术错误：仅用于内部逻辑判断（如"是否授权类错误"），不展示给用户 */
const rawError = ref('')
const balance = ref<number | null>(null)
/** 卡片展示的积分余额: 所有不限时(永久有效)积分之和 */
const permanentBalance = ref<number | null>(null)
const grants = ref<GrantItem[]>([])
const txns = ref<TxnItem[]>([])
const usageDays = ref<UsageDay[]>([])

/** 只保留有到期时间的条目: 不限时积分已汇总进「积分余额」, 不逐条展示(旧服务端仍下发时也在前端过滤) */
const limitedGrants = computed(() => grants.value.filter(g => !!g.expireAt))
const expiringBalance = computed(() => limitedGrants.value.reduce((s, g) => s + (g.remaining ?? 0), 0))
const nearestExpireAt = computed(() =>
  limitedGrants.value.map(g => g.expireAt as string).sort()[0] || ''
)

function fmtDate(s: string): string {
  return (s || '').slice(0, 10)
}

/** 服务端未下发 sourceLabel 时的兼容映射 */
function sourceText(s?: string): string {
  switch (s) {
    case 'SUBSCRIBE': return '订阅赠送'
    case 'RECHARGE': return '充值'
    case 'ADMIN_GRANT': return '赠送'
    case 'ACTIVITY': return '活动'
    case 'COUPON': return '兑换码'
    case 'INVITE': return '邀请返利'
    default: return s || '其他'
  }
}

function txnTypeText(t: string): string {
  switch (t) {
    case 'GRANT': return '入账'
    case 'CONSUME': return '消耗'
    case 'REFUND': return '退回'
    case 'EXPIRE': return '过期'
    case 'ADJUST': return '调整'
    default: return t
  }
}

async function loadAll(): Promise<void> {
  if (!loggedIn.value) return
  loading.value = true
  error.value = ''
  rawError.value = ''
  try {
    const vo = await invoke<BalanceVO>('ai_asset_balance')
    balance.value = typeof vo.balance === 'number' ? vo.balance : null
    // 全局摘要(设备卡片折叠态/头像菜单)用的是可消耗总额, 与请求返回的 balanceRemaining 同口径
    agentStore.setBalance(balance.value)
    grants.value = vo.grants ?? []
    // 积分余额 = 不限时积分之和; 兼容旧服务端(无该字段)时用「总额 - 限时」回推
    if (typeof vo.permanentBalance === 'number') {
      permanentBalance.value = vo.permanentBalance
    } else {
      permanentBalance.value = balance.value === null ? null : Math.max(0, balance.value - expiringBalance.value)
    }
    if (props.detailed) {
      const [txnRes, usage] = await Promise.all([
        invoke<{ list?: TxnItem[] }>('ai_asset_txns', { pageNum: 1, pageSize: 5 }).catch(() => null),
        invoke<UsageDay[]>('ai_usage_daily', { startDate: daysAgo(6), endDate: daysAgo(0) }).catch(() => [])
      ])
      txns.value = txnRes?.list ?? []
      usageDays.value = (usage ?? []).slice(-7)
    }
  } catch (e: any) {
    // 原始错误只留在 rawError 供"是否授权类错误"判断；展示给用户的必须是友好文案
    // （技术细节已由 api/index.ts 的 invoke 包装自动上报）
    rawError.value = e?.message || String(e)
    error.value = friendlyError(e)
    // 授权类错误：后端登录态与前端可能已不一致（如其他窗口登出/token 被清），
    // 主动重新拉取登录态，让 UI 收敛到真实状态
    if (isAuthError.value) authStore.load()
  } finally {
    loading.value = false
  }
}

/** 是否为授权/登录态类错误（此类错误提供"重新登录"入口） */
const isAuthError = computed(() => {
  const m = rawError.value
  return m.includes('未授权') || m.includes('登录态') || m.includes('Unauthorized')
})

function daysAgo(n: number): string {
  const d = new Date(Date.now() - n * 86400_000)
  return d.toISOString().slice(0, 10)
}

onMounted(loadAll)
</script>

<style scoped>
.section-title {
  margin-top: 8px;
  margin-bottom: 2px;
  font-weight: 600;
  color: var(--jc-text-soft);
}
</style>
