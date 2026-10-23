// Copyright 2024-2026 OpenJC (https://www.openjc.cc)
// 
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// 
//     http://www.apache.org/licenses/LICENSE-2.0
// 
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

/**
 * 统一安全密码（本地保护）前端封装。
 * 密码哈希与锁定状态保存在 Rust 后端（OS 凭据库），前端不持有任何密码凭据，
 * 普通用户无法靠「删除本地文件 / 卸载重装」绕过密码保护。
 * 作者：JC
 */
import { ElMessage } from 'element-plus'
import { invoke } from '@/api'
import { openVerify } from './securityDialog'
import { biometricUnlock, type BiometricMode } from './biometric'

export interface SecurityScopes {
  /** 查看会话密码（必选） */
  view_password: boolean
  /** 软件启动时需要验证（可选） */
  startup: boolean
  /** 修改/删除会话时需要验证（可选） */
  edit_delete: boolean
  /** 导出会话数据时验证（可选） */
  export_data: boolean
  /** 导出会话密码时验证（始终开启） */
  export_password: boolean
}

export interface SecurityStatus {
  has_password: boolean
  scopes: SecurityScopes
  locked: boolean
  lock_remaining_sec: number
  /** 平台是否支持生物识别解锁（移动端 true，桌面端 false） */
  biometric_supported: boolean
  /** 是否已开启生物识别解锁（平台不支持 / 未设密码时恒 false） */
  biometric_enabled: boolean
  /** 生物识别方式：'both'（指纹+人脸）/ 'fingerprint'（仅指纹）/ 'face'（人脸优先） */
  biometric_mode: BiometricMode
}

export interface VerifyResult {
  ok: boolean
  locked: boolean
  lock_remaining_sec: number
}

/** 随 prefs 同步到云端的策略文档（不含锁定信息） */
export interface SecurityPolicySync {
  enabled: boolean
  scopes: SecurityScopes
  salt: string
  pin_hash: string
  updated_at: number
}

export type ScopeKey = keyof SecurityScopes

export function securityStatus(): Promise<SecurityStatus> {
  return invoke<SecurityStatus>('security_status')
}
export function securitySetPassword(pw: string, scopes: SecurityScopes): Promise<void> {
  return invoke('security_set_password', { password: pw, scopes })
}
export function securityVerify(pw: string): Promise<VerifyResult> {
  return invoke<VerifyResult>('security_verify', { password: pw })
}
export function securityDisable(pw: string): Promise<void> {
  return invoke('security_disable', { password: pw })
}
export function securityUpdateScopes(pw: string, scopes: SecurityScopes): Promise<void> {
  return invoke('security_update_scopes', { password: pw, scopes })
}
/** 开启 / 关闭生物识别解锁、或修改方式：后端强制校验 PIN（改任意一项都要验） */
export function securitySetBiometric(pw: string, enabled: boolean, mode?: BiometricMode): Promise<void> {
  return invoke('security_set_biometric', { password: pw, enabled, mode: mode ?? null })
}
export function securityWipeAndDisable(): Promise<void> {
  return invoke('security_wipe_and_disable')
}
/** 读取待上云的策略（未开启返回 null） */
export function securityGetSync(): Promise<SecurityPolicySync | null> {
  return invoke<SecurityPolicySync | null>('security_get_sync')
}
/** 应用云端下发的策略（null = 移除 PIN） */
export function securitySetSync(policy: SecurityPolicySync | null): Promise<boolean> {
  return invoke<boolean>('security_set_sync', { policy })
}

function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms))
}
async function safeStatus(): Promise<SecurityStatus | null> {
  try {
    return await securityStatus()
  } catch {
    return null // 后端未就绪，软降级放行（锁定为前端提示，非硬安全边界）
  }
}

/**
 * 确保已通过安全密码验证（用于受保护操作的入口）。
 * - 未设密码 / 该操作不在保护范围内 → 直接放行（true）
 * - 已锁定 → 提示倒计时并等待，循环直到解锁
 * - 验证失败 → 提示错误并重新要求输入
 */
/** 各保护范围对应的验证提示文案 */
const SCOPE_MESSAGES: Partial<Record<ScopeKey, string>> = {
  view_password: '查看会话密码需要验证安全密码',
  export_data: '导出会话数据需要验证安全密码',
  export_password: '导出会话密码需要验证安全密码',
}

export async function ensureUnlocked(scope?: ScopeKey): Promise<boolean> {
  let status: SecurityStatus | null = await safeStatus()
  if (!status || !status.has_password) return true
  if (scope && !status.scopes[scope]) return true

  // 生物识别优先：已开启时先弹系统指纹 / 人脸，通过即放行。
  // 未通过时**不自动**弹安全密码框 —— 只有用户在选择器里明确点「使用安全密码」（'pin'）
  // 才进入下面的 PIN 校验；用户取消（'cancel'）则本次操作直接终止。
  // 否则会出现「刷脸 / 刷指纹时就蹦出密码输入框」的观感。
  if (status.biometric_enabled) {
    const reason = (scope && SCOPE_MESSAGES[scope]) || '此操作需要验证安全密码'
    const outcome = await biometricUnlock(reason, status.biometric_mode)
    if (outcome === 'ok') return true
    if (outcome === 'cancel') return false
  }

  while (true) {
    const st = await safeStatus()
    if (st?.locked) {
      ElMessage.warning(`密码错误次数过多，已锁定 ${st.lock_remaining_sec} 秒，请稍后再试`)
      await sleep((st.lock_remaining_sec + 1) * 1000)
      continue
    }
    const res = await openVerify({
      message: (scope && SCOPE_MESSAGES[scope]) || '此操作需要验证安全密码',
      showWipe: false,
    })
    if (res.action === 'cancel') return false
    const r = await securityVerify(res.pin)
    if (r.ok) return true
    if (r.locked) {
      ElMessage.error(`密码错误次数过多，已锁定 ${r.lock_remaining_sec} 秒`)
      continue
    }
    ElMessage.error('安全密码错误')
  }
}
