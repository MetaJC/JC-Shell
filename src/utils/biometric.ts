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
 * 生物识别解锁（系统指纹 / 人脸）前端封装。
 * 作者：JC
 *
 * 底层是 tauri-plugin-biometric（`src-tauri/Cargo.toml` 中仅对 Android/iOS 引入，
 * lib.rs 里 `#[cfg(mobile)]` 注册；本仓库对其打了补丁，见下方第 2 条）。
 * 桌面端没有该插件，所有调用安全降级为「不可用」并返回 false，调用方回落 PIN。
 *
 * 两个必须在本文件里消化掉的 Android 约束：
 *
 * 1) 弹窗是**独立 Activity**：弹出/关闭会让宿主 Activity onPause/onResume，表现为一次
 *    「切出应用又回来」的 visibilitychange。若不屏蔽，MainWindow 的「切出应用自动锁定」
 *    会在解锁过程中把应用再次上锁 → 锁屏再次触发生物识别 → **无限重复验证**。
 *    故这里维护 isBiometricPromptActive()，并让锁定侧跳过。
 *
 * 2) BiometricPrompt 没有「指定模态」的能力，只能选认证强度：
 *    BIOMETRIC_STRONG 排除了只属于 WEAK 的 class-2 人脸 → 对这类机型等价于「只用指纹」；
 *    BIOMETRIC_WEAK = 指纹 + 人脸（弹哪个由系统 / ROM 决定）。
 *    因此「只用指纹」「人脸失败后改用指纹」可精确实现，「仅人脸」无法强制（上游插件
 *    硬编码 WEAK，本仓库 fork 扩展了 allowedAuthenticators 参数）。
 */
import { checkStatus, authenticate, type AuthOptions } from '@tauri-apps/plugin-biometric'
import { CAP } from './platform'
import { openBiometricMethodDialog } from './biometricDialog'
import { tracePinBlack } from './pinBlackTrace'

/** 用户选择的生物识别方式（后端 biometric_mode） */
export type BiometricMode = 'both' | 'fingerprint' | 'face'
/** 具体模态 */
export type BiometricKind = 'fingerprint' | 'face'
/**
 * 认证强度 / 模态：
 *  - 'fingerprint'：**只认指纹**（原生走 FingerprintManager，真正排除人脸）
 *  - 'strong'：仅强生物识别（对 class-2 人脸机型等价于只用指纹；class-3 人脸仍满足）
 *  - 'weak'：指纹 + 人脸，弹哪个由系统决定
 */
type Strength = 'weak' | 'fingerprint' | 'strong'

/**
 * 生物识别主流程的结果：
 *  - 'ok'     ：系统验证已通过
 *  - 'pin'    ：用户在选择器里**明确点选**了「使用安全密码」→ 调用方可以弹 PIN 输入框
 *  - 'cancel' ：用户放弃（关掉系统验证后又取消选择器 / 并发调用被拒）→ 调用方**不得**弹 PIN
 *
 * 之所以要区分后两者：调用方只有看到 'pin' 才允许弹安全密码框。若只返回 boolean，
 * 「取消」与「选安全密码」无法区分，就会出现「刷脸 / 刷指纹时密码框自己蹦出来」的观感。
 */
export type BiometricOutcome = 'ok' | 'pin' | 'cancel'

/** 单次系统验证的结果：'ok' 通过；'fail' 取消 / 失败；'busy' 已有验证在进行（本次未发起） */
type PromptResult = 'ok' | 'fail' | 'busy'

export interface BiometricAvailability {
  /** 可用（有硬件 + 已录入 + 未被系统锁定） */
  available: boolean
  /** 有硬件但用户还没录入（UI 要引导去系统设置录入） */
  notEnrolled: boolean
  /** 本机确实具备的模态；拿不到时为空数组（调用方按「都可能有」兜底，不要据此屏蔽选项） */
  kinds: BiometricKind[]
  /** 本机能否做到「只认指纹」的精确限制（原生走 FingerprintManager，见 utils/biometric.ts 头注释） */
  fingerprintOnly: boolean
  /**
   * 原生扩展是否生效：`biometryTypes` 只有本仓库 fork 的原生代码才会回传。
   * Android 上拿不到它就说明跑的是**未打补丁的插件**（认证强度参数会被忽略 →
   * 选了「仅指纹」也不会生效）。设置页据此给出可自证的提示，免去盲猜。
   */
  extensionActive: boolean
}

const UNAVAILABLE: BiometricAvailability = {
  available: false, notEnrolled: false, kinds: [], extensionActive: false, fingerprintOnly: false,
}

/** 插件 status 的原始返回（含 fork 扩展字段 biometryTypes） */
interface RawStatus {
  isAvailable?: boolean
  errorCode?: string
  biometryType?: number
  /** fork 扩展：全部模态 1=指纹 2=人脸 3=虹膜 */
  biometryTypes?: number[]
  /** fork 扩展：本机能否做到「只认指纹」（走 FingerprintManager） */
  fingerprintOnly?: boolean
}

/** 插件模态编号 → 本项目模态名（3=虹膜不参与，忽略） */
function mapKinds(list: number[]): BiometricKind[] {
  const out: BiometricKind[] = []
  if (list.includes(1)) out.push('fingerprint')
  if (list.includes(2)) out.push('face')
  return out
}

/** 查询本机生物识别是否可用（每次都实时问系统：用户可能刚去系统设置里录了指纹） */
export async function biometricAvailability(): Promise<BiometricAvailability> {
  if (!CAP.biometricUnlock()) return UNAVAILABLE
  try {
    const st = (await checkStatus()) as RawStatus
    return {
      available: !!st.isAvailable,
      notEnrolled: st.errorCode === 'biometryNotEnrolled',
      kinds: mapKinds(st.biometryTypes ?? (st.biometryType ? [st.biometryType] : [])),
      extensionActive: Array.isArray(st.biometryTypes),
      fingerprintOnly: st.fingerprintOnly === true,
    }
  } catch {
    // 插件未注册（桌面）/ 权限未放开 / 原生异常：一律当作不可用
    return UNAVAILABLE
  }
}

// ===== 弹窗活跃标记（供「切出应用自动锁定」避让）=====
let prompting = false
/** 弹窗关闭后的尾窗：Activity 恢复的 visibilitychange 与本次结果几乎同时到达，用它吞掉 */
let promptTailUntil = 0
const PROMPT_TAIL_MS = 1500

/**
 * 是否有生物识别弹窗正在进行（含结束后 1.5s 尾窗）。
 * MainWindow 的「切出应用自动锁定」必须据此跳过，否则会把弹窗误判成「用户切出去又回来」，
 * 从而在解锁过程中再次上锁 → 再次弹窗 → 无限重复验证。桌面端恒为 false。
 */
export function isBiometricPromptActive(): boolean {
  return prompting || Date.now() < promptTailUntil
}

/** 弹一次系统验证；取消 / 失败返回 'fail'，已有验证在进行返回 'busy'（调用方放弃本次流程） */
async function promptBiometric(reason: string, strength: Strength): Promise<PromptResult> {
  if (!CAP.biometricUnlock()) return 'busy'
  // 串行化：已经在弹窗中就不再叠加一个（插件每次都会启动一个 Activity，叠加会出现
  // 「刚验完又弹一次」的观感），直接返回 'busy' 让调用方放弃本次流程（绝不落到 PIN）。
  if (prompting) return 'busy'
  prompting = true
  // 【临时】黑屏诊断：原生弹窗是一次 Activity 跳转，只有 start 没有 end 即说明原生页没回来
  tracePinBlack(`bio:start ${strength}`)
  try {
    await authenticate(reason, {
      // Android 弹框标题：不传会落到插件内置的英文默认值（"Fingerprint Authentication"）。
      // 按强度给出不同标题，它同时也是**现场可自证的信号**：选「只用指纹」时若标题仍是
      // 「需要验证身份」，说明强度没被原生接收（详见文件头第 2 条）。
      title: strength === 'weak' ? '需要验证身份' : '请使用指纹验证',
      // 只认生物识别：系统锁屏密码不作数，兜底用我们自己的安全密码（PIN），
      // 否则会出现「第二个密码」把用户绕晕，也绕开本应用的保护范围设置。
      allowDeviceCredential: false,
      // 人脸识别后不再要求额外点一次「确认」，少一次点击
      confirmationRequired: false,
      // fork 扩展字段：strong = 仅强生物识别（≈只用指纹）
      allowedAuthenticators: strength,
    } as AuthOptions)
    tracePinBlack('bio:end ok')
    return 'ok'
  } catch {
    tracePinBlack('bio:end fail')
    return 'fail'
  } finally {
    prompting = false
    promptTailUntil = Date.now() + PROMPT_TAIL_MS
  }
}

/** 方式 → 首选强度：限定只用指纹时走 'fingerprint'（真排除人脸），其余用 'weak'（可用人脸） */
export function preferredStrength(mode: BiometricMode): Strength {
  return mode === 'fingerprint' ? 'fingerprint' : 'weak'
}

/** 用户设置允许、且本机确实具备的模态（拿不到本机信息时保守认为两种都在） */
async function candidateKinds(mode: BiometricMode): Promise<BiometricKind[]> {
  const allowed: BiometricKind[] = mode === 'both' ? ['fingerprint', 'face'] : [mode]
  const { kinds } = await biometricAvailability()
  return kinds.length ? allowed.filter(k => kinds.includes(k)) : allowed
}

/**
 * 生物识别解锁主流程（锁屏与所有敏感操作统一入口）：
 *   1) 按用户设置的方式弹一次系统验证 → 通过即返回 'ok'
 *   2) 未通过 → 弹「换一种方式」选择器：使用指纹 / 使用人脸 / 使用安全密码 / 取消
 *      （本机没有可切换的模态时选择器只剩「使用安全密码 / 取消」，但仍由用户点选）
 *   3) 选了某种模态 → 用对应强度再试一次；再失败则回到第 2 步
 *   4) 选「使用安全密码」→ 'pin'；取消 → 'cancel'
 *
 * ⚠️ 本函数只回答「用户是否**明确**选择了安全密码」，绝不在其它情况下返回 'pin'：
 *    调用方据此决定是否弹 PIN 输入框，避免刷脸 / 刷指纹失败时密码框自己蹦出来。
 */
export async function biometricUnlock(reason: string, mode: BiometricMode): Promise<BiometricOutcome> {
  if (!CAP.biometricUnlock()) return 'cancel'

  let strength: Strength = preferredStrength(mode)
  for (;;) {
    const r = await promptBiometric(reason, strength)
    if (r === 'ok') return 'ok'
    // 已有验证在进行（并发调用）：不插队、也不弹 PIN，直接结束本次流程
    if (r === 'busy') return 'cancel'

    // 失败 / 取消后提供切换：这正是「人脸识别不出来就改用指纹 / 密码」的落点。
    const kinds = await candidateKinds(mode)
    const pick = await openBiometricMethodDialog({ kinds })
    if (pick === 'pin') return 'pin'
    if (pick === null) return 'cancel'
    // 「使用指纹」走 'fingerprint'（真排除人脸），而不是 'strong'（对强级人脸无效）
    strength = pick === 'fingerprint' ? 'fingerprint' : 'weak'
  }
}
